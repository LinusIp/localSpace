//! Opt-in startup and frame timing, printed to stderr when `LOCALSPACE_PERF` is
//! set. This is how "it feels slow" becomes a number that names its cause.
//!
//! Every line carries milliseconds since `mark_start` — the top of `main` — so
//! the startup story reads in order: Core ready, surface compiled, first frame.
//! After that, one line per second: how many host frames ran, what they cost on
//! the CPU, and how often the guest surface actually ran and for how long.

use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

/// Call once, as early in `main` as possible.
pub fn mark_start() {
    let _ = START.set(Instant::now());
}

pub fn since_start_ms() -> f32 {
    START
        .get()
        .map(|s| s.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or(0.0)
}

pub fn enabled() -> bool {
    std::env::var_os("LOCALSPACE_PERF").is_some()
}

/// `LOCALSPACE_PERF_SPIN`: ask for a repaint every frame, so the meter reports
/// the frame rate the render loop can actually reach with nobody touching the
/// mouse. If that number is low, the cost is in presenting frames, not in
/// producing them.
pub fn spin() -> bool {
    std::env::var_os("LOCALSPACE_PERF_SPIN").is_some()
}

pub fn log(line: impl AsRef<str>) {
    if enabled() {
        eprintln!("[perf {:8.0} ms] {}", since_start_ms(), line.as_ref());
    }
}

/// Per-second aggregation of host and guest frame costs.
pub struct Meter {
    window: Instant,
    frames: u32,
    host_ms_sum: f32,
    host_ms_max: f32,
    guest_runs: u32,
    guest_ms_sum: f32,
    guest_ms_max: f32,
    seen_guest_frames: u64,
    first_reported: bool,
    pub gpu_reported: bool,
    /// Frames since start, for the on-screen counter in spin mode.
    pub total_frames: u64,
    /// Wall-clock gaps between consecutive frames this second: what the user
    /// feels, as opposed to what a frame costs. A cheap frame every 150 ms is
    /// a throttle somewhere between the app and the screen.
    last_frame: Option<Instant>,
    gap_ms_sum: f32,
    gap_ms_max: f32,
    gap_ms_min: f32,
    /// Who asked for each repaint this second, by call site. This is what turns
    /// "it repaints when idle" into a file and line.
    causes: std::collections::BTreeMap<String, u32>,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new()
    }
}

impl Meter {
    pub fn new() -> Meter {
        Meter {
            window: Instant::now(),
            frames: 0,
            host_ms_sum: 0.0,
            host_ms_max: 0.0,
            guest_runs: 0,
            guest_ms_sum: 0.0,
            guest_ms_max: 0.0,
            seen_guest_frames: 0,
            first_reported: false,
            gpu_reported: false,
            total_frames: 0,
            last_frame: None,
            gap_ms_sum: 0.0,
            gap_ms_max: 0.0,
            gap_ms_min: f32::MAX,
            causes: std::collections::BTreeMap::new(),
        }
    }

    /// Record who asked for the repaints that produced this frame.
    pub fn note_causes<I: IntoIterator<Item = String>>(&mut self, causes: I) {
        if !enabled() {
            return;
        }
        for c in causes {
            *self.causes.entry(c).or_insert(0) += 1;
        }
    }

    /// Once per host frame. `host_cpu_s` is what the previous frame cost on the
    /// CPU (eframe reports it a frame late); `guest_frames_total` and
    /// `guest_last_ms` come from the surface runner.
    pub fn frame(&mut self, host_cpu_s: Option<f32>, guest_frames_total: u64, guest_last_ms: f32) {
        if !enabled() {
            return;
        }
        if !self.first_reported {
            self.first_reported = true;
            log("first frame drawn");
        }
        self.frames += 1;
        self.total_frames += 1;
        let now = Instant::now();
        if let Some(last) = self.last_frame {
            let gap = (now - last).as_secs_f32() * 1000.0;
            self.gap_ms_sum += gap;
            self.gap_ms_max = self.gap_ms_max.max(gap);
            self.gap_ms_min = self.gap_ms_min.min(gap);
        }
        self.last_frame = Some(now);
        if let Some(s) = host_cpu_s {
            let ms = s * 1000.0;
            self.host_ms_sum += ms;
            self.host_ms_max = self.host_ms_max.max(ms);
        }
        if guest_frames_total > self.seen_guest_frames {
            let ran = (guest_frames_total - self.seen_guest_frames) as u32;
            self.guest_runs += ran;
            self.guest_ms_sum += guest_last_ms * ran as f32;
            self.guest_ms_max = self.guest_ms_max.max(guest_last_ms);
            self.seen_guest_frames = guest_frames_total;
        }

        let elapsed = self.window.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            let host_avg = if self.frames > 0 {
                self.host_ms_sum / self.frames as f32
            } else {
                0.0
            };
            let guest_avg = if self.guest_runs > 0 {
                self.guest_ms_sum / self.guest_runs as f32
            } else {
                0.0
            };
            let gaps = if self.frames > 1 {
                format!(
                    " · frame gap min {:.0} avg {:.0} max {:.0} ms",
                    self.gap_ms_min,
                    self.gap_ms_sum / (self.frames - 1) as f32,
                    self.gap_ms_max
                )
            } else {
                String::new()
            };
            log(format!(
                "{} host frames/s · host cpu avg {:.2} max {:.2} ms · guest ran {} (avg {:.2} max {:.2} ms){gaps}",
                self.frames, host_avg, self.host_ms_max, self.guest_runs, guest_avg, self.guest_ms_max
            ));
            self.gap_ms_sum = 0.0;
            self.gap_ms_max = 0.0;
            self.gap_ms_min = f32::MAX;
            if !self.causes.is_empty() {
                let list: Vec<String> = self
                    .causes
                    .iter()
                    .map(|(c, n)| format!("{c} ×{n}"))
                    .collect();
                log(format!("  repaints asked for by: {}", list.join(" · ")));
                self.causes.clear();
            }
            self.window = Instant::now();
            self.frames = 0;
            self.host_ms_sum = 0.0;
            self.host_ms_max = 0.0;
            self.guest_runs = 0;
            self.guest_ms_sum = 0.0;
            self.guest_ms_max = 0.0;
        }
    }
}
