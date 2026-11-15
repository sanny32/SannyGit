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
}

impl<T: Send + 'static> Worker<T> {
    pub fn spawn(
        job: impl FnOnce(&AtomicBool) -> Result<T, String> + Send + 'static,
    ) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = thread::Builder::new()
            .name("sannygit-pipe".into())
            .spawn(move || job(&flag))?;
        Ok(Self {
            handle: Some(handle),
            stop,
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
                    fn CancelSynchronousIo(thread: *mut std::ffi::c_void) -> i32;
                }
                // SAFETY: the join handle owns a live native thread handle.
                // This worker performs only pipe I/O. Repeat to cover the race
                // between checking stop and entering a synchronous read/write.
                unsafe {
                    CancelSynchronousIo(handle.as_raw_handle());
                }
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
