//! The placement planner (spec §11.1).
//!
//! Input: a model's tensor map, the machine, and the request. Output: where every
//! tensor lives, KV precision, expert-cache size, an estimated tok/s and a verdict.
//! The catalog shows the verdict and the estimate *for this machine* before the
//! download button, so nobody downloads 60 GB to discover it does not fit.
//!
//! The throughput number is a roofline estimate: bytes that must move per decoded
//! token divided by the bandwidth of the slowest path they move over. It is
//! deliberately conservative and is meant to be replaced, per machine, by the
//! micro-benchmark `localspace bench` records at install.

use crate::profile::Machine;
use serde::{Deserialize, Serialize};

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// What a model weighs and how it is shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorMap {
    pub model_id: String,
    /// Everything that must be GPU-resident regardless of placement:
    /// embeddings, attention, norms, router, shared experts, LM head.
    pub core_bytes: u64,
    /// Routed-expert weights, total across all layers. Zero for a dense model.
    pub routed_expert_bytes: u64,
    pub layers: u32,
    pub moe: Option<MoeLayout>,
    /// KV cache bytes per token at FP16, both K and V, all layers.
    pub kv_bytes_per_token_fp16: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MoeLayout {
    pub experts_per_layer: u32,
    /// Experts actually evaluated per token per layer.
    pub active_experts: u32,
    pub shared_experts: u32,
}

impl TensorMap {
    pub fn total_bytes(&self) -> u64 {
        self.core_bytes + self.routed_expert_bytes
    }

    /// Size of one routed expert's weights.
    pub fn expert_bytes(&self) -> u64 {
        match self.moe {
            Some(m) if m.experts_per_layer > 0 && self.layers > 0 => {
                self.routed_expert_bytes / (m.experts_per_layer as u64 * self.layers as u64)
            }
            _ => 0,
        }
    }

    /// Routed-expert bytes touched per decoded token, before any caching.
    pub fn expert_bytes_per_token(&self) -> u64 {
        match self.moe {
            Some(m) => self.expert_bytes() * m.active_experts as u64 * self.layers as u64,
            None => 0,
        }
    }

    pub fn is_moe(&self) -> bool {
        self.moe.is_some() && self.routed_expert_bytes > 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRequest {
    pub context_len: u32,
    pub batch: u32,
    /// VRAM the planner must leave alone: draft model, utility model, VLM, harness pool.
    pub reservations: Vec<Reservation>,
    /// Prefer FP8/Q8 KV to leave room for experts.
    pub kv_quantized: bool,
}

impl Default for PlanRequest {
    fn default() -> Self {
        PlanRequest {
            context_len: 16384,
            batch: 1,
            reservations: Vec::new(),
            kv_quantized: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reservation {
    pub name: String,
    pub bytes: u64,
}

impl Reservation {
    pub fn gb(name: &str, gb: f64) -> Reservation {
        Reservation {
            name: name.into(),
            bytes: (gb * GB) as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Every weight on the GPU.
    Resident,
    /// Core on GPU, some experts cached on GPU, the rest in RAM.
    Hybrid,
    /// Part of the expert set does not fit in RAM and streams from NVMe.
    Streaming,
    DoesNotFit,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Resident => "resident",
            Verdict::Hybrid => "hybrid",
            Verdict::Streaming => "streaming",
            Verdict::DoesNotFit => "does not fit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementPlan {
    pub model_id: String,
    pub verdict: Verdict,
    pub gpu_resident_bytes: u64,
    pub kv_cache_bytes: u64,
    pub kv_precision: &'static str,
    /// Routed experts held on the GPU.
    pub hot_expert_cache_bytes: u64,
    pub hot_expert_fraction: f32,
    /// Routed experts in pinned host RAM.
    pub ram_expert_bytes: u64,
    /// Routed experts that must stream from NVMe.
    pub nvme_expert_bytes: u64,
    pub reservations: Vec<Reservation>,
    pub estimated_tok_s: f32,
    pub first_token_ms: f32,
    /// Whether routed experts are computed on the CPU rather than copied to the GPU.
    pub cpu_expert_compute: bool,
    pub notes: Vec<String>,
}

impl PlacementPlan {
    pub fn interactive(&self) -> bool {
        self.estimated_tok_s >= 15.0
    }

    pub fn summary(&self) -> String {
        format!(
            "{} — ~{:.0} tok/s, {:.0}% of experts cached on GPU, first token ~{:.1}s",
            self.verdict.label(),
            self.estimated_tok_s,
            self.hot_expert_fraction * 100.0,
            self.first_token_ms / 1000.0
        )
    }
}

/// Plan where a model's tensors live on this machine.
pub fn plan(map: &TensorMap, machine: &Machine, req: &PlanRequest) -> PlacementPlan {
    let mut notes = Vec::new();

    let vram = machine.largest_gpu_gb() as f64 * GB;
    let multi_gpu_vram = machine.total_vram_gb() as f64 * GB;
    let reserved: u64 = req.reservations.iter().map(|r| r.bytes).sum();

    // 1. GPU-resident always: core tensors and the KV cache.
    let (kv_bytes, kv_precision) = kv_cache(map, req);
    let must_reside = map.core_bytes + kv_bytes;

    // Tensor-parallel groups pool their VRAM; a single GPU does not.
    let usable = if machine.gpus.len() > 1 {
        multi_gpu_vram
    } else {
        vram
    } - reserved as f64;

    if usable <= 0.0 || (must_reside as f64) > usable {
        notes.push(format!(
            "needs {:.1} GB for weights and KV before any experts; {:.1} GB is free after reservations",
            must_reside as f64 / GB,
            usable.max(0.0) / GB
        ));
        return PlacementPlan {
            model_id: map.model_id.clone(),
            verdict: Verdict::DoesNotFit,
            gpu_resident_bytes: 0,
            kv_cache_bytes: kv_bytes,
            kv_precision,
            hot_expert_cache_bytes: 0,
            hot_expert_fraction: 0.0,
            ram_expert_bytes: 0,
            nvme_expert_bytes: 0,
            reservations: req.reservations.clone(),
            estimated_tok_s: 0.0,
            first_token_ms: 0.0,
            cpu_expert_compute: false,
            notes,
        };
    }

    // 2. Hot-expert cache: whatever VRAM is left after core weights and KV.
    let free_for_experts = (usable - must_reside as f64).max(0.0);
    let hot = free_for_experts.min(map.routed_expert_bytes as f64) as u64;
    let hot_fraction = if map.routed_expert_bytes > 0 {
        hot as f32 / map.routed_expert_bytes as f32
    } else {
        1.0
    };

    // 3. What is left goes to RAM, then 4. NVMe.
    let overflow = map.routed_expert_bytes.saturating_sub(hot);
    // Weights are memory-mapped and RAM is their page cache, not a second copy,
    // so nearly all of RAM is available to hold experts — minus a fixed reserve
    // for the OS, Core itself and Tier B harness processes.
    let base_reserve_gb = (machine.ram_gb as f64 / 8.0).max(12.0);
    let ram_for_experts = ((machine.ram_gb as f64 - base_reserve_gb).max(0.0) * GB) as u64;
    let ram_experts = overflow.min(ram_for_experts);
    let nvme_experts = overflow.saturating_sub(ram_experts);

    let verdict = if overflow == 0 {
        Verdict::Resident
    } else if nvme_experts == 0 {
        Verdict::Hybrid
    } else {
        Verdict::Streaming
    };

    // 3b. Copy or compute? Measured per machine at install; this is the default rule.
    // An expert copied over PCIe costs bytes/pcie_bandwidth; computed on the CPU it
    // costs roughly bytes/memory_bandwidth with AVX-512, and more without.
    let cpu_bandwidth = cpu_expert_bandwidth(machine);
    let cpu_expert_compute = !machine.unified_memory && cpu_bandwidth > machine.pcie_gbps as f64;
    if map.is_moe() && overflow > 0 {
        notes.push(if cpu_expert_compute {
            format!(
                "routed experts computed on the CPU (~{cpu_bandwidth:.0} GB/s effective) rather than copied over PCIe (~{:.0} GB/s)",
                machine.pcie_gbps
            )
        } else {
            format!(
                "routed experts copied to the GPU over PCIe (~{:.0} GB/s), prefetched one layer ahead",
                machine.pcie_gbps
            )
        });
    }

    let (tok_s, first_token_ms) = estimate_throughput(
        map,
        machine,
        hot_fraction,
        nvme_experts,
        cpu_expert_compute,
        req,
    );

    if verdict == Verdict::Streaming {
        notes.push(format!(
            "{:.1} GB of experts stream from NVMe; the estimate below is honest, not optimistic",
            nvme_experts as f64 / GB
        ));
    }
    if !map.is_moe() && overflow > 0 {
        notes
            .push("dense model with layer offload — the catalog steers this machine to MoE".into());
    }
    if hot_fraction < 1.0 && map.is_moe() {
        notes.push(
            "expert usage statistics are persisted per model and pre-warm this cache on the next load"
                .into(),
        );
    }

    PlacementPlan {
        model_id: map.model_id.clone(),
        verdict,
        gpu_resident_bytes: map.core_bytes + hot,
        kv_cache_bytes: kv_bytes,
        kv_precision,
        hot_expert_cache_bytes: hot,
        hot_expert_fraction: hot_fraction,
        ram_expert_bytes: ram_experts,
        nvme_expert_bytes: nvme_experts,
        reservations: req.reservations.clone(),
        estimated_tok_s: tok_s,
        first_token_ms,
        cpu_expert_compute,
        notes,
    }
}

fn kv_cache(map: &TensorMap, req: &PlanRequest) -> (u64, &'static str) {
    let per_token = if req.kv_quantized {
        map.kv_bytes_per_token_fp16 / 2
    } else {
        map.kv_bytes_per_token_fp16
    };
    let bytes = per_token * req.context_len as u64 * req.batch.max(1) as u64;
    (bytes, if req.kv_quantized { "q8/fp8" } else { "fp16" })
}

/// Effective bandwidth for evaluating an expert on the CPU, GB/s.
/// Expert GEMV is memory-bound, so this is DRAM bandwidth derated by ISA.
fn cpu_expert_bandwidth(machine: &Machine) -> f64 {
    let base = if machine.cores >= 32 { 120.0 } else { 75.0 };
    let isa = if machine.amx {
        1.0
    } else if machine.avx512 {
        0.85
    } else {
        0.55
    };
    base * isa
}

/// Roofline: bytes that must move per token / bandwidth of the path they move over.
fn estimate_throughput(
    map: &TensorMap,
    machine: &Machine,
    hot_fraction: f32,
    nvme_bytes: u64,
    cpu_expert_compute: bool,
    req: &PlanRequest,
) -> (f32, f32) {
    // On-GPU weights read once per token at HBM speed. Conservative for the
    // 5090/H100 class; the bench replaces it with a measured number.
    let hbm_gbps = 1200.0_f64;

    let core_s = map.core_bytes as f64 / (hbm_gbps * GB);

    let expert_per_token = map.expert_bytes_per_token() as f64;
    let hot_share = expert_per_token * hot_fraction as f64;
    let cold_share = expert_per_token - hot_share;

    let hot_s = hot_share / (hbm_gbps * GB);

    // Cold experts either cross PCIe or are evaluated on the CPU.
    let cold_bandwidth = if cpu_expert_compute {
        cpu_expert_bandwidth(machine)
    } else {
        machine.pcie_gbps as f64
    };
    let cold_s = if cold_share > 0.0 {
        cold_share / (cold_bandwidth * GB)
    } else {
        0.0
    };

    // The NVMe-resident slice is hit in proportion to how much of the cold set it is.
    let nvme_s = if nvme_bytes > 0 && map.routed_expert_bytes > 0 {
        let nvme_share = nvme_bytes as f64 / map.routed_expert_bytes as f64;
        cold_share * nvme_share / (machine.nvme_gbps as f64 * GB)
    } else {
        0.0
    };

    let per_token_s = core_s + hot_s + cold_s + nvme_s;
    let tok_s = if per_token_s > 0.0 {
        (1.0 / per_token_s) as f32
    } else {
        0.0
    };

    // Prefill reads every weight the prompt touches once; approximate as the core
    // weights plus the whole hot cache, at HBM speed, plus a fixed launch cost.
    let prefill_s = (map.core_bytes as f64
        + (map.routed_expert_bytes as f64 * hot_fraction as f64))
        / (hbm_gbps * GB);
    let prompt_tokens = req.context_len.min(8192) as f64;
    let first_token_ms =
        ((prefill_s + prompt_tokens / (tok_s.max(1.0) as f64 * 12.0)) * 1000.0 + 120.0) as f32;

    (tok_s, first_token_ms)
}

// ---------------------------------------------------------------------------
// Reference tensor maps — the models the design is validated against
// ---------------------------------------------------------------------------

/// A 100B+-class MoE at Q4, the W32 reference (Qwen3-family, gpt-oss-120b).
pub fn reference_moe_100b_q4() -> TensorMap {
    TensorMap {
        model_id: "reference-100b-moe-q4".into(),
        // Embeddings, attention, norms, router, shared experts, LM head.
        core_bytes: (11.0 * GB) as u64,
        routed_expert_bytes: (52.0 * GB) as u64,
        layers: 48,
        moe: Some(MoeLayout {
            experts_per_layer: 128,
            active_experts: 8,
            shared_experts: 1,
        }),
        kv_bytes_per_token_fp16: 192 * 1024,
    }
}

/// A 70B-class dense model at FP8, the W96/S reference.
pub fn reference_dense_70b_fp8() -> TensorMap {
    TensorMap {
        model_id: "reference-70b-dense-fp8".into(),
        core_bytes: (70.0 * GB) as u64,
        routed_expert_bytes: 0,
        layers: 80,
        moe: None,
        kv_bytes_per_token_fp16: 320 * 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Machine;

    fn w32() -> Machine {
        Machine {
            gpus: vec![32],
            ram_gb: 64,
            cores: 16,
            nvme_gbps: 5.0,
            pcie_gbps: 52.0,
            avx512: true,
            amx: false,
            unified_memory: false,
        }
    }

    fn w96() -> Machine {
        Machine {
            gpus: vec![96, 96],
            ram_gb: 128,
            cores: 32,
            pcie_gbps: 52.0,
            avx512: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_100b_moe_on_w32_is_hybrid_and_interactive() {
        let plan = plan(
            &reference_moe_100b_q4(),
            &w32(),
            &PlanRequest {
                context_len: 16384,
                reservations: vec![
                    Reservation::gb("harness pool", 6.0),
                    Reservation::gb("draft model", 2.0),
                    Reservation::gb("utility model", 2.5),
                ],
                ..Default::default()
            },
        );
        assert_eq!(plan.verdict, Verdict::Hybrid, "{:?}", plan.notes);
        // The gate in the spec: >= 15 tok/s single stream on the W32 reference.
        assert!(
            plan.interactive(),
            "estimate was {:.1} tok/s: {}",
            plan.estimated_tok_s,
            plan.summary()
        );
        // Core weights and some experts are on the GPU; the rest is in RAM, not NVMe.
        assert!(plan.hot_expert_cache_bytes > 0);
        assert!(plan.ram_expert_bytes > 0);
        assert_eq!(plan.nvme_expert_bytes, 0);
    }

    #[test]
    fn reservations_shrink_the_expert_cache_rather_than_the_model() {
        let map = reference_moe_100b_q4();
        let bare = plan(&map, &w32(), &PlanRequest::default());
        let with_sim = plan(
            &map,
            &w32(),
            &PlanRequest {
                reservations: vec![Reservation::gb("simulation", 10.0)],
                ..Default::default()
            },
        );
        assert!(with_sim.hot_expert_cache_bytes < bare.hot_expert_cache_bytes);
        // The model is never unloaded; it just gets slower.
        assert_ne!(with_sim.verdict, Verdict::DoesNotFit);
        assert!(with_sim.estimated_tok_s < bare.estimated_tok_s);
    }

    #[test]
    fn a_70b_dense_is_resident_on_w96_and_does_not_fit_on_w32() {
        let map = reference_dense_70b_fp8();
        assert_eq!(
            plan(&map, &w96(), &PlanRequest::default()).verdict,
            Verdict::Resident
        );
        assert_eq!(
            plan(&map, &w32(), &PlanRequest::default()).verdict,
            Verdict::DoesNotFit
        );
    }

    #[test]
    fn a_model_larger_than_ram_and_vram_streams_from_nvme() {
        let mut map = reference_moe_100b_q4();
        map.routed_expert_bytes = (400.0 * GB) as u64;
        let plan = plan(&map, &w32(), &PlanRequest::default());
        assert_eq!(plan.verdict, Verdict::Streaming);
        assert!(plan.nvme_expert_bytes > 0);
        assert!(
            plan.notes
                .iter()
                .any(|n| n.contains("honest, not optimistic"))
        );
    }

    #[test]
    fn quantized_kv_leaves_more_room_for_experts() {
        let map = reference_moe_100b_q4();
        let q8 = plan(
            &map,
            &w32(),
            &PlanRequest {
                kv_quantized: true,
                ..Default::default()
            },
        );
        let fp16 = plan(
            &map,
            &w32(),
            &PlanRequest {
                kv_quantized: false,
                ..Default::default()
            },
        );
        assert!(q8.kv_cache_bytes < fp16.kv_cache_bytes);
        assert!(q8.hot_expert_cache_bytes > fp16.hot_expert_cache_bytes);
    }

    #[test]
    fn a_bigger_hot_cache_means_more_tokens_per_second() {
        let map = reference_moe_100b_q4();
        let small_gpu = Machine {
            gpus: vec![24],
            ..w32()
        };
        let big_gpu = Machine {
            gpus: vec![48],
            ..w32()
        };
        let a = plan(&map, &small_gpu, &PlanRequest::default());
        let b = plan(&map, &big_gpu, &PlanRequest::default());
        assert!(b.hot_expert_fraction > a.hot_expert_fraction);
        assert!(b.estimated_tok_s > a.estimated_tok_s);
    }

    #[test]
    fn the_planner_reports_why_a_model_does_not_fit() {
        let map = reference_dense_70b_fp8();
        let p = plan(&map, &w32(), &PlanRequest::default());
        assert_eq!(p.verdict, Verdict::DoesNotFit);
        assert!(
            p.notes.iter().any(|n| n.contains("before any experts")),
            "{:?}",
            p.notes
        );
    }
}
