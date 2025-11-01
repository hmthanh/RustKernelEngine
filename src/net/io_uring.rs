use anyhow::{anyhow, Result};
use std::os::unix::io::RawFd;
use tracing::{debug, info};

#[cfg(feature = "io_uring")]
use io_uring::{opcode, types, IoUring, Submitter};

/// Check if io_uring is available on the system
pub fn is_available() -> bool {
    #[cfg(feature = "io_uring")]
    {
        // Try to create a small io_uring instance
        match IoUring::new(8) {
            Ok(_) => {
                info!("io_uring is available");
                true
            }
            Err(e) => {
                debug!("io_uring not available: {}", e);
                false
            }
        }
    }
    #[cfg(not(feature = "io_uring"))]
    {
        false
    }
}

#[cfg(feature = "io_uring")]
pub struct IoUringBackend {
    ring: IoUring,
    queue_depth: u32,
}

#[cfg(feature = "io_uring")]
impl IoUringBackend {
    /// Create a new io_uring backend with specified queue depth
    pub fn new(queue_depth: u32) -> Result<Self> {
        let ring = IoUring::new(queue_depth)
            .map_err(|e| anyhow!("Failed to create io_uring: {}", e))?;

        info!(
            "io_uring initialized with queue depth: {}",
            queue_depth
        );

        Ok(Self { ring, queue_depth })
    }

    /// Submit a read operation using io_uring (zero-copy)
    pub fn submit_read(&mut self, fd: RawFd, buf: *mut u8, len: usize, offset: u64) -> Result<()> {
        let read_e = opcode::Read::new(types::Fd(fd), buf, len as u32)
            .offset(offset)
            .build()
            .user_data(fd as u64);

        // SAFETY: We're using io_uring's unsafe API for zero-copy operations.
        // The buffer must remain valid until the operation completes.
        unsafe {
            self.ring
                .submission()
                .push(&read_e)
                .map_err(|e| anyhow!("Failed to push read operation: {}", e))?;
        }

        self.ring
            .submit()
            .map_err(|e| anyhow!("Failed to submit io_uring operations: {}", e))?;

        Ok(())
    }

    /// Submit a write operation using io_uring (zero-copy)
    pub fn submit_write(&mut self, fd: RawFd, buf: *const u8, len: usize, offset: u64) -> Result<()> {
        let write_e = opcode::Write::new(types::Fd(fd), buf, len as u32)
            .offset(offset)
            .build()
            .user_data(fd as u64);

        // SAFETY: We're using io_uring's unsafe API for zero-copy operations.
        // The buffer must remain valid until the operation completes.
        unsafe {
            self.ring
                .submission()
                .push(&write_e)
                .map_err(|e| anyhow!("Failed to push write operation: {}", e))?;
        }

        self.ring
            .submit()
            .map_err(|e| anyhow!("Failed to submit io_uring operations: {}", e))?;

        Ok(())
    }

    /// Submit an accept operation using io_uring
    pub fn submit_accept(&mut self, listen_fd: RawFd, addr: *mut libc::sockaddr, addrlen: *mut libc::socklen_t) -> Result<()> {
        let accept_e = opcode::Accept::new(types::Fd(listen_fd), addr, addrlen)
            .build()
            .user_data(listen_fd as u64);

        // SAFETY: We're using io_uring's unsafe API for zero-copy operations.
        unsafe {
            self.ring
                .submission()
                .push(&accept_e)
                .map_err(|e| anyhow!("Failed to push accept operation: {}", e))?;
        }

        self.ring
            .submit()
            .map_err(|e| anyhow!("Failed to submit io_uring operations: {}", e))?;

        Ok(())
    }

    /// Submit a sendfile operation using io_uring (zero-copy file transfer)
    pub fn submit_sendfile(&mut self, out_fd: RawFd, in_fd: RawFd, offset: u64, len: usize) -> Result<()> {
        let splice_e = opcode::Splice::new(
            types::Fd(in_fd),
            offset as i64,
            types::Fd(out_fd),
            -1,
            len as u32,
        )
        .build()
        .user_data(out_fd as u64);

        // SAFETY: We're using io_uring's unsafe API for zero-copy operations.
        unsafe {
            self.ring
                .submission()
                .push(&splice_e)
                .map_err(|e| anyhow!("Failed to push splice operation: {}", e))?;
        }

        self.ring
            .submit()
            .map_err(|e| anyhow!("Failed to submit io_uring operations: {}", e))?;

        Ok(())
    }

    /// Wait for completion events
    pub fn wait_for_completion(&mut self) -> Result<Vec<(u64, i32)>> {
        let mut completions = Vec::new();

        self.ring
            .submit_and_wait(1)
            .map_err(|e| anyhow!("Failed to wait for completions: {}", e))?;

        let mut cqueue = self.ring.completion();
        for cqe in &mut cqueue {
            completions.push((cqe.user_data(), cqe.result()));
        }

        Ok(completions)
    }

    /// Batch submit multiple operations
    pub fn batch_submit(&mut self) -> Result<usize> {
        self.ring
            .submit()
            .map_err(|e| anyhow!("Failed to batch submit: {}", e))
    }
}

#[cfg(not(feature = "io_uring"))]
pub struct IoUringBackend;

#[cfg(not(feature = "io_uring"))]
impl IoUringBackend {
    pub fn new(_queue_depth: u32) -> Result<Self> {
        Err(anyhow!("io_uring feature not enabled"))
    }
}
