//! What Core asks of the operating system beyond starting a program
//! (`crate::child`): that what it starts ends with it, and that the computer
//! stays awake while a long run needs it. Windows only; elsewhere both are
//! nothing.
//!
//! **Unsafe code**, isolated here as in `footprint`: a few `kernel32` calls,
//! declared here as `footprint` declares its own, so no crate is added (the
//! job object was approved 2026-09-24). Each call is checked; a failure is
//! logged and the program goes on as it did before this module existed.
//!
//! **A job object that kills on close** (docs/DECISIONS.md, 2026-10-12): every
//! engine Core starts joins one job, created once per process and never
//! closed by hand. When Core's process ends, however it ends (Ctrl+C, a stop
//! from Task Manager, a crash), Windows closes the job's last handle and ends
//! every process in it. An engine left running after its Core held the card
//! and made the next hardware reading wrong (2026-09-23); the orphan-engine
//! file still covers a power cut, which ends both at once anyway.
//!
//! **Awake while measuring**: `SetThreadExecutionState` with
//! `ES_SYSTEM_REQUIRED | ES_CONTINUOUS` keeps Windows from sleeping while the
//! thread that asked lives, without changing any setting; the guard says
//! `ES_CONTINUOUS` alone when it is dropped, and Windows forgets the request
//! when the process ends in any case.

/// Make `child` end when this process ends (Windows); nothing elsewhere.
pub fn end_with_this_process(child: &std::process::Child) {
    #[cfg(windows)]
    imp::adopt(child);
    #[cfg(not(windows))]
    let _ = child;
}

/// Keeps the computer from sleeping until it is dropped (Windows); the
/// display may still turn off. Nothing elsewhere.
pub struct Awake {
    _not_send: std::marker::PhantomData<*const ()>,
}

/// Ask that the computer stay awake while the returned guard lives, on this
/// thread: `localspace measure` holds it for its whole run.
pub fn stay_awake() -> Awake {
    #[cfg(windows)]
    imp::awake(true);
    Awake {
        _not_send: std::marker::PhantomData,
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        #[cfg(windows)]
        imp::awake(false);
    }
}

/// Whether `pid` is in this process's job, which ends it when this process
/// ends: what a test asks of an engine Core started.
#[cfg(windows)]
pub fn ends_with_this_process(pid: u32) -> bool {
    imp::ends_with_this_process(pid)
}

#[cfg(windows)]
#[allow(non_snake_case)]
mod imp {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    type Handle = *mut c_void;

    /// `JOBOBJECT_BASIC_LIMIT_INFORMATION`.
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        PerProcessUserTimeLimit: i64,
        PerJobUserTimeLimit: i64,
        LimitFlags: u32,
        MinimumWorkingSetSize: usize,
        MaximumWorkingSetSize: usize,
        ActiveProcessLimit: u32,
        Affinity: usize,
        PriorityClass: u32,
        SchedulingClass: u32,
    }

    /// `IO_COUNTERS`.
    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        ReadOperationCount: u64,
        WriteOperationCount: u64,
        OtherOperationCount: u64,
        ReadTransferCount: u64,
        WriteTransferCount: u64,
        OtherTransferCount: u64,
    }

    /// `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`.
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        BasicLimitInformation: BasicLimits,
        IoInfo: IoCounters,
        ProcessMemoryLimit: usize,
        JobMemoryLimit: usize,
        PeakProcessMemoryUsed: usize,
        PeakJobMemoryUsed: usize,
    }

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    const ES_CONTINUOUS: u32 = 0x8000_0000;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, size: u32) -> i32;
        fn QueryInformationJobObject(
            job: Handle,
            class: i32,
            info: *mut c_void,
            size: u32,
            written: *mut u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn IsProcessInJob(process: Handle, job: Handle, inside: *mut i32) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn SetThreadExecutionState(flags: u32) -> u32;
    }

    /// The job, as an address-sized number: a raw handle is not `Send`, and
    /// this one lives as long as the process. `None` when it could not be made.
    static JOB: OnceLock<Option<usize>> = OnceLock::new();

    fn job() -> Option<Handle> {
        let made = JOB.get_or_init(|| {
            // SAFETY: no security attributes and no name: a new, unnamed job
            // owned by this process; a null handle is a failure, checked.
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                tracing::warn!("engine: no job object: {}", std::io::Error::last_os_error());
                return None;
            }
            let mut limits = ExtendedLimits::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: `job` is the handle just made; the pointer and size are
            // those of `limits`, which outlives the call.
            let set = unsafe {
                SetInformationJobObject(
                    job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    (&limits as *const ExtendedLimits).cast(),
                    std::mem::size_of::<ExtendedLimits>() as u32,
                )
            };
            if set == 0 {
                tracing::warn!(
                    "engine: the job object could not be set to end its processes: {}",
                    std::io::Error::last_os_error()
                );
                // SAFETY: the handle is ours and not used again.
                unsafe { CloseHandle(job) };
                return None;
            }
            Some(job as usize)
        });
        made.map(|job| job as Handle)
    }

    pub fn adopt(child: &std::process::Child) {
        let Some(job) = job() else { return };
        // SAFETY: the child's handle is valid while `child` is borrowed; the
        // job's handle lives as long as the process.
        let done = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as Handle) };
        if done == 0 {
            tracing::warn!(
                "engine: process {} did not join the job object; it may outlive Core: {}",
                child.id(),
                std::io::Error::last_os_error()
            );
        }
    }

    pub fn awake(on: bool) {
        let flags = if on {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        // SAFETY: a plain flag value; the call takes no pointer.
        if unsafe { SetThreadExecutionState(flags) } == 0 {
            tracing::warn!("the request to keep the computer awake was refused");
        }
    }

    pub fn ends_with_this_process(pid: u32) -> bool {
        let Some(job) = job() else { return false };
        // SAFETY: opening a process for a query only; a null handle is a
        // failure, checked, and the handle is closed before returning.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return false;
        }
        let mut inside = 0;
        // SAFETY: both handles are valid; `inside` outlives the call.
        let asked = unsafe { IsProcessInJob(process, job, &mut inside) };
        // SAFETY: the handle is ours and not used again.
        unsafe { CloseHandle(process) };
        if asked == 0 || inside == 0 {
            return false;
        }
        let mut limits = ExtendedLimits::default();
        let mut written = 0u32;
        // SAFETY: `job` is valid; the pointer and size are those of `limits`.
        let read = unsafe {
            QueryInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&mut limits as *mut ExtendedLimits).cast(),
                std::mem::size_of::<ExtendedLimits>() as u32,
                &mut written,
            )
        };
        read != 0
            && limits.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE != 0
    }
}
