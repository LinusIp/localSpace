//! What the app itself weighs (spec §1.2).
//!
//! The budget is 50 MB of private resident memory for the Client at idle and
//! 50 MB for Core at idle — the shell, not the workload. Weights, caches,
//! memory-mapped documents and harness processes are shown separately. This
//! module measures the process; `bench` prints it against the budget and the
//! server exports it on `/metrics`, so the number is always in view and never
//! aspirational.

/// The §1.2 budget, in bytes: 50 MB each for the Client and for Core.
pub const APP_BUDGET_BYTES: u64 = 50 * 1024 * 1024;

/// Private (non-shared) resident memory of this process, in bytes.
///
/// Windows: `PrivateUsage` from `GetProcessMemoryInfo`, which is committed
/// private bytes — the same figure Task Manager calls "Commit" for a process.
/// Linux: `RssAnon` from `/proc/self/status`. Elsewhere: `None`, stated as such.
pub fn private_rss_bytes() -> Option<u64> {
    imp::private_rss_bytes()
}

/// Resident set including shared pages, for comparison.
pub fn working_set_bytes() -> Option<u64> {
    imp::working_set_bytes()
}

#[derive(Debug, Clone)]
pub struct Footprint {
    pub private_bytes: Option<u64>,
    pub working_set_bytes: Option<u64>,
    pub budget_bytes: u64,
}

impl Footprint {
    pub fn measure() -> Footprint {
        Footprint {
            private_bytes: private_rss_bytes(),
            working_set_bytes: working_set_bytes(),
            budget_bytes: APP_BUDGET_BYTES,
        }
    }

    pub fn within_budget(&self) -> Option<bool> {
        self.private_bytes.map(|b| b <= self.budget_bytes)
    }

    pub fn describe(&self) -> String {
        match (self.private_bytes, self.working_set_bytes) {
            (Some(p), Some(w)) => format!(
                "{} MB private ({} MB working set) against a {} MB budget — {}",
                p / (1024 * 1024),
                w / (1024 * 1024),
                self.budget_bytes / (1024 * 1024),
                if p <= self.budget_bytes { "within" } else { "over" }
            ),
            (Some(p), None) => format!(
                "{} MB private against a {} MB budget",
                p / (1024 * 1024),
                self.budget_bytes / (1024 * 1024)
            ),
            _ => "not measurable on this platform".into(),
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
        PrivateUsage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCountersEx,
            cb: u32,
        ) -> i32;
    }

    fn counters() -> Option<ProcessMemoryCountersEx> {
        let mut c = ProcessMemoryCountersEx {
            cb: std::mem::size_of::<ProcessMemoryCountersEx>() as u32,
            PageFaultCount: 0,
            PeakWorkingSetSize: 0,
            WorkingSetSize: 0,
            QuotaPeakPagedPoolUsage: 0,
            QuotaPagedPoolUsage: 0,
            QuotaPeakNonPagedPoolUsage: 0,
            QuotaNonPagedPoolUsage: 0,
            PagefileUsage: 0,
            PeakPagefileUsage: 0,
            PrivateUsage: 0,
        };
        // SAFETY: a documented Win32 call with a correctly sized, initialised
        // out-parameter for the current process.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        if ok != 0 {
            Some(c)
        } else {
            None
        }
    }

    pub fn private_rss_bytes() -> Option<u64> {
        counters().map(|c| c.PrivateUsage as u64)
    }

    pub fn working_set_bytes() -> Option<u64> {
        counters().map(|c| c.WorkingSetSize as u64)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    fn status_kb(key: &str) -> Option<u64> {
        let text = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix(key) {
                let kb: u64 = rest
                    .trim()
                    .trim_start_matches(':')
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    pub fn private_rss_bytes() -> Option<u64> {
        status_kb("RssAnon")
    }

    pub fn working_set_bytes() -> Option<u64> {
        status_kb("VmRSS")
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod imp {
    pub fn private_rss_bytes() -> Option<u64> {
        None
    }
    pub fn working_set_bytes() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_can_weigh_itself() {
        let f = Footprint::measure();
        if cfg!(any(target_os = "windows", target_os = "linux")) {
            let p = f.private_bytes.expect("private RSS should be measurable here");
            // A test binary is at least a few MB and not tens of GB.
            assert!(p > 1024 * 1024, "{p} bytes is implausibly small");
            assert!(p < 64 * 1024 * 1024 * 1024, "{p} bytes is implausibly large");
            assert!(f.describe().contains("MB private"));
        }
    }

    #[test]
    fn the_budget_is_the_spec_s_number() {
        assert_eq!(APP_BUDGET_BYTES, 50 * 1024 * 1024);
    }
}
