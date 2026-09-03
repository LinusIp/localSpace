//! Hardware profiles and model profiles.
//!
//! Every budget in the system comes from a profile rather than a constant, so a
//! smaller tier later is a profile plus a plan, not a redesign. Detection is
//! best-effort and dependency-free; anything it cannot see comes from config.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Machine {
    /// VRAM per GPU, in GB, in device order.
    pub gpus: Vec<u32>,
    pub ram_gb: u32,
    pub cores: u32,
    /// Sequential NVMe read, GB/s. From config; detection is unreliable.
    pub nvme_gbps: f32,
    /// Host-to-device bandwidth, GB/s. PCIe 4.0 x16 ~26, 5.0 x16 ~52 measured.
    pub pcie_gbps: f32,
    pub avx512: bool,
    pub amx: bool,
    /// True for Apple-silicon style unified memory: no PCIe hop to pay.
    pub unified_memory: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Machine {
            gpus: Vec::new(),
            ram_gb: 16,
            cores: 8,
            nvme_gbps: 5.0,
            pcie_gbps: 26.0,
            avx512: false,
            amx: false,
            unified_memory: false,
        }
    }
}

impl Machine {
    pub fn total_vram_gb(&self) -> u32 {
        self.gpus.iter().sum()
    }

    pub fn largest_gpu_gb(&self) -> u32 {
        self.gpus.iter().copied().max().unwrap_or(0)
    }

    /// Which profile this machine satisfies.
    pub fn tier(&self) -> HardwareTier {
        let vram = self.total_vram_gb();
        let big_gpus = self.gpus.iter().filter(|g| **g >= 70).count();
        if big_gpus >= 4 && self.ram_gb >= 480 {
            HardwareTier::S
        } else if vram >= 96 && self.ram_gb >= 120 {
            HardwareTier::W96
        } else if vram >= 30 && self.ram_gb >= 60 && self.cores >= 16 {
            HardwareTier::W32
        } else {
            HardwareTier::BelowFloor
        }
    }

    /// Best-effort detection. Everything it cannot see keeps its configured value.
    pub fn detect() -> Machine {
        let mut m = Machine {
            cores: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(8),
            ..Default::default()
        };
        if let Some(ram) = detect_ram_gb() {
            m.ram_gb = ram;
        }
        m.gpus = detect_gpus();
        if cfg!(target_os = "macos") && m.gpus.is_empty() {
            // Apple silicon: the GPU shares system memory, so most of RAM is VRAM.
            m.unified_memory = true;
            m.gpus = vec![(m.ram_gb as f32 * 0.7) as u32];
            m.pcie_gbps = 400.0;
        }
        m
    }

    pub fn describe(&self) -> String {
        let gpus = if self.gpus.is_empty() {
            "no GPU detected".to_string()
        } else {
            format!(
                "{} GPU(s): {}",
                self.gpus.len(),
                self.gpus
                    .iter()
                    .map(|g| format!("{g} GB"))
                    .collect::<Vec<_>>()
                    .join(" + ")
            )
        };
        format!(
            "{} — {gpus}, {} GB RAM, {} cores",
            self.tier().label(),
            self.ram_gb,
            self.cores
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardwareTier {
    BelowFloor,
    W32,
    W96,
    S,
}

impl HardwareTier {
    pub fn label(self) -> &'static str {
        match self {
            HardwareTier::BelowFloor => "below floor",
            HardwareTier::W32 => "W32 (single-GPU workstation)",
            HardwareTier::W96 => "W96 (multi-GPU workstation)",
            HardwareTier::S => "S (organisation server)",
        }
    }

    /// `serve` refuses below S without `--allow-below-floor`; W32 is "team mode".
    pub fn may_serve(self) -> bool {
        self >= HardwareTier::S
    }

    pub fn team_mode(self) -> bool {
        self == HardwareTier::W32 || self == HardwareTier::W96
    }
}

fn detect_ram_gb() -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some((kb / 1024 / 1024) as u32);
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some((bytes / 1024 / 1024 / 1024) as u32)
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ])
            .output()
            .ok()?;
        let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some((bytes / 1024 / 1024 / 1024) as u32)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn detect_gpus() -> Vec<u32> {
    let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .map(|mib| mib / 1024)
        .collect()
}

// ---------------------------------------------------------------------------
// Model profile — where every agent-side budget comes from
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub name: String,
    /// Total tool-description tokens allowed in context (spec §9).
    pub tool_budget_tokens: usize,
    /// Bounded working set the big model sees, regardless of conversation length.
    pub working_set_tokens: usize,
    /// Total budget shared across all context-provider blocks in a turn.
    pub context_budget_tokens: usize,
    /// Budget handed to the focused harness's provider.
    pub focused_context_tokens: usize,
    /// Budget handed to each pinned harness's provider.
    pub pinned_context_tokens: usize,
    /// Prompt-token ceiling per agent step, checked by the bench.
    pub prompt_tokens_per_step: usize,
}

impl ModelProfile {
    /// 70B-class dense or GPU-resident MoE: room to breathe.
    pub fn server() -> ModelProfile {
        ModelProfile {
            name: "S/W96 (70B-class reference)".into(),
            tool_budget_tokens: 4000,
            working_set_tokens: 24_000,
            context_budget_tokens: 3000,
            focused_context_tokens: 1500,
            pinned_context_tokens: 400,
            prompt_tokens_per_step: 6000,
        }
    }

    /// Hybrid MoE on one GPU: every prompt token is paid for on a PCIe-bound machine.
    pub fn w32() -> ModelProfile {
        ModelProfile {
            name: "W32 (100B+ MoE, hybrid)".into(),
            tool_budget_tokens: 2500,
            working_set_tokens: 16_000,
            context_budget_tokens: 2000,
            focused_context_tokens: 1000,
            pinned_context_tokens: 300,
            prompt_tokens_per_step: 4000,
        }
    }

    /// Kept because the spec requires a smaller tier to remain a profile, not a rewrite.
    pub fn small() -> ModelProfile {
        ModelProfile {
            name: "small model".into(),
            tool_budget_tokens: 1500,
            working_set_tokens: 8000,
            context_budget_tokens: 1200,
            focused_context_tokens: 600,
            pinned_context_tokens: 200,
            prompt_tokens_per_step: 2500,
        }
    }

    pub fn for_tier(tier: HardwareTier) -> ModelProfile {
        match tier {
            HardwareTier::S | HardwareTier::W96 => ModelProfile::server(),
            HardwareTier::W32 => ModelProfile::w32(),
            HardwareTier::BelowFloor => ModelProfile::small(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(gpus: Vec<u32>, ram_gb: u32, cores: u32) -> Machine {
        Machine {
            gpus,
            ram_gb,
            cores,
            ..Default::default()
        }
    }

    #[test]
    fn the_reference_machines_classify_as_their_profiles() {
        assert_eq!(machine(vec![32], 64, 16).tier(), HardwareTier::W32);
        assert_eq!(machine(vec![96, 96], 128, 32).tier(), HardwareTier::W96);
        assert_eq!(
            machine(vec![80, 80, 80, 80], 512, 64).tier(),
            HardwareTier::S
        );
    }

    #[test]
    fn an_ordinary_laptop_is_below_the_floor() {
        assert_eq!(machine(vec![8], 16, 8).tier(), HardwareTier::BelowFloor);
        assert_eq!(machine(vec![], 32, 12).tier(), HardwareTier::BelowFloor);
    }

    #[test]
    fn only_a_server_may_serve_unforced() {
        assert!(HardwareTier::S.may_serve());
        assert!(!HardwareTier::W32.may_serve());
        assert!(HardwareTier::W32.team_mode());
        assert!(!HardwareTier::BelowFloor.team_mode());
    }

    #[test]
    fn budgets_come_from_the_profile_not_a_constant() {
        assert_eq!(
            ModelProfile::for_tier(HardwareTier::W32).tool_budget_tokens,
            2500
        );
        assert_eq!(
            ModelProfile::for_tier(HardwareTier::S).tool_budget_tokens,
            4000
        );
        assert_eq!(ModelProfile::small().tool_budget_tokens, 1500);
        // W32 is tighter than S on every agent-side budget.
        let w = ModelProfile::w32();
        let s = ModelProfile::server();
        assert!(w.working_set_tokens < s.working_set_tokens);
        assert!(w.prompt_tokens_per_step < s.prompt_tokens_per_step);
    }

    #[test]
    fn detection_never_panics_on_this_machine() {
        let m = Machine::detect();
        assert!(m.cores >= 1);
        assert!(!m.describe().is_empty());
    }
}
