use anyhow::{anyhow, Result};
use nix::sys::mman::{mmap, munmap, MapFlags, ProtFlags};
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::ptr::NonNull;
use std::slice;
use tracing::debug;

/// Memory-mapped file for zero-copy serving
pub struct MmapFile {
    ptr: NonNull<u8>,
    len: usize,
    _file: File,
}

impl MmapFile {
    /// Create a new memory-mapped file
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path.as_ref())
            .map_err(|e| anyhow!("Failed to open file: {}", e))?;

        let metadata = file
            .metadata()
            .map_err(|e| anyhow!("Failed to get file metadata: {}", e))?;

        let len = metadata.len() as usize;
        if len == 0 {
            return Err(anyhow!("Cannot mmap empty file"));
        }

        // SAFETY: We're mapping a valid file with appropriate protections.
        // The mapping will remain valid as long as the File is alive.
        let ptr_raw = unsafe {
            mmap(
                None,
                len.try_into().unwrap(),
                ProtFlags::PROT_READ,
                MapFlags::MAP_PRIVATE,
                &file,
                0,
            )
            .map_err(|e| anyhow!("mmap failed: {}", e))?
        };

        let ptr = NonNull::new(ptr_raw.as_ptr() as *mut u8)
            .ok_or_else(|| anyhow!("mmap returned null pointer"))?;

        debug!("Memory-mapped file: {} ({} bytes)", path.as_ref().display(), len);

        Ok(Self {
            ptr,
            len,
            _file: file,
        })
    }

    /// Get the memory-mapped data as a slice
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: We've validated that ptr and len are correct during construction,
        // and the lifetime is tied to self
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// Get the length of the mapped region
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the mapped region is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a pointer to the mapped memory (for use with sendfile/splice)
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }
}

impl Drop for MmapFile {
    fn drop(&mut self) {
        // SAFETY: We're unmapping the same region we mapped
        unsafe {
            let _ = munmap(self.ptr.cast(), self.len);
        }
        debug!("Unmapped file ({} bytes)", self.len);
    }
}

// SAFETY: MmapFile can be sent between threads as long as the File is Send
unsafe impl Send for MmapFile {}
// SAFETY: Multiple threads can read from the same mmap region
unsafe impl Sync for MmapFile {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_mmap_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = b"Hello, mmap world!";
        temp_file.write_all(content).unwrap();
        temp_file.flush().unwrap();

        let mmap = MmapFile::new(temp_file.path()).unwrap();
        assert_eq!(mmap.as_slice(), content);
        assert_eq!(mmap.len(), content.len());
    }

    #[test]
    fn test_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let result = MmapFile::new(temp_file.path());
        assert!(result.is_err());
    }
}
