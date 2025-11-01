use anyhow::{anyhow, Result};
use std::fs::File;
use std::os::unix::io::{AsRawFd, RawFd};
use tracing::debug;

/// Send file using zero-copy sendfile syscall
/// This is the most efficient way to transfer file content to a socket
pub fn send_file(out_fd: RawFd, in_file: &File, offset: Option<i64>, count: usize) -> Result<usize> {
    let in_fd = in_file.as_raw_fd();
    
    // SAFETY: We're calling sendfile with valid file descriptors.
    // The offset pointer is either null or points to a valid i64.
    let sent = unsafe {
        let mut off = offset.unwrap_or(0);
        let offset_ptr = if offset.is_some() {
            &mut off as *mut i64
        } else {
            std::ptr::null_mut()
        };

        libc::sendfile(out_fd, in_fd, offset_ptr, count)
    };

    if sent < 0 {
        Err(anyhow!(
            "sendfile failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        debug!("Sent {} bytes via sendfile", sent);
        Ok(sent as usize)
    }
}

/// Send file using zero-copy sendfile64 syscall (for large files)
#[cfg(target_pointer_width = "64")]
pub fn send_file64(
    out_fd: RawFd,
    in_file: &File,
    offset: Option<i64>,
    count: usize,
) -> Result<usize> {
    // On 64-bit systems, sendfile and sendfile64 are typically the same
    send_file(out_fd, in_file, offset, count)
}

/// Send entire file to socket using sendfile in a loop
pub fn send_entire_file(out_fd: RawFd, in_file: &File) -> Result<usize> {
    let metadata = in_file
        .metadata()
        .map_err(|e| anyhow!("Failed to get file metadata: {}", e))?;
    
    let file_size = metadata.len() as usize;
    let mut total_sent = 0;
    let mut offset = 0i64;

    while total_sent < file_size {
        let remaining = file_size - total_sent;
        let to_send = remaining.min(1024 * 1024 * 1024); // Send max 1GB at a time

        match send_file(out_fd, in_file, Some(offset), to_send) {
            Ok(sent) => {
                if sent == 0 {
                    // No more data to send
                    break;
                }
                total_sent += sent;
                offset += sent as i64;
            }
            Err(e) => {
                if total_sent > 0 {
                    // Partial send
                    debug!("Partial send: {} of {} bytes", total_sent, file_size);
                    return Ok(total_sent);
                }
                return Err(e);
            }
        }
    }

    debug!("Sent entire file: {} bytes", total_sent);
    Ok(total_sent)
}

/// Use splice to transfer data between file descriptors (zero-copy)
pub fn splice_data(
    fd_in: RawFd,
    off_in: Option<i64>,
    fd_out: RawFd,
    off_out: Option<i64>,
    len: usize,
    flags: u32,
) -> Result<usize> {
    // SAFETY: We're calling splice with valid file descriptors.
    let result = unsafe {
        let mut off_in_val = off_in.unwrap_or(0);
        let mut off_out_val = off_out.unwrap_or(0);
        
        let off_in_ptr = if off_in.is_some() {
            &mut off_in_val as *mut i64
        } else {
            std::ptr::null_mut()
        };
        
        let off_out_ptr = if off_out.is_some() {
            &mut off_out_val as *mut i64
        } else {
            std::ptr::null_mut()
        };

        libc::splice(fd_in, off_in_ptr, fd_out, off_out_ptr, len, flags)
    };

    if result < 0 {
        Err(anyhow!(
            "splice failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        debug!("Spliced {} bytes", result);
        Ok(result as usize)
    }
}

/// Use vmsplice for zero-copy writes from user memory to pipe
pub fn vmsplice_write(fd: RawFd, buffers: &[&[u8]], flags: u32) -> Result<usize> {
    let mut iovecs: Vec<libc::iovec> = buffers
        .iter()
        .map(|buf| libc::iovec {
            iov_base: buf.as_ptr() as *mut libc::c_void,
            iov_len: buf.len(),
        })
        .collect();

    // SAFETY: We're calling vmsplice with valid iovec structures
    let result = unsafe {
        libc::vmsplice(
            fd,
            iovecs.as_mut_ptr(),
            iovecs.len(),
            flags,
        )
    };

    if result < 0 {
        Err(anyhow!(
            "vmsplice failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        debug!("Vmspliced {} bytes", result);
        Ok(result as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use tempfile::NamedTempFile;

    #[test]
    fn test_sendfile() {
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let mut temp_file = NamedTempFile::new().unwrap();
        let content = b"Hello from sendfile!";
        temp_file.write_all(content).unwrap();
        temp_file.flush().unwrap();

        let file = temp_file.reopen().unwrap();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).unwrap();
            assert_eq!(buf, content);
        });

        let stream = std::net::TcpStream::connect(addr).unwrap();
        let fd = stream.as_raw_fd();
        
        let sent = send_entire_file(fd, &file).unwrap();
        assert_eq!(sent, content.len());
    }
}
