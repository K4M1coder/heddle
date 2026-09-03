//! One `CreateProcessW`, bounded twice.
//!
//! The AppContainer and the Job Object compose on a **single** raw launch, and
//! it has to be raw: `std::process::Command` cannot carry
//! `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`, and it does not hand back the
//! child's thread handle, so it could neither enter the container nor be
//! resumed after a suspended start.
//!
//! The order — create suspended, assign to the job, *then* resume — is what
//! closes the assignment race completely: the child executes no instruction
//! before it belongs to the job.

use crate::{argv, win32_path, Captured, Run, Sandbox};
use std::ffi::c_void;
use std::fs::File;
use std::io::Read;
use std::mem::size_of;
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use std::thread::JoinHandle;
use std::time::Duration;
use win32job::{ExtendedLimitInfo, Job};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, SetHandleInformation, HANDLE, HANDLE_FLAGS, HANDLE_FLAG_INHERIT,
    HLOCAL, WAIT_OBJECT_0,
};
use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows::Win32::Security::{SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, ResumeThread, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

/// How much of a pipe is read per syscall. Not a cap on anything — the readers
/// drain until EOF regardless.
const READ_CHUNK: usize = 8 * 1024;

pub(crate) fn run(
    sandbox: &Sandbox,
    exe: &Path,
    args: &[String],
    stream_cap: usize,
    timeout: Duration,
) -> Result<Run, String> {
    let exe_path = win32_path(exe);
    // Refused before anything exists: an argument the command line cannot
    // carry is the model's mistake to hear about, not a Win32 error code.
    let mut command_line = argv::command_line(&exe_path, args)?;
    let exe_wide = wide(&exe_path);
    let cwd_wide = wide(&win32_path(&sandbox.root));
    let mut environment = environment_block();

    // Kill-on-close is what the timeout rests on: dropping this `Job` kills the
    // whole tree, grandchildren included. `win32job` 2.0.3 exposes no
    // process-count or job-memory limit — its inner
    // `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` is private — and a hard memory cap
    // would make a legitimate compiler fail rather than a runaway one, so the
    // wall clock is the only other bound.
    let mut limits = ExtendedLimitInfo::new();
    limits.limit_kill_on_job_close();
    let job = Job::create_with_limit_info(&limits)
        .map_err(|e| format!("the job object could not be created: {e}"))?;

    let pipes = Pipes::create()?;
    let attributes = Attributes::with_app_container(&sandbox.sid)?;

    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = pipes.stdin_read;
    startup.StartupInfo.hStdOutput = pipes.stdout_write;
    startup.StartupInfo.hStdError = pipes.stderr_write;
    startup.lpAttributeList = attributes.list;

    let mut process = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR(exe_wide.as_ptr()),
            // `CreateProcessW` may write into this buffer, which is why it is
            // owned and mutable rather than borrowed from a literal.
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            true,
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT,
            Some(environment.as_mut_ptr() as *const c_void),
            PCWSTR(cwd_wide.as_ptr()),
            &startup.StartupInfo as *const _,
            &mut process,
        )
    }
    .map_err(|e| format!("{} could not be launched: {e}", exe.display()))?;

    // **Before the readers, and unconditionally.** The child holds its own
    // copies now; if the parent kept these open the read ends would never see
    // EOF and the joins below would hang forever.
    let readers = pipes.hand_over(stream_cap);

    let assigned = job
        .assign_process(process.hProcess.0 as isize)
        .map_err(|e| format!("the child could not be bounded by a job object: {e}"));
    // Resumed only once it is inside the job. A failure here still has to reach
    // the reader joins, or the threads outlive the call.
    let started = assigned.and_then(|()| {
        // `ResumeThread` returns the previous suspend count, and `u32::MAX` on
        // failure — the one Win32 call in this file that reports through its
        // return value rather than a `Result`.
        if unsafe { ResumeThread(process.hThread) } == u32::MAX {
            Err("the child was launched but could not be resumed".to_string())
        } else {
            Ok(())
        }
    });

    let outcome = started.and_then(|()| wait(process.hProcess, timeout));
    // Dropping the job kills every surviving descendant, which is what closes
    // the last hold on the pipes' write ends. It happens on the success path
    // too: a descendant that outlived its parent would otherwise keep the
    // readers alive indefinitely, and this slice supports no background runs.
    drop(job);
    let (stdout, stderr) = readers.join();

    unsafe {
        let _ = CloseHandle(process.hThread);
        let _ = CloseHandle(process.hProcess);
    }

    Ok(Run {
        exit_code: outcome?,
        stdout,
        stderr,
    })
}

/// Blocks until the child exits or the clock runs out, and terminates it on the
/// latter.
///
/// # Safety
/// `process` must be a live process handle.
fn wait(process: HANDLE, timeout: Duration) -> Result<u32, String> {
    let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    if unsafe { WaitForSingleObject(process, millis) } != WAIT_OBJECT_0 {
        // The tree dies with the job a moment later; this closes the one
        // process the handle names, so the exit is not left to the drop alone.
        unsafe { TerminateProcess(process, 1) }.map_err(|e| {
            format!("the run exceeded {millis}ms and the child could not be killed: {e}")
        })?;
        return Err(format!(
            "the run exceeded the {}s limit and was terminated",
            timeout.as_secs()
        ));
    }
    let mut code = 0u32;
    unsafe { GetExitCodeProcess(process, &mut code) }
        .map_err(|e| format!("the child exited but its status is unreadable: {e}"))?;
    Ok(code)
}

/// A fixed, minimal environment: four variables, and each one earns its place.
///
/// `PATH` is the same two directories the caller's own executable resolution
/// searches and **not** the operator's ambient `PATH`, for the same reason — an
/// ambient value would make what the child can reach undecidable from the
/// configuration. There is deliberately no `TEMP`: the root is the only
/// writable place, and a tool that needs scratch space should fail loudly
/// rather than quietly litter the workspace.
///
/// **`LOCALAPPDATA` is not optional, and this was measured rather than
/// assumed.** An AppContainer's per-package state lives under
/// `%LOCALAPPDATA%\Packages\<profile name>\`, and process creation resolves
/// that path from the block being handed to the *child* — so omitting it fails
/// the whole launch with `ERROR_ENVVAR_NOT_FOUND` (0x800700CB), an error that
/// looks nothing like the cause. The child learns where the user profile is and
/// can do nothing with it: no directory there carries an ACE naming its SID.
///
/// **Sorted, case-insensitively.** An environment block is searched rather than
/// scanned, so leaving it unsorted is its own way to produce the same error.
fn environment_block() -> Vec<u16> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let mut variables = [
        format!("LOCALAPPDATA={local_app_data}"),
        format!(r"PATH={system_root}\System32;{system_root}"),
        "PATHEXT=.COM;.EXE;.BAT;.CMD".to_string(),
        format!("SystemRoot={system_root}"),
        format!("windir={system_root}"),
    ];
    variables.sort_by_key(|variable| variable.to_uppercase());

    let mut block: Vec<u16> = Vec::new();
    for variable in &variables {
        block.extend(variable.encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

/// The three pipes, split by **who ends up owning each end** rather than by
/// which direction it points.
///
/// That split is the whole correctness story here, and getting it backwards
/// fails in two different ways: a child-side end left non-inheritable makes
/// `CreateProcessW` refuse the launch outright with `ERROR_INVALID_PARAMETER`,
/// and a parent-side end left *inheritable* hands the child the write end of
/// its own stdin — which it then never sees EOF on.
struct Pipes {
    stdin_read: HANDLE,
    stdin_write: HANDLE,
    stdout_read: HANDLE,
    stdout_write: HANDLE,
    stderr_read: HANDLE,
    stderr_write: HANDLE,
}

impl Pipes {
    fn create() -> Result<Pipes, String> {
        let (stdin_read, stdin_write) = pipe()?;
        let (stdout_read, stdout_write) = pipe()?;
        let (stderr_read, stderr_write) = pipe()?;
        // `CreatePipe` with an inheritable descriptor makes **both** ends
        // inheritable, so the three the parent keeps are made private here. The
        // three the child gets — `stdin_read`, `stdout_write`, `stderr_write` —
        // must stay inheritable, because `STARTF_USESTDHANDLES` names them.
        for parent_end in [stdin_write, stdout_read, stderr_read] {
            unsafe { SetHandleInformation(parent_end, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) }
                .map_err(|e| format!("a pipe end could not be made private: {e}"))?;
        }
        Ok(Pipes {
            stdin_read,
            stdin_write,
            stdout_read,
            stdout_write,
            stderr_read,
            stderr_write,
        })
    }

    /// Closes every parent-side handle the child now owns and starts the two
    /// readers on the ones it does not.
    ///
    /// Consuming `self` is the point: after this there is no handle left in the
    /// parent that could keep a pipe from reaching EOF.
    fn hand_over(self, cap: usize) -> Readers {
        unsafe {
            let _ = CloseHandle(self.stdout_write);
            let _ = CloseHandle(self.stderr_write);
            let _ = CloseHandle(self.stdin_read);
            // The child's stdin is a pipe whose write end is gone, so a read on
            // it is an immediate EOF. That is what "no stdin" means here: not a
            // closed handle the child would fail on, an empty stream.
            let _ = CloseHandle(self.stdin_write);
        }
        Readers {
            stdout: drain(self.stdout_read, cap),
            stderr: drain(self.stderr_read, cap),
        }
    }
}

fn pipe() -> Result<(HANDLE, HANDLE), String> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: true.into(),
    };
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    unsafe { CreatePipe(&mut read, &mut write, Some(&attributes), 0) }
        .map_err(|e| format!("a pipe could not be created: {e}"))?;
    Ok((read, write))
}

struct Readers {
    stdout: JoinHandle<Captured>,
    stderr: JoinHandle<Captured>,
}

impl Readers {
    fn join(self) -> (Captured, Captured) {
        (
            self.stdout.join().unwrap_or_else(|_| lost("stdout")),
            self.stderr.join().unwrap_or_else(|_| lost("stderr")),
        )
    }
}

fn lost(stream: &str) -> Captured {
    Captured {
        text: format!("# {stream} could not be read: its reader panicked"),
        dropped_bytes: 0,
    }
}

/// One thread per stream, draining until EOF and **never stopping at the cap**.
///
/// Stopping early is the classic `CreateProcess` deadlock: a child that fills
/// its pipe blocks in `WriteFile` forever and the wait below never returns.
/// Past the cap the bytes are counted and discarded, so the number the model is
/// shown is the real one.
fn drain(read_end: HANDLE, cap: usize) -> JoinHandle<Captured> {
    // `HANDLE` is not `Send`; the raw address is, and the thread is the sole
    // owner of the handle from here on — the `File` closes it on drop.
    let raw = read_end.0 as isize;
    std::thread::spawn(move || {
        let mut pipe = unsafe { File::from_raw_handle(raw as *mut c_void) };
        let mut kept: Vec<u8> = Vec::new();
        let mut dropped_bytes = 0usize;
        let mut chunk = [0u8; READ_CHUNK];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let room = cap.saturating_sub(kept.len()).min(read);
                    kept.extend_from_slice(&chunk[..room]);
                    dropped_bytes += read - room;
                }
            }
        }
        Captured {
            // Lossy, deliberately: a console program writes the OEM code page,
            // not UTF-8, and a run whose output happened to contain one
            // non-ASCII byte must still reach the model rather than becoming an
            // encoding error.
            text: String::from_utf8_lossy(&kept).into_owned(),
            dropped_bytes,
        }
    })
}

/// The proc-thread attribute list carrying the AppContainer identity, and every
/// allocation whose lifetime that list depends on.
///
/// **`UpdateProcThreadAttribute` stores the pointer it is given, it does not
/// copy the value.** The `SECURITY_CAPABILITIES` must therefore outlive the
/// whole `CreateProcessW`, and it is boxed rather than held inline so that
/// moving this struct — which happens as soon as it is returned — does not move
/// the memory the list points at. Getting this wrong is not a leak: it is an
/// `ERROR_INVALID_PARAMETER` from the launch, measured.
struct Attributes {
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
    // Read by nothing. It exists because `list` points into it.
    _buffer: Vec<u8>,
    // Read by nothing but [`Drop`]. It exists because the attribute list points
    // at it, and it owns the `PSID` freed there.
    capabilities: Box<SECURITY_CAPABILITIES>,
}

impl Attributes {
    fn with_app_container(string_sid: &str) -> Result<Attributes, String> {
        let wide_sid = wide(string_sid);
        let mut sid = windows::Win32::Security::PSID::default();
        unsafe { ConvertStringSidToSidW(PCWSTR(wide_sid.as_ptr()), &mut sid) }
            .map_err(|e| format!("the app container identity {string_sid} does not parse: {e}"))?;

        // `CapabilityCount: 0` **is** the no-network decision at the launch
        // level, matching the profile's own zero capability SIDs: the Windows
        // Filtering Platform matches its permit filters on capability SIDs, and
        // this token carries none.
        let capabilities = Box::new(SECURITY_CAPABILITIES {
            AppContainerSid: sid,
            Capabilities: std::ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        });

        let mut size = 0usize;
        // Expected to fail with `ERROR_INSUFFICIENT_BUFFER`; the out-param is
        // the point, so the result is deliberately discarded.
        let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
        let mut buffer = vec![0u8; size];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(buffer.as_mut_ptr() as *mut c_void);

        let built = unsafe {
            InitializeProcThreadAttributeList(Some(list), 1, None, &mut size).and_then(|()| {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                    Some(&*capabilities as *const _ as *const c_void),
                    size_of::<SECURITY_CAPABILITIES>(),
                    None,
                    None,
                )
            })
        };
        if let Err(e) = built {
            unsafe { LocalFree(Some(HLOCAL(sid.0))) };
            return Err(format!("the app container attribute could not be set: {e}"));
        }

        Ok(Attributes {
            list,
            _buffer: buffer,
            capabilities,
        })
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.list);
            // `ConvertStringSidToSidW` allocates with `LocalAlloc`, so this is
            // `LocalFree` and **not** `FreeSid` — which is what the SID from
            // `CreateAppContainerProfile` needs. Two allocators, two frees.
            LocalFree(Some(HLOCAL(self.capabilities.AppContainerSid.0)));
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
