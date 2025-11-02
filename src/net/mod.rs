pub mod dpdk_adapter;
pub mod epoll;
pub mod io_uring;

use anyhow::Result;
use std::os::unix::io::RawFd;

/// Common trait for network I/O backends
pub trait NetworkBackend: Send + Sync {
    fn accept(&mut self) -> Result<RawFd>;
    fn read(&mut self, fd: RawFd, buf: &mut [u8]) -> Result<usize>;
    fn write(&mut self, fd: RawFd, buf: &[u8]) -> Result<usize>;
    fn close(&mut self, fd: RawFd) -> Result<()>;
}
