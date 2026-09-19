//! How a model sits on this computer, and how fast it will be.
//!
//! One calculation serves three places: the verdict and speed every catalog
//! entry shows *before* a download, the recommendation of the first run, and
//! the number of layers the engine is told to put on the graphics card
//! (never "all of them and hope"). It is made from what was measured on this
//! computer (the card's memory and how much of it is in use, the system's
//! memory and how fast it moves) and from the model's own numbers.
//!
//! The speed is a roofline: generating a token reads every active weight
//! once, so the time is the bytes read divided by the bandwidth of the memory
//! they sit in, per part, plus a fixed cost a token on the card. The
//! efficiencies are measured, per backend (docs/DECISIONS.md, 2026-09-18), and
//! what a person is shown is never this number but a range of words a second,
//! rounded down.

use crate::hardware::{Backend, Hardware};
use serde::{Deserialize, Serialize};

const MIB: u64 = 1024 * 1024;

/// Left free on the card beyond what the plan uses: the desktop's compositor
/// and every other window draw from the same memory, and on Windows a card
/// that is asked for more than it has does not refuse, it spills into system
/// memory and the model crawls. The larger of a floor and a share of the card.
const GPU_MARGIN_FLOOR_MIB: u64 = 384;
const GPU_MARGIN_SHARE: f64 = 0.08;
/// The engine's working buffers on the card, beside weights and the KV cache.
/// 80 MiB was measured for a 3B model at 8192 tokens; larger models need more.
const GPU_COMPUTE_MIB: u64 = 256;
/// System memory that is not the model's: the operating system, the desktop,
/// localSpace itself, a browser. The larger of a floor and a share.
const RAM_RESERVE_FLOOR_MIB: u64 = 4 * 1024;
const RAM_RESERVE_SHARE: f64 = 0.25;

/// Measured on an RTX 3050 Ti Laptop through Vulkan (engine b10869) with two
/// models: the card delivers this share of its published bandwidth, and every
/// token costs this much besides.
const VULKAN_EFFICIENCY: f64 = 0.60;
/// Not measured yet: taken as Vulkan's, so that a CUDA build placed by hand
/// is never promised more than what was measured.
const CUDA_EFFICIENCY: f64 = 0.60;
const GPU_TOKEN_OVERHEAD_S: f64 = 0.0052;
/// Generation on the processor read memory at twice the rate the machine
/// copied it (a copy reads and writes each byte): measured on a Ryzen 7 6800H.
const READ_PER_COPY: f64 = 2.0;

/// The lines between the verdicts, in tokens a second. **Provisional**: the
/// ten laptops of the first test calibrate them, and revising them after it
/// is the expected thing (docs/DECISIONS.md, 2026-09-18, answer 5).
const RUNS_WELL_TOKENS_PER_SECOND: f32 = 15.0;
const WORKS_TOKENS_PER_SECOND: f32 = 5.0;
/// English runs at about three words to four tokens.
const WORDS_PER_TOKEN: f32 = 0.75;
/// The shown range reaches this far below the estimate.
const RANGE_LOW: f32 = 0.75;

/// A model's own numbers, from the catalog or from its file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    /// Every weight.
    pub weight_bytes: u64,
    /// What generating one token reads: all of a dense model, the shared
    /// part and the active experts of a mixture of experts.
    pub active_bytes: u64,
    /// Repeating layers; the engine counts one more, the output layer.
    pub layers: u32,
    pub kv_bytes_per_token_fp16: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    RunsWell,
    Works,
    TooSlow,
    WillNotFit,
}

impl Verdict {
    /// What a person reads. A slow model is shown with its verdict, never hidden.
    pub fn label(self) -> &'static str {
        match self {
            Verdict::RunsWell => "Runs well \u{2014} faster than you read",
            Verdict::Works => "Works, slower than reading pace",
            Verdict::TooSlow => "Too slow for everyday use",
            Verdict::WillNotFit => "Will not fit on this computer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fit {
    /// Layers the engine is told to put on the card, of `layers_total`.
    pub gpu_layers: u32,
    /// The repeating layers and the output layer: what "all" means.
    pub layers_total: u32,
    /// The device the engine is pinned to; `None` runs on the processor.
    pub device: Option<String>,
    /// What the plan takes on the card: weights, KV cache, working buffers.
    pub gpu_mib: u64,
    /// What stays in system memory.
    pub ram_mib: u64,
    pub verdict: Verdict,
    /// The model's estimate. Not for display: see `words_per_second`.
    pub tokens_per_second: f32,
    /// What is displayed: a range of words a second, rounded down.
    pub words_per_second: (u32, u32),
    /// The card is not one the table knows, so the speed is what the
    /// processor alone would do and is shown as "at least": no figure is
    /// given that cannot be supported.
    pub at_least: bool,
    /// One plain sentence on where the model sits.
    pub placement: String,
}

impl Fit {
    /// "about 20 to 30 words a second", or the one number when the range is
    /// a point, or nothing to promise.
    pub fn speed_in_words(&self) -> String {
        match (self.verdict, self.words_per_second) {
            (Verdict::WillNotFit, _) => String::new(),
            (_, (_, 0)) => "less than one word a second".into(),
            (_, (_, high)) if self.at_least => format!("at least {high} words a second"),
            (_, (0, high)) => format!("up to {high} words a second"),
            (_, (low, high)) if low == high => format!("about {high} words a second"),
            (_, (low, high)) => format!("about {low} to {high} words a second"),
        }
    }
}

/// What is asked for: the context the engine will be started with.
#[derive(Debug, Clone, Copy)]
pub struct Ask {
    pub context_len: u32,
    pub kv_quantized: bool,
}

/// The card's memory this plan may use, in MiB: the **smaller** of the
/// budget the driver gives the engine and the card's total less what other
/// programs hold, and the margin off that. Planning against the larger of
/// two numbers is how a model loads and crawls.
fn usable_gpu_mib(gpu: &crate::hardware::Gpu) -> u64 {
    let margin = GPU_MARGIN_FLOOR_MIB.max((gpu.total_mib as f64 * GPU_MARGIN_SHARE) as u64);
    let not_held = gpu
        .total_mib
        .saturating_sub(gpu.used_by_others_mib.unwrap_or(0));
    gpu.free_mib
        .min(gpu.total_mib)
        .min(not_held)
        .saturating_sub(margin)
}

fn usable_ram_mib(hardware: &Hardware) -> u64 {
    let reserve =
        RAM_RESERVE_FLOOR_MIB.max((hardware.ram_total_mib as f64 * RAM_RESERVE_SHARE) as u64);
    hardware.ram_total_mib.saturating_sub(reserve)
}

/// Place `shape` on `hardware`. `gpu_layer_cap` is the most layers the card
/// may be given: `None` at first, fewer after a load that did not hold.
pub fn fit(shape: &Shape, hardware: &Hardware, ask: Ask, gpu_layer_cap: Option<u32>) -> Fit {
    let layers_total = shape.layers.max(1) + 1;
    let per_layer = shape.weight_bytes.div_ceil(layers_total as u64);
    let kv_per_token = if ask.kv_quantized {
        shape.kv_bytes_per_token_fp16 / 2
    } else {
        shape.kv_bytes_per_token_fp16
    };
    let kv_bytes = kv_per_token * ask.context_len as u64;
    // The KV cache of a layer lives where the layer does; the output layer has none.
    let kv_per_layer = kv_bytes.div_ceil(shape.layers.max(1) as u64);

    // A card of its own is worth planning for; graphics that share the
    // system's memory are planned as the processor they sit beside.
    let card = hardware.gpu().filter(|gpu| !gpu.integrated);
    let mut gpu_layers = 0u32;
    if let Some(gpu) = card {
        let usable = usable_gpu_mib(gpu) * MIB;
        let fixed = GPU_COMPUTE_MIB * MIB;
        if usable > fixed {
            let each = per_layer + kv_per_layer;
            gpu_layers = (((usable - fixed) / each.max(1)) as u32).min(layers_total);
        }
        if let Some(cap) = gpu_layer_cap {
            gpu_layers = gpu_layers.min(cap);
        }
    }

    let on_card = gpu_layers.min(layers_total);
    let gpu_weight = per_layer * on_card as u64;
    let gpu_kv = kv_per_layer * on_card.min(shape.layers) as u64;
    let gpu_bytes = if on_card > 0 {
        gpu_weight + gpu_kv + GPU_COMPUTE_MIB * MIB
    } else {
        0
    };
    let ram_bytes = shape.weight_bytes.saturating_sub(gpu_weight) + kv_bytes.saturating_sub(gpu_kv);

    let fits = ram_bytes / MIB <= usable_ram_mib(hardware);

    // The roofline, part by part. A card the table does not know gets no
    // figure of its own: the layers still go to it, and the speed promised
    // is what the processor alone would do.
    let at_least = card.is_some_and(|gpu| on_card > 0 && gpu.bandwidth_gbps.is_none());
    let share_on_card = if at_least {
        0.0
    } else {
        on_card as f64 / layers_total as f64
    };
    let active = shape.active_bytes as f64;
    let mut seconds = 0.0_f64;
    if let (Some(gpu), true) = (card, share_on_card > 0.0) {
        let efficiency = match gpu.backend {
            Backend::Cuda => CUDA_EFFICIENCY,
            Backend::Vulkan | Backend::Other => VULKAN_EFFICIENCY,
        };
        let bandwidth = gpu.bandwidth_gbps.unwrap_or(1.0) as f64;
        seconds += active * share_on_card / (bandwidth * efficiency * 1e9);
        seconds += GPU_TOKEN_OVERHEAD_S * share_on_card;
    }
    let read_gbps = (hardware.ram_bandwidth_gbps as f64 * READ_PER_COPY).max(1.0);
    seconds += active * (1.0 - share_on_card) / (read_gbps * 1e9);
    let tokens_per_second = if fits && seconds > 0.0 {
        (1.0 / seconds) as f32
    } else {
        0.0
    };

    let verdict = if !fits {
        Verdict::WillNotFit
    } else if tokens_per_second >= RUNS_WELL_TOKENS_PER_SECOND {
        Verdict::RunsWell
    } else if tokens_per_second >= WORKS_TOKENS_PER_SECOND {
        Verdict::Works
    } else {
        Verdict::TooSlow
    };
    let words = tokens_per_second * WORDS_PER_TOKEN;
    let placement = if !fits {
        "It needs more memory than this computer has.".to_string()
    } else if on_card == layers_total {
        "All of it fits in the graphics memory.".to_string()
    } else if on_card > 0 {
        // A share in plain words; how many layers that is, is for support
        // (`localspace doctor`, the trace), not for the person choosing.
        let share = on_card as f32 / layers_total as f32;
        let part = if share >= 0.75 {
            "Most of it fits"
        } else if share >= 0.4 {
            "About half of it fits"
        } else {
            "A small part of it fits"
        };
        format!("{part} in the graphics memory; the rest runs from system memory, which is slower.")
    } else if card.is_some() {
        "It does not fit in the graphics memory, so it runs from system memory.".to_string()
    } else {
        "It runs on the processor, from system memory.".to_string()
    };

    Fit {
        gpu_layers: on_card,
        layers_total,
        device: card.filter(|_| on_card > 0).map(|gpu| gpu.device.clone()),
        gpu_mib: gpu_bytes / MIB,
        ram_mib: ram_bytes / MIB,
        verdict,
        tokens_per_second,
        words_per_second: (round_down(words * RANGE_LOW), round_down(words)),
        at_least,
        placement,
    }
}

/// Down to a number that does not pretend: whole words under ten, fives
/// under fifty, tens above.
fn round_down(words: f32) -> u32 {
    let whole = words.max(0.0).floor() as u32;
    match whole {
        0..=9 => whole,
        10..=49 => whole / 5 * 5,
        _ => whole / 10 * 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{Gpu, GpuListing, Vendor};

    const GB: u64 = 1_000_000_000;

    fn laptop_3050ti() -> Hardware {
        // The development laptop as measured on 2026-09-18.
        Hardware {
            gpus: vec![Gpu {
                device: "Vulkan0".into(),
                backend: Backend::Vulkan,
                name: "NVIDIA GeForce RTX 3050 Ti Laptop GPU".into(),
                vendor: Vendor::Nvidia,
                total_mib: 3962,
                free_mib: 3367,
                used_by_others_mib: Some(29),
                integrated: false,
                bandwidth_gbps: Some(192.0),
            }],
            gpu_listing: GpuListing::Listed,
            ram_total_mib: 15_613,
            ram_free_mib: 7_184,
            ram_bandwidth_gbps: 19.3,
            disk_free_mib: Some(140_000),
            cores: 16,
            cpu: None,
            cpu_features: Vec::new(),
        }
    }

    fn without_a_card(mut hardware: Hardware) -> Hardware {
        hardware.gpus.clear();
        hardware
    }

    fn qwen_3b() -> Shape {
        Shape {
            weight_bytes: 2_104_932_768,
            active_bytes: 2_104_932_768,
            layers: 36,
            kv_bytes_per_token_fp16: 36_864,
        }
    }

    fn qwen_half_b() -> Shape {
        Shape {
            weight_bytes: 491_400_032,
            active_bytes: 491_400_032,
            layers: 24,
            kv_bytes_per_token_fp16: 12_288,
        }
    }

    fn qwen_7b() -> Shape {
        Shape {
            weight_bytes: 4_683_073_536,
            active_bytes: 4_683_073_536,
            layers: 28,
            kv_bytes_per_token_fp16: 57_344,
        }
    }

    const ASK: Ask = Ask {
        context_len: 8192,
        kv_quantized: false,
    };

    fn within(estimate: f32, measured: f32, tolerance: f32) -> bool {
        (estimate - measured).abs() / measured <= tolerance
    }

    #[test]
    fn the_estimates_agree_with_what_was_measured_on_the_development_laptop() {
        // llama-bench, engine b10869, 2026-09-18: tokens a second generated.
        let on_the_card = fit(&qwen_3b(), &laptop_3050ti(), ASK, None);
        assert_eq!(on_the_card.gpu_layers, on_the_card.layers_total);
        assert!(
            within(on_the_card.tokens_per_second, 42.5, 0.15),
            "3B on the card: {}",
            on_the_card.tokens_per_second
        );
        let small = fit(&qwen_half_b(), &laptop_3050ti(), ASK, None);
        assert!(
            within(small.tokens_per_second, 105.6, 0.15),
            "0.5B on the card: {}",
            small.tokens_per_second
        );
        let on_the_processor = fit(&qwen_3b(), &without_a_card(laptop_3050ti()), ASK, None);
        assert!(
            within(on_the_processor.tokens_per_second, 18.6, 0.15),
            "3B on the processor: {}",
            on_the_processor.tokens_per_second
        );
        let half = fit(&qwen_3b(), &laptop_3050ti(), ASK, Some(18));
        assert_eq!(half.gpu_layers, 18);
        assert!(
            within(half.tokens_per_second, 25.0, 0.15),
            "3B with half its layers on the card: {}",
            half.tokens_per_second
        );
    }

    #[test]
    fn a_model_larger_than_the_card_still_runs_with_the_layers_that_fit() {
        let plan = fit(&qwen_7b(), &laptop_3050ti(), ASK, None);
        assert!(
            plan.gpu_layers > 0 && plan.gpu_layers < plan.layers_total,
            "{plan:?}"
        );
        assert_ne!(plan.verdict, Verdict::WillNotFit);
        assert_eq!(plan.device.as_deref(), Some("Vulkan0"));
        // What goes on the card stays under what is free there, less the margin.
        assert!(plan.gpu_mib <= 3367 - 384, "{plan:?}");
        assert_eq!(
            plan.placement,
            "About half of it fits in the graphics memory; the rest runs from system memory, which is slower."
        );
        assert!(
            !plan.placement.contains("layer"),
            "no jargon where a person reads"
        );
        // Slower than it would be on a card that held all of it.
        let mut roomy = laptop_3050ti();
        roomy.gpus[0].total_mib = 16_000;
        roomy.gpus[0].free_mib = 15_000;
        assert!(fit(&qwen_7b(), &roomy, ASK, None).tokens_per_second > plan.tokens_per_second);
    }

    #[test]
    fn a_cap_after_a_load_that_did_not_hold_takes_layers_off_the_card() {
        let first = fit(&qwen_7b(), &laptop_3050ti(), ASK, None);
        let second = fit(
            &qwen_7b(),
            &laptop_3050ti(),
            ASK,
            Some(first.gpu_layers - 2),
        );
        assert_eq!(second.gpu_layers, first.gpu_layers - 2);
        assert!(second.tokens_per_second < first.tokens_per_second);
        let none = fit(&qwen_7b(), &laptop_3050ti(), ASK, Some(0));
        assert_eq!(none.gpu_layers, 0);
        assert_eq!(none.device, None);
    }

    #[test]
    fn no_card_is_the_processor_and_never_a_refusal() {
        let plan = fit(&qwen_3b(), &without_a_card(laptop_3050ti()), ASK, None);
        assert_eq!(plan.gpu_layers, 0);
        assert_eq!(plan.verdict, Verdict::RunsWell);
        assert!(plan.placement.contains("on the processor"));
        // The same machine with a 7.6B model: it works, and is said to be slower.
        let larger = fit(&qwen_7b(), &without_a_card(laptop_3050ti()), ASK, None);
        assert_eq!(larger.verdict, Verdict::Works);
    }

    #[test]
    fn graphics_that_share_the_system_memory_are_planned_as_the_processor() {
        let mut shared = laptop_3050ti();
        shared.gpus[0].integrated = true;
        shared.gpus[0].bandwidth_gbps = None;
        let plan = fit(&qwen_3b(), &shared, ASK, None);
        assert_eq!(plan.gpu_layers, 0);
        let bare = fit(&qwen_3b(), &without_a_card(laptop_3050ti()), ASK, None);
        assert_eq!(plan.tokens_per_second, bare.tokens_per_second);
    }

    #[test]
    fn an_unknown_card_is_used_and_promised_only_what_the_processor_would_do() {
        let mut unknown = laptop_3050ti();
        unknown.gpus[0].bandwidth_gbps = None;
        let plan = fit(&qwen_3b(), &unknown, ASK, None);
        let known = fit(&qwen_3b(), &laptop_3050ti(), ASK, None);
        assert_eq!(
            plan.gpu_layers, known.gpu_layers,
            "its layers still go to it"
        );
        let processor = fit(&qwen_3b(), &without_a_card(laptop_3050ti()), ASK, None);
        assert_eq!(plan.tokens_per_second, processor.tokens_per_second);
        assert!(plan.at_least);
        assert_eq!(plan.speed_in_words(), "at least 10 words a second");
        assert!(!known.at_least);
    }

    #[test]
    fn what_other_programs_hold_of_the_card_is_not_planned_with() {
        let idle = fit(&qwen_7b(), &laptop_3050ti(), ASK, None);
        let mut busy = laptop_3050ti();
        // A game holds 2 GB: the driver's budget for the engine does not say so.
        busy.gpus[0].used_by_others_mib = Some(2048);
        let plan = fit(&qwen_7b(), &busy, ASK, None);
        assert!(plan.gpu_layers < idle.gpu_layers, "{plan:?}");
        assert!(plan.gpu_mib <= 3962 - 2048 - 384, "{plan:?}");
    }

    #[test]
    fn what_does_not_fit_in_memory_is_said_not_to_fit() {
        let huge = Shape {
            weight_bytes: 40 * GB,
            active_bytes: 40 * GB,
            layers: 80,
            kv_bytes_per_token_fp16: 327_680,
        };
        let plan = fit(&huge, &laptop_3050ti(), ASK, None);
        assert_eq!(plan.verdict, Verdict::WillNotFit);
        assert_eq!(plan.speed_in_words(), "");
    }

    #[test]
    fn a_slow_model_is_shown_with_its_honest_number() {
        let mut modest = without_a_card(laptop_3050ti());
        modest.ram_bandwidth_gbps = 8.0;
        let plan = fit(&qwen_7b(), &modest, ASK, None);
        assert_eq!(plan.verdict, Verdict::TooSlow, "shown, never hidden");
        assert_eq!(plan.verdict.label(), "Too slow for everyday use");
        assert_eq!(plan.speed_in_words(), "about 1 to 2 words a second");
    }

    #[test]
    fn the_displayed_speed_is_a_range_rounded_down() {
        assert_eq!(round_down(31.9), 30);
        assert_eq!(round_down(23.9), 20);
        assert_eq!(round_down(9.7), 9);
        assert_eq!(round_down(79.0), 70);
        let plan = fit(&qwen_3b(), &laptop_3050ti(), ASK, None);
        assert_eq!(plan.speed_in_words(), "about 20 to 30 words a second");
    }
}
