use super::{
    command_line,
    os_error::{self, OsApi},
};
use crate::application::{
    error::{EngineError, EngineResult},
    process::{ProcessOutcome, ProcessPort, ProcessRequest},
};
use std::{
    io::{self, Read},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct NativeProcess;

#[derive(Clone, Copy)]
struct CaptureKiller {
    #[cfg(windows)]
    handle: usize,
    #[cfg(unix)]
    process: libc::pid_t,
}
impl CaptureKiller {
    fn new(child: &Child) -> Self {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            Self {
                handle: child.as_raw_handle() as usize,
            }
        }
        #[cfg(unix)]
        {
            Self {
                process: child.id() as libc::pid_t,
            }
        }
    }
    fn terminate(self) {
        // RunningChild retains the owning handle until both readers have joined.
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Threading::TerminateProcess(self.handle as _, 1);
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(self.process, libc::SIGKILL);
        }
    }
}

struct RunningChild {
    child: Option<Child>,
    stdout: Option<JoinHandle<io::Result<Vec<u8>>>>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
}
impl Drop for RunningChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        if let Some(reader) = self.stdout.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr.take() {
            let _ = reader.join();
        }
    }
}

fn reader(
    mut pipe: impl Read + Send + 'static,
    limit: Option<usize>,
    overflow: Arc<AtomicBool>,
    killer: CaptureKiller,
) -> JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut result = Vec::new();
        let mut buffer = [0u8; 65536];
        loop {
            let mut count = 0;
            while count < buffer.len() {
                match pipe.read(&mut buffer[count..]) {
                    Ok(0) => break,
                    Ok(read) => count += read,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
            if count == 0 {
                return Ok(result);
            }
            if limit.is_some_and(|maximum| count > maximum.saturating_sub(result.len())) {
                killer.terminate();
                overflow.store(true, Ordering::Release);
                return Ok(result);
            }
            result.extend_from_slice(&buffer[..count]);
        }
    })
}

fn finish_reader(reader: JoinHandle<io::Result<Vec<u8>>>) -> EngineResult<Vec<u8>> {
    reader
        .join()
        .map_err(|_| io::Error::other("process capture reader panicked"))?
        .map_err(Into::into)
}

#[cfg(windows)]
fn exit_code(status: ExitStatus) -> i64 {
    // Rust exposes the DWORD through a signed code; Python retains its unsigned value.
    i64::from(status.code().unwrap() as u32)
}
#[cfg(unix)]
fn exit_code(status: ExitStatus) -> i64 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .map(i64::from)
        .unwrap_or_else(|| -i64::from(status.signal().unwrap()))
}

impl ProcessPort for NativeProcess {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        if request.arguments.is_empty() {
            return Err(EngineError::python("IndexError", "list index out of range"));
        }
        if request
            .arguments
            .iter()
            .any(|argument| argument.0.contains(&0))
        {
            return Err(EngineError::python("ValueError", "embedded null character"));
        }
        let arguments = request
            .arguments
            .iter()
            .map(command_line::native_string)
            .collect::<EngineResult<Vec<_>>>()?;
        let mut command = Command::new(&arguments[0]);
        command
            .args(&arguments[1..])
            .current_dir(request.directory)
            .stdout(Stdio::piped())
            .stderr(if request.capture_stderr {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        let child = command.spawn().map_err(|error| {
            #[cfg(windows)]
            let filename = None;
            #[cfg(unix)]
            let filename = Some(std::path::Path::new(&arguments[0]));
            os_error::with_path(error, OsApi::Native, filename, None)
        })?;
        let mut child = RunningChild {
            child: Some(child),
            stdout: None,
            stderr: None,
        };
        let process = child.child.as_mut().unwrap();
        let killer = CaptureKiller::new(process);
        let overflow = Arc::new(AtomicBool::new(false));
        child.stdout = Some(reader(
            process.stdout.take().unwrap(),
            request.stdout_limit,
            overflow.clone(),
            killer,
        ));
        child.stderr = process
            .stderr
            .take()
            .map(|pipe| reader(pipe, None, overflow.clone(), killer));
        let started = Instant::now();
        let mut status = None;
        let mut timed_out = false;
        loop {
            if overflow.load(Ordering::Acquire) {
                break;
            }
            if status.is_none() {
                status = process.try_wait()?;
            }
            if status.is_some()
                && child.stdout.as_ref().unwrap().is_finished()
                && child.stderr.as_ref().is_none_or(JoinHandle::is_finished)
            {
                break;
            }
            let elapsed = started.elapsed();
            if elapsed >= request.timeout {
                timed_out = true;
                break;
            }
            thread::sleep(Duration::from_millis(2).min(request.timeout - elapsed));
        }
        if status.is_none() {
            let _ = process.kill();
        }
        let final_status = process.wait()?;
        let stdout = finish_reader(child.stdout.take().unwrap())?;
        let stderr = child
            .stderr
            .take()
            .map(finish_reader)
            .transpose()?
            .unwrap_or_default();
        child.child.take();
        if overflow.load(Ordering::Acquire) {
            Ok(ProcessOutcome::OutputLimit)
        } else if timed_out {
            Ok(ProcessOutcome::TimedOut { stdout, stderr })
        } else {
            Ok(ProcessOutcome::Completed {
                exit_code: exit_code(final_status),
                stdout,
                stderr,
            })
        }
    }
}
