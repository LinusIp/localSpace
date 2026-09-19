//! What this computer is, for a person and for the planner.
//!
//! The graphics cards are the ones the inference engine itself lists
//! (`llama-server --list-devices`): one question that covers NVIDIA, AMD and
//! Intel through Vulkan, answers with the memory that is *free right now*, and
//! cannot disagree with what the engine will then use. The rest (system
//! memory, the processor, the disk) comes from the operating system, and the
//! memory's speed is measured, because nothing reports it and the estimate
//! for a machine without a usable GPU divides by it.
//!
//! Detection never fails: what cannot be seen is absent or zero, a card that
//! is not in the table is "capability unknown" and promised nothing it
//! cannot be shown to do, and a machine with no GPU runs on its processor.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};

/// The table of graphics cards' memory bandwidth, compiled in with the model
/// list until both become a fetched index (docs/DECISIONS.md, 2026-09-18).
const GPU_TABLE: &str = include_str!("../../../models/gpus.json");

/// How long the engine gets to list its devices. The first Vulkan start on
/// a machine compiles nothing, but a cold driver can take seconds.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

/// Which of the engine's backends a device belongs to: the prefix of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Vulkan,
    Cuda,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gpu {
    /// What the engine's `--device` takes, such as `Vulkan0`.
    pub device: String,
    pub backend: Backend,
    pub name: String,
    pub vendor: Vendor,
    pub total_mib: u64,
    /// What the engine may use of it: a budget the driver gives each
    /// process, which does not shrink when another program fills the card.
    pub free_mib: u64,
    /// What other programs hold of the card right now, where the operating
    /// system says (Windows' performance counters); `None` where it does not.
    pub used_by_others_mib: Option<u64>,
    /// Shares the system's memory instead of having its own.
    pub integrated: bool,
    /// Memory bandwidth in GB/s when the card is one the table knows.
    pub bandwidth_gbps: Option<f32>,
}

impl Gpu {
    /// A card whose capability is known: its own memory and a known speed.
    pub fn known(&self) -> bool {
        self.integrated || self.bandwidth_gbps.is_some()
    }
}

/// Why there is no list of graphics cards, when there is none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuListing {
    /// The engine answered; the list may still be empty.
    Listed,
    /// No engine to ask: nothing is known about graphics cards.
    NoEngine,
    /// The engine could not be asked, and why.
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hardware {
    /// In the engine's order; the first is the one localSpace uses.
    pub gpus: Vec<Gpu>,
    pub gpu_listing: GpuListing,
    pub ram_total_mib: u64,
    pub ram_free_mib: u64,
    /// Measured by copying memory on several threads for a moment, GB/s.
    pub ram_bandwidth_gbps: f32,
    /// Free where the models are kept; `None` when it could not be asked.
    pub disk_free_mib: Option<u64>,
    pub cores: u32,
    pub cpu: Option<String>,
    pub cpu_features: Vec<String>,
}

impl Hardware {
    /// The card localSpace uses, pinned by name when the engine starts: the
    /// first the engine lists, which prefers a discrete card over the
    /// processor's own graphics.
    pub fn gpu(&self) -> Option<&Gpu> {
        self.gpus.first()
    }

    /// One sentence a person recognises their computer in: "NVIDIA GeForce
    /// RTX 4060 Laptop GPU, 8 GB of graphics memory, 32 GB of system memory".
    pub fn sentence(&self) -> String {
        let memory = format!("{} GB of system memory", installed_gb(self.ram_total_mib));
        match self.gpu() {
            Some(gpu) if gpu.integrated => format!(
                "{}, which shares the system memory, {memory}",
                display_name(&gpu.name)
            ),
            Some(gpu) => format!(
                "{}, {} GB of graphics memory, {memory}",
                display_name(&gpu.name),
                nearest_gb(gpu.total_mib)
            ),
            None if self.gpu_listing == GpuListing::Listed => {
                format!("No graphics card localSpace can use, {memory}")
            }
            None => format!("Graphics card not checked, {memory}"),
        }
    }

    /// What follows from the sentence, in plain words; empty when there is
    /// nothing to add.
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        match (&self.gpu_listing, self.gpu()) {
            (GpuListing::Listed, Some(gpu)) if gpu.integrated => notes.push(
                "This computer's graphics share the system memory, so localSpace expects the \
                 speed of the processor and starts carefully."
                    .into(),
            ),
            (GpuListing::Listed, Some(gpu)) if !gpu.known() => {
                notes.push("GPU detected, capability unknown \u{2014} starting carefully.".into())
            }
            (GpuListing::Listed, Some(_)) => {}
            (GpuListing::Listed, None) => notes.push(
                "localSpace will run the model on the processor. If this computer has a \
                 graphics card, its driver may be too old for localSpace to use it: updating \
                 the graphics driver is the fix."
                    .into(),
            ),
            (GpuListing::NoEngine, _) => notes.push(
                "The graphics card could not be checked, because the part of localSpace that \
                 runs the model is not installed."
                    .into(),
            ),
            (GpuListing::Failed(_), _) => notes.push(
                "The graphics card could not be checked, so localSpace starts as if there \
                 were none."
                    .into(),
            ),
        }
        notes
    }
}

/// A card's memory as its box counts it: 3962 MiB is "4 GB", 11264 is "11 GB".
fn nearest_gb(mib: u64) -> u64 {
    (mib as f64 / 1024.0).round().max(1.0) as u64
}

/// System memory as it was bought. What the operating system reports is the
/// installed amount less what the hardware keeps (15.4 of 16, 31.2 of 32),
/// and installed amounts are even, so the next even size above it is the one.
fn installed_gb(mib: u64) -> u64 {
    let gb = mib as f64 / 1024.0;
    if gb <= 4.0 {
        gb.ceil().max(1.0) as u64
    } else {
        ((gb / 2.0).ceil() * 2.0) as u64
    }
}

/// The name as the maker writes it, without the marks a driver adds.
fn display_name(name: &str) -> String {
    name.replace("(R)", "")
        .replace("(TM)", "")
        .replace("(r)", "")
        .replace("(tm)", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// The engine's list of devices
// ---------------------------------------------------------------------------

/// What `llama-server --list-devices` printed, as graphics cards. The format
/// is the engine's own, one line a device:
/// `  Vulkan0: NVIDIA GeForce RTX 3050 Ti Laptop GPU (3962 MiB, 3367 MiB free)`.
/// A line that does not read that way is still a device, with nothing known
/// about its memory.
pub fn parse_devices(text: &str) -> Vec<Gpu> {
    let table = GpuTable::built_in();
    let mut gpus = Vec::new();
    let mut listing = false;
    for line in text.lines() {
        if line.trim_start().starts_with("Available devices") {
            listing = true;
            continue;
        }
        if !listing || !line.starts_with(' ') {
            continue;
        }
        let Some((device, rest)) = line.trim().split_once(':') else {
            continue;
        };
        if device.is_empty() || device.contains(' ') {
            continue;
        }
        let rest = rest.trim();
        let (name, total_mib, free_mib) =
            match rest.rfind('(').filter(|at| rest[*at..].contains("MiB")) {
                Some(at) => {
                    let (total, free) = memory(&rest[at..]);
                    (rest[..at].trim(), total, free)
                }
                None => (rest, 0, 0),
            };
        let vendor = vendor_of(name);
        let integrated = is_integrated(name, vendor);
        gpus.push(Gpu {
            device: device.to_string(),
            backend: backend_of(device),
            name: name.to_string(),
            vendor,
            total_mib,
            free_mib,
            used_by_others_mib: None,
            integrated,
            bandwidth_gbps: if integrated {
                None
            } else {
                table.bandwidth(name)
            },
        });
    }
    gpus
}

/// `(3962 MiB, 3367 MiB free)` as two numbers; zero for what is not there.
fn memory(parenthesis: &str) -> (u64, u64) {
    let mut numbers = parenthesis
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u64>().ok());
    let total = numbers.next().unwrap_or(0);
    let free = numbers.next().unwrap_or(0);
    (total, free.min(total))
}

fn backend_of(device: &str) -> Backend {
    let id = device.to_ascii_lowercase();
    if id.starts_with("vulkan") {
        Backend::Vulkan
    } else if id.starts_with("cuda") {
        Backend::Cuda
    } else {
        Backend::Other
    }
}

fn words(name: &str) -> Vec<String> {
    display_name(name)
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn vendor_of(name: &str) -> Vendor {
    let words = words(name);
    let has = |w: &str| words.iter().any(|x| x == w);
    if has("nvidia") || has("geforce") || has("rtx") || has("gtx") || has("quadro") {
        Vendor::Nvidia
    } else if has("amd") || has("radeon") {
        Vendor::Amd
    } else if has("intel") || has("arc") || has("iris") {
        Vendor::Intel
    } else {
        Vendor::Other
    }
}

/// The processor's own graphics, by how their makers name them: AMD's are
/// "... Graphics" or a three-digit "780M" where its cards are "RX 7600" and
/// never "Graphics"; Intel's carry no Arc model number ("UHD Graphics",
/// "Iris Xe Graphics", "Arc Graphics") where its cards are "Arc A770".
fn is_integrated(name: &str, vendor: Vendor) -> bool {
    let words = words(name);
    let model = |w: &String, letters: &[char]| {
        let mut chars = w.chars();
        chars.next().is_some_and(|c| letters.contains(&c))
            && w.len() >= 4
            && w[1..4].chars().all(|c| c.is_ascii_digit())
    };
    let mobile_apu =
        |w: &String| w.len() == 4 && w.ends_with('m') && w[..3].chars().all(|c| c.is_ascii_digit());
    match vendor {
        Vendor::Nvidia | Vendor::Other => false,
        Vendor::Amd => words.iter().any(|w| w == "graphics") || words.iter().any(mobile_apu),
        Vendor::Intel => !words.iter().any(|w| model(w, &['a', 'b'])),
    }
}

#[derive(Deserialize)]
struct GpuTable {
    gpus: Vec<GpuEntry>,
}

#[derive(Deserialize)]
struct GpuEntry {
    #[serde(rename = "match")]
    name: String,
    gbps: f32,
}

impl GpuTable {
    fn built_in() -> GpuTable {
        serde_json::from_str(GPU_TABLE).unwrap_or(GpuTable { gpus: Vec::new() })
    }

    /// The longest entry whose words stand together in the name. A laptop
    /// part is a different part from the desktop one of the same number, so
    /// it matches laptop entries only.
    fn bandwidth(&self, name: &str) -> Option<f32> {
        let name = words(name);
        let laptop = name.iter().any(|w| w == "laptop");
        self.gpus
            .iter()
            .filter_map(|entry| {
                let entry_words = words(&entry.name);
                let is_laptop_entry = entry_words.iter().any(|w| w == "laptop");
                let stands = name
                    .windows(entry_words.len().max(1))
                    .any(|window| window == entry_words.as_slice());
                (is_laptop_entry == laptop && stands).then_some((entry_words.len(), entry.gbps))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, gbps)| gbps)
    }
}

/// Ask the engine for its devices. It is given a time limit and no window.
pub fn list_devices(engine: &Path) -> Result<Vec<Gpu>, String> {
    let mut child = crate::child::command(engine)
        .arg("--list-devices")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{} could not be started: {e}", engine.display()))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > LIST_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} did not list its devices within {} seconds",
                    engine.display(),
                    LIST_TIMEOUT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("waiting for {}: {e}", engine.display())),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("reading from {}: {e}", engine.display()))?;
    Ok(parse_devices(&String::from_utf8_lossy(&output.stdout)))
}

// ---------------------------------------------------------------------------
// The rest of the machine
// ---------------------------------------------------------------------------

/// Everything, best effort. `engine` is `llama-server` when there is one;
/// `storage` is where the models are kept, for the free space.
pub fn detect(engine: Option<&Path>, storage: Option<&Path>) -> Hardware {
    let (gpus, gpu_listing) = match engine {
        None => (Vec::new(), GpuListing::NoEngine),
        Some(engine) => match list_devices(engine) {
            Ok(gpus) => (gpus, GpuListing::Listed),
            Err(why) => {
                tracing::warn!("hardware: {why}");
                (Vec::new(), GpuListing::Failed(why))
            }
        },
    };
    let system = system_facts(storage);
    let mut gpus = gpus;
    for gpu in &mut gpus {
        gpu.used_by_others_mib = system.held_of(&gpu.name);
    }
    Hardware {
        gpus,
        gpu_listing,
        ram_total_mib: system.ram_total_mib,
        ram_free_mib: system.ram_free_mib,
        ram_bandwidth_gbps: measure_ram_bandwidth(),
        disk_free_mib: system.disk_free_mib,
        cores: std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1),
        cpu: system.cpu,
        cpu_features: cpu_features(),
    }
}

/// The drive a path is on, on Windows: the letter of `C:\...` and of the
/// verbatim `\\?\C:\...` alike.
#[cfg(target_os = "windows")]
fn drive_letter(path: &Path) -> Option<char> {
    let full = std::fs::canonicalize(path)
        .ok()
        .or_else(|| std::path::absolute(path).ok())?;
    full.components().find_map(|c| match c {
        std::path::Component::Prefix(prefix) => match prefix.kind() {
            std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => {
                Some(char::from(letter).to_ascii_uppercase())
            }
            _ => None,
        },
        _ => None,
    })
}

/// Where `path` is, as a person names it: "drive C:" on Windows, and the
/// folder itself where drives have no letters.
pub fn place_in_words(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    if let Some(letter) = drive_letter(path) {
        return format!("drive {letter}:");
    }
    format!("the disk that holds {}", path.display())
}

/// Free space where `path` is (or will be), in MiB, asked now. For the
/// moment before a download: the figure of the first run is minutes old.
pub fn disk_free_mib(path: &Path) -> Option<u64> {
    system_facts_for_disk(path)
}

#[cfg(target_os = "windows")]
fn system_facts_for_disk(path: &Path) -> Option<u64> {
    let letter = drive_letter(path)?;
    let script = format!("([System.IO.DriveInfo]::new('{letter}:\\')).AvailableFreeSpace");
    let out = crate::child::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(bytes / 1024 / 1024)
}

#[cfg(target_os = "linux")]
fn system_facts_for_disk(path: &Path) -> Option<u64> {
    // `df -Pk`: one header line, then the file system's line, the fourth
    // field the available kilobytes.
    let existing = path.ancestors().find(|p| p.exists())?;
    let out = crate::child::command("df")
        .arg("-Pk")
        .arg(existing)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let available: u64 = text
        .lines()
        .nth(1)?
        .split_whitespace()
        .nth(3)?
        .parse()
        .ok()?;
    Some(available / 1024)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn system_facts_for_disk(_path: &Path) -> Option<u64> {
    None
}

#[derive(Default)]
struct SystemFacts {
    ram_total_mib: u64,
    ram_free_mib: u64,
    disk_free_mib: Option<u64>,
    cpu: Option<String>,
    /// Each graphics adapter by the name its driver gives it, with the MiB
    /// of its own memory in use by every program together. Windows only.
    adapters: Vec<(String, u64)>,
}

impl SystemFacts {
    /// What is held of the card the engine calls `name`. Asked before
    /// anything of localSpace's is on the card, so all of it is other
    /// programs'. `None` where the operating system does not say, or when
    /// two cards share the name and the figure could be the other one's.
    fn held_of(&self, name: &str) -> Option<u64> {
        let mut matching = self
            .adapters
            .iter()
            .filter(|(adapter, _)| adapter.trim().eq_ignore_ascii_case(name.trim()));
        let first = matching.next()?;
        matching.next().is_none().then_some(first.1)
    }
}

/// What the running engine holds of the graphics memory, in MiB: its own on
/// the card, and "shared", which is system memory standing in for the card.
/// A large shared figure is a model that loaded and will crawl. Windows
/// only (elsewhere a card that is asked for too much refuses, and the load
/// fails instead); read once after a load, never while a person waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineGraphicsMemory {
    pub dedicated_mib: u64,
    pub shared_mib: u64,
}

#[cfg(target_os = "windows")]
pub fn engine_graphics_memory(pid: u32) -> Option<EngineGraphicsMemory> {
    let script = format!(
        "$rows = @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory \
         -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -like 'pid_{pid}_*' }}); \
         if ($rows.Count -gt 0) {{ @{{ \
         dedicated = ($rows | Measure-Object -Property DedicatedUsage -Sum).Sum; \
         shared = ($rows | Measure-Object -Property SharedUsage -Sum).Sum }} | ConvertTo-Json -Compress }}"
    );
    let out = crate::child::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    let json = serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()?;
    Some(EngineGraphicsMemory {
        dedicated_mib: json["dedicated"].as_f64()? as u64 / 1024 / 1024,
        shared_mib: json["shared"].as_f64()? as u64 / 1024 / 1024,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn engine_graphics_memory(_pid: u32) -> Option<EngineGraphicsMemory> {
    None
}

#[cfg(target_os = "windows")]
fn system_facts(storage: Option<&Path>) -> SystemFacts {
    // One question to the operating system, answered as JSON: the memory in
    // KB, the processor's name, and the free bytes of the models' drive.
    let drive = storage.and_then(drive_letter).unwrap_or('C');
    // The adapters: Windows counts each one's memory in use under its LUID
    // (classes whose names are the same in every language, unlike the
    // counters'), and the registry says which name a LUID carries.
    let script = format!(
        "$os = Get-CimInstance Win32_OperatingSystem; \
         $cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name; \
         $free = ([System.IO.DriveInfo]::new('{drive}:\\')).AvailableFreeSpace; \
         $used = @{{}}; \
         Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory \
         -ErrorAction SilentlyContinue | ForEach-Object {{ $used[$_.Name.ToLower()] = $_.DedicatedUsage }}; \
         $adapters = @(); \
         Get-ChildItem 'HKLM:\\SOFTWARE\\Microsoft\\DirectX' -ErrorAction SilentlyContinue | ForEach-Object {{ \
         $p = Get-ItemProperty $_.PSPath; \
         if ($p.Description -and $p.AdapterLuid) {{ \
         $key = ('luid_0x{{0:x8}}_0x{{1:x8}}_phys_0' -f (($p.AdapterLuid -shr 32) -band 0xffffffff), ($p.AdapterLuid -band 0xffffffff)); \
         if ($used.ContainsKey($key)) {{ $adapters += @{{ name = $p.Description; used = $used[$key] }} }} }} }}; \
         @{{ total_kb = $os.TotalVisibleMemorySize; free_kb = $os.FreePhysicalMemory; \
         cpu = $cpu; disk_free = $free; adapters = @($adapters) }} | ConvertTo-Json -Compress -Depth 4"
    );
    let Ok(out) = crate::child::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    else {
        return SystemFacts::default();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return SystemFacts::default();
    };
    SystemFacts {
        ram_total_mib: json["total_kb"].as_u64().unwrap_or(0) / 1024,
        ram_free_mib: json["free_kb"].as_u64().unwrap_or(0) / 1024,
        disk_free_mib: json["disk_free"].as_u64().map(|b| b / 1024 / 1024),
        cpu: json["cpu"].as_str().map(|s| s.trim().to_string()),
        adapters: json["adapters"]
            .as_array()
            .map(|adapters| {
                adapters
                    .iter()
                    .filter_map(|a| {
                        let used = a["used"].as_f64()? as u64 / 1024 / 1024;
                        Some((a["name"].as_str()?.to_string(), used))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[cfg(target_os = "linux")]
fn system_facts(storage: Option<&Path>) -> SystemFacts {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let kb = |key: &str| {
        meminfo
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("model name"))
                .and_then(|rest| rest.split_once(':'))
                .map(|(_, name)| name.trim().to_string())
        });
    let disk_free_mib = storage.and_then(system_facts_for_disk);
    SystemFacts {
        ram_total_mib: kb("MemTotal:") / 1024,
        ram_free_mib: kb("MemAvailable:") / 1024,
        disk_free_mib,
        cpu,
        adapters: Vec::new(),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn system_facts(_storage: Option<&Path>) -> SystemFacts {
    SystemFacts::default()
}

fn cpu_features() -> Vec<String> {
    #[allow(unused_mut)]
    let mut features: Vec<String> = Vec::new();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        for (name, present) in [
            ("avx", std::arch::is_x86_feature_detected!("avx")),
            ("avx2", std::arch::is_x86_feature_detected!("avx2")),
            ("fma", std::arch::is_x86_feature_detected!("fma")),
            ("f16c", std::arch::is_x86_feature_detected!("f16c")),
            ("avx512f", std::arch::is_x86_feature_detected!("avx512f")),
        ] {
            if present {
                features.push(name.to_string());
            }
        }
    }
    features
}

/// How fast this machine's memory moves, GB/s: several threads each copying
/// a buffer larger than any cache for a tenth of a second. Generating a
/// token on the processor reads the whole model once, so its speed is this
/// figure divided by the model's size, times a measured efficiency.
pub fn measure_ram_bandwidth() -> f32 {
    const BUFFER: usize = 32 * 1024 * 1024;
    const FOR: Duration = Duration::from_millis(100);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 8);
    let rate: f64 = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let source = vec![1u8; BUFFER];
                    let mut target = vec![0u8; BUFFER];
                    let begun = Instant::now();
                    let mut bytes = 0u64;
                    while begun.elapsed() < FOR {
                        target.copy_from_slice(&source);
                        // Looked at, so the copy cannot be optimised away.
                        std::hint::black_box(target[BUFFER / 2]);
                        bytes += BUFFER as u64;
                    }
                    bytes as f64 / begun.elapsed().as_secs_f64().max(1e-9)
                })
            })
            .collect();
        workers.into_iter().filter_map(|w| w.join().ok()).sum()
    });
    (rate / 1e9) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorded on the development laptop on 2026-09-18 (release b10869): a
    /// Ryzen with Radeon graphics and an RTX 3050 Ti. The engine lists the
    /// discrete card only.
    const RECORDED_HYBRID_LAPTOP: &str = "Available devices:\n  Vulkan0: NVIDIA GeForce RTX 3050 Ti Laptop GPU (3962 MiB, 3367 MiB free)\n";
    /// Recorded the same day with every device hidden: a machine without one.
    const RECORDED_NONE: &str = "ggml_vulkan: Invalid device index 99 in GGML_VK_VISIBLE_DEVICES.\nAvailable devices:\n  (none)\n";

    /// Written, not recorded: the engine's format with names as AMD's and
    /// Intel's drivers report them. The format is the engine's one line for
    /// every vendor; what these check is how the names are read.
    fn written(name: &str, total: u64, free: u64) -> String {
        format!("Available devices:\n  Vulkan0: {name} ({total} MiB, {free} MiB free)\n")
    }

    #[test]
    fn the_recorded_laptop_is_one_known_discrete_card_with_its_free_memory() {
        let gpus = parse_devices(RECORDED_HYBRID_LAPTOP);
        assert_eq!(gpus.len(), 1);
        let gpu = &gpus[0];
        assert_eq!(gpu.device, "Vulkan0");
        assert_eq!(gpu.backend, Backend::Vulkan);
        assert_eq!(gpu.vendor, Vendor::Nvidia);
        assert_eq!((gpu.total_mib, gpu.free_mib), (3962, 3367));
        assert!(!gpu.integrated);
        assert_eq!(
            gpu.bandwidth_gbps,
            Some(192.0),
            "a laptop part, not the desktop 3050"
        );
        assert!(gpu.known());
    }

    #[test]
    fn no_device_is_an_empty_list_not_an_error() {
        assert!(parse_devices(RECORDED_NONE).is_empty());
        assert!(parse_devices("").is_empty());
        assert!(parse_devices("something else entirely\n").is_empty());
    }

    #[test]
    fn a_laptop_part_never_takes_the_desktop_figure_and_the_longest_name_wins() {
        let table = GpuTable::built_in();
        assert_eq!(
            table.bandwidth("NVIDIA GeForce RTX 4060 Laptop GPU"),
            Some(256.0)
        );
        assert_eq!(table.bandwidth("NVIDIA GeForce RTX 4060"), Some(272.0));
        assert_eq!(table.bandwidth("NVIDIA GeForce RTX 4060 Ti"), Some(288.0));
        assert_eq!(
            table.bandwidth("NVIDIA GeForce RTX 4070 Ti SUPER"),
            Some(672.0)
        );
        // A laptop part the table does not have is unknown, never the desktop one.
        assert_eq!(table.bandwidth("NVIDIA GeForce RTX 5070 Laptop GPU"), None);
        // "7600S" is not a "7600".
        assert_eq!(table.bandwidth("AMD Radeon RX 7600S"), None);
        assert_eq!(table.bandwidth("AMD Radeon RX 7600"), Some(288.0));
        assert_eq!(
            table.bandwidth("Intel(R) Arc(TM) A770 Graphics"),
            Some(512.0)
        );
    }

    #[test]
    fn a_processors_own_graphics_are_told_from_a_card_by_name() {
        for name in [
            "AMD Radeon(TM) Graphics",
            "AMD Radeon(TM) 780M",
            "Intel(R) UHD Graphics",
            "Intel(R) Iris(R) Xe Graphics",
            "Intel(R) Arc(TM) Graphics",
        ] {
            let gpu = &parse_devices(&written(name, 8192, 7000))[0];
            assert!(gpu.integrated, "{name}");
            assert!(
                gpu.known(),
                "{name}: planned as the processor, so nothing is unknown"
            );
        }
        for name in [
            "AMD Radeon RX 7600",
            "AMD Radeon RX 6600M",
            "Intel(R) Arc(TM) A770 Graphics",
            "Intel(R) Arc(TM) A370M Graphics",
            "NVIDIA GeForce GTX 1650",
        ] {
            assert!(
                !parse_devices(&written(name, 8192, 7000))[0].integrated,
                "{name}"
            );
        }
    }

    #[test]
    fn an_unknown_card_is_kept_and_said_to_be_unknown() {
        let gpus = parse_devices(&written("Moore Threads MTT S80", 16384, 16000));
        assert_eq!(gpus[0].vendor, Vendor::Other);
        assert!(!gpus[0].known());
        let hardware = Hardware {
            gpus,
            gpu_listing: GpuListing::Listed,
            ram_total_mib: 32_000,
            ram_free_mib: 20_000,
            ram_bandwidth_gbps: 40.0,
            disk_free_mib: Some(100_000),
            cores: 8,
            cpu: None,
            cpu_features: Vec::new(),
        };
        assert_eq!(
            hardware.notes(),
            ["GPU detected, capability unknown \u{2014} starting carefully."]
        );
    }

    #[test]
    fn a_line_without_its_memory_is_still_a_device() {
        let gpus = parse_devices("Available devices:\n  Vulkan0: Some Card\n");
        assert_eq!(gpus.len(), 1);
        assert_eq!((gpus[0].total_mib, gpus[0].free_mib), (0, 0));
    }

    fn with(gpus: Vec<Gpu>, listing: GpuListing, ram_total_mib: u64) -> Hardware {
        Hardware {
            gpus,
            gpu_listing: listing,
            ram_total_mib,
            ram_free_mib: ram_total_mib / 2,
            ram_bandwidth_gbps: 40.0,
            disk_free_mib: None,
            cores: 8,
            cpu: None,
            cpu_features: Vec::new(),
        }
    }

    #[test]
    fn the_sentence_is_in_the_words_of_the_box_the_computer_came_in() {
        let laptop = with(
            parse_devices(RECORDED_HYBRID_LAPTOP),
            GpuListing::Listed,
            15_790,
        );
        assert_eq!(
            laptop.sentence(),
            "NVIDIA GeForce RTX 3050 Ti Laptop GPU, 4 GB of graphics memory, 16 GB of system memory"
        );
        assert!(laptop.notes().is_empty());

        let shared = with(
            parse_devices(&written("AMD Radeon(TM) 780M", 8192, 7000)),
            GpuListing::Listed,
            31_900,
        );
        assert_eq!(
            shared.sentence(),
            "AMD Radeon 780M, which shares the system memory, 32 GB of system memory"
        );
        assert_eq!(shared.notes().len(), 1);

        let none = with(Vec::new(), GpuListing::Listed, 7_900);
        assert_eq!(
            none.sentence(),
            "No graphics card localSpace can use, 8 GB of system memory"
        );
        assert!(none.notes()[0].contains("on the processor"));
    }

    #[test]
    fn detection_without_an_engine_still_describes_the_machine() {
        let hardware = detect(None, None);
        // In the test log: what the machine that ran this looks like.
        eprintln!(
            "{} | memory moves at {:.1} GB/s | {:?} | {:?}",
            hardware.sentence(),
            hardware.ram_bandwidth_gbps,
            hardware.cpu,
            hardware.cpu_features
        );
        assert_eq!(hardware.gpu_listing, GpuListing::NoEngine);
        assert!(hardware.sentence().starts_with("Graphics card not checked"));
        assert!(hardware.cores >= 1);
        assert!(hardware.ram_bandwidth_gbps > 0.0);
        assert!(!hardware.sentence().is_empty());
    }

    #[test]
    fn what_is_held_of_a_card_is_found_by_its_name_and_never_guessed() {
        let facts = SystemFacts {
            adapters: vec![
                ("NVIDIA GeForce RTX 3050 Ti Laptop GPU".into(), 50),
                ("AMD Radeon(TM) Graphics".into(), 300),
            ],
            ..SystemFacts::default()
        };
        assert_eq!(
            facts.held_of("NVIDIA GeForce RTX 3050 Ti Laptop GPU"),
            Some(50)
        );
        assert_eq!(
            facts.held_of(" nvidia geforce rtx 3050 ti laptop gpu "),
            Some(50)
        );
        assert_eq!(facts.held_of("NVIDIA GeForce RTX 4060"), None);
        let twins = SystemFacts {
            adapters: vec![
                ("NVIDIA RTX A4000".into(), 10),
                ("NVIDIA RTX A4000".into(), 9000),
            ],
            ..SystemFacts::default()
        };
        assert_eq!(
            twins.held_of("NVIDIA RTX A4000"),
            None,
            "which of the two is not known"
        );
    }

    #[test]
    fn an_engine_that_is_not_there_is_a_reason_not_a_panic() {
        let failed = list_devices(Path::new("no-such-engine-here"));
        assert!(failed.is_err());
    }
}
