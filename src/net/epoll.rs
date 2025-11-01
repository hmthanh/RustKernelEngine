use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::os::unix::io::RawFd;
use tracing::{debug, info};

/// Epoll-based network backend (fallback when io_uring is unavailable)
pub struct EpollBackend {
    epoll_fd: RawFd,
    events: Vec<libc::epoll_event>,
    max_events: usize,
    connections: HashMap<RawFd, ConnectionState>,
}

#[derive(Debug, Clone)]
struct ConnectionState {
    read_buffer: Vec<u8>,
    write_buffer: Vec<u8>,
    bytes_read: usize,
    bytes_written: usize,
}

impl EpollBackend {
    /// Create a new epoll backend
    pub fn new(max_events: usize) -> Result<Self> {
        // SAFETY: Creating epoll instance
        let epoll_fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        
        if epoll_fd < 0 {
            return Err(anyhow!("Failed to create epoll: {}", std::io::Error::last_os_error()));
        }

        info!("Epoll backend initialized with max events: {}", max_events);

        Ok(Self {
            epoll_fd,
            events: vec![unsafe { std::mem::zeroed() }; max_events],
            max_events,
            connections: HashMap::new(),
        })
    }

    /// Register a file descriptor with epoll
    pub fn register(&mut self, fd: RawFd, events: u32) -> Result<()> {
        let mut event = libc::epoll_event {
            events,
            u64: fd as u64,
        };

        // SAFETY: Adding fd to epoll
        let result = unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event)
        };

        if result < 0 {
            return Err(anyhow!("Failed to add fd to epoll: {}", std::io::Error::last_os_error()));
        }

        self.connections.insert(
            fd,
            ConnectionState {
                read_buffer: Vec::with_capacity(8192),
                write_buffer: Vec::new(),
                bytes_read: 0,
                bytes_written: 0,
            },
        );

        debug!("Registered fd {} with epoll", fd);
        Ok(())
    }

    /// Modify epoll event for a file descriptor
    pub fn modify(&mut self, fd: RawFd, events: u32) -> Result<()> {
        let mut event = libc::epoll_event {
            events,
            u64: fd as u64,
        };

        // SAFETY: Modifying epoll event
        let result = unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_MOD, fd, &mut event)
        };

        if result < 0 {
            return Err(anyhow!("Failed to modify epoll event: {}", std::io::Error::last_os_error()));
        }

        debug!("Modified fd {} in epoll", fd);
        Ok(())
    }

    /// Unregister a file descriptor from epoll
    pub fn unregister(&mut self, fd: RawFd) -> Result<()> {
        // SAFETY: Removing fd from epoll
        let result = unsafe {
            libc::epoll_ctl(self.epoll_fd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut())
        };

        if result < 0 {
            return Err(anyhow!("Failed to delete fd from epoll: {}", std::io::Error::last_os_error()));
        }

        self.connections.remove(&fd);
        debug!("Unregistered fd {} from epoll", fd);
        Ok(())
    }

    /// Wait for events with timeout
    pub fn wait(&mut self, timeout_ms: i32) -> Result<usize> {
        // SAFETY: Waiting for epoll events
        let num_events = unsafe {
            libc::epoll_wait(
                self.epoll_fd,
                self.events.as_mut_ptr(),
                self.max_events as i32,
                timeout_ms,
            )
        };

        if num_events < 0 {
            return Err(anyhow!("Epoll wait failed: {}", std::io::Error::last_os_error()));
        }

        Ok(num_events as usize)
    }

    /// Read data from a file descriptor
    pub fn read(&mut self, fd: RawFd, buf: &mut [u8]) -> Result<usize> {
        // SAFETY: Reading from valid fd
        let bytes = unsafe {
            libc::read(
                fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            )
        };

        if bytes < 0 {
            return Err(anyhow!("Read failed: {}", std::io::Error::last_os_error()));
        }

        debug!("Read {} bytes from fd {}", bytes, fd);
        Ok(bytes as usize)
    }

    /// Write data to a file descriptor
    pub fn write(&mut self, fd: RawFd, buf: &[u8]) -> Result<usize> {
        // SAFETY: Writing to valid fd
        let bytes = unsafe {
            libc::write(
                fd,
                buf.as_ptr() as *const libc::c_void,
                buf.len(),
            )
        };

        if bytes < 0 {
            return Err(anyhow!("Write failed: {}", std::io::Error::last_os_error()));
        }

        debug!("Wrote {} bytes to fd {}", bytes, fd);
        Ok(bytes as usize)
    }

    /// Accept a new connection
    pub fn accept(&self, listen_fd: RawFd) -> Result<RawFd> {
        // SAFETY: Accepting on valid fd
        let client_fd = unsafe {
            libc::accept(listen_fd, std::ptr::null_mut(), std::ptr::null_mut())
        };

        if client_fd < 0 {
            return Err(anyhow!("Accept failed: {}", std::io::Error::last_os_error()));
        }

        debug!("Accepted new connection on fd {}", client_fd);
        Ok(client_fd)
    }

    /// Close a file descriptor
    pub fn close(&mut self, fd: RawFd) -> Result<()> {
        self.unregister(fd)?;
        
        // SAFETY: Closing valid fd
        unsafe {
            if libc::close(fd) < 0 {
                return Err(anyhow!("Close failed: {}", std::io::Error::last_os_error()));
            }
        }

        debug!("Closed fd {}", fd);
        Ok(())
    }

    /// Batch process multiple events
    pub fn process_events(&mut self, timeout_ms: i32) -> Result<Vec<(RawFd, u32)>> {
        let num_events = self.wait(timeout_ms)?;
        let mut results = Vec::new();

        for i in 0..num_events {
            let event = self.events[i];
            let fd = event.u64 as RawFd;
            let flags = event.events;
            results.push((fd, flags));
        }

        Ok(results)
    }

    /// Get event data
    pub fn get_event_fd(&self, idx: usize) -> RawFd {
        self.events[idx].u64 as RawFd
    }

    /// Get event flags
    pub fn get_event_flags(&self, idx: usize) -> u32 {
        self.events[idx].events
    }
}

impl Drop for EpollBackend {
    fn drop(&mut self) {
        // Close all registered connections
        let fds: Vec<RawFd> = self.connections.keys().copied().collect();
        for fd in fds {
            let _ = self.close(fd);
        }
        
        // Close epoll fd
        unsafe {
            libc::close(self.epoll_fd);
        }
    }
}
