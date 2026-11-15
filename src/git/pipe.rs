//! Pipe workers that can be stopped even when a descendant outlives Git.
use std::{
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) struct Worker<T> {
    handle: Option<JoinHandle<Result<T, String>>>,
    stop: Arc<AtomicBool>,
    #[cfg(windows)]
    cancel: Arc<std::sync::Mutex<Option<std::os::windows::io::OwnedHandle>>>,
}

#[cfg(windows)]
pub(super) trait Pipe: std::os::windows::io::AsHandle {}
#[cfg(windows)]
impl<T: std::os::windows::io::AsHandle> Pipe for T {}
#[cfg(unix)]
pub(super) trait Pipe: std::os::fd::AsRawFd {}
#[cfg(unix)]
impl<T: std::os::fd::AsRawFd> Pipe for T {}

impl<T: Send + 'static> Worker<T> {
    pub fn spawn<P: Pipe + Send + 'static>(
        pipe: P,
        job: impl FnOnce(P, &AtomicBool) -> Result<T, String> + Send + 'static,
    ) -> io::Result<Self> {
        prepare(&pipe)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        #[cfg(windows)]
        let cancel = Arc::new(std::sync::Mutex::new(Some(
            pipe.as_handle().try_clone_to_owned()?,
        )));
        #[cfg(windows)]
        let worker_cancel = cancel.clone();
        let handle = thread::Builder::new()
            .name("sannygit-pipe".into())
            .spawn(move || {
                let result = job(pipe, &flag);
                // Close the duplicate too: a completed stdin writer must send EOF.
                #[cfg(windows)]
                worker_cancel
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                result
            })?;
        Ok(Self {
            handle: Some(handle),
            stop,
            #[cfg(windows)]
            cancel,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn finish(mut self) -> thread::Result<Result<T, String>> {
        match self.handle.take() {
            Some(handle) => handle.join(),
            None => Ok(Err("Git pipe worker is unavailable".into())),
        }
    }
}

impl<T> Drop for Worker<T> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            #[cfg(windows)]
            while !handle.is_finished() {
                use std::os::windows::io::AsRawHandle;
                #[link(name = "kernel32")]
                extern "system" {
                    fn CancelIoEx(
                        pipe: *mut std::ffi::c_void,
                        overlapped: *mut std::ffi::c_void,
                    ) -> i32;
                }
                let cancel = self
                    .cancel
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(pipe) = &*cancel {
                    // SAFETY: this duplicate keeps the pipe object alive while
                    // cancelling its I/O. Rust child pipes use overlapped I/O;
                    // thread cancellation alone cannot interrupt those waits.
                    unsafe {
                        CancelIoEx(pipe.as_raw_handle(), std::ptr::null_mut());
                    }
                }
                drop(cancel);
                thread::sleep(Duration::from_millis(10));
            }
            let _ = handle.join();
        }
    }
}

#[cfg(unix)]
pub(super) fn prepare(pipe: &impl std::os::fd::AsRawFd) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    // SAFETY: fd is borrowed from a live, exclusively owned child pipe. These
    // fcntl commands take integer flags and do not access pointer arguments.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags == -1 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn prepare<T>(_: &T) -> io::Result<()> {
    Ok(())
}

pub(super) fn capture(mut reader: impl Read, stop: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    while !stop.load(Ordering::Relaxed) {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                // Drain even after reaching the limit so Git cannot block.
                let remaining = (super::OUTPUT_LIMIT + 1).saturating_sub(bytes.len());
                bytes.extend_from_slice(&buffer[..count.min(remaining)]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(bytes)
}

pub(super) fn write(
    mut writer: impl Write,
    mut bytes: &[u8],
    stop: &AtomicBool,
) -> Result<(), String> {
    while !bytes.is_empty() && !stop.load(Ordering::Relaxed) {
        match writer.write(bytes) {
            Ok(0) => return Err("Cannot send input to Git: pipe closed".into()),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("Cannot send input to Git: {error}")),
        }
    }
    Ok(())
}
