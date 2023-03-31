use std::io::Write;
use std::os::fd::{AsRawFd, RawFd};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::Result;

use nix::errno::Errno;
use nix::libc;

use crate::i3bar_protocol::{Block, Event, Protocol};

#[derive(Debug)]
pub struct StatusCmd {
    pub child: Child,
    pub output: ChildStdout,
    input: ChildStdin,
    protocol: Protocol,
    buf: Vec<u8>,
}

impl StatusCmd {
    pub fn new(cmd: &str) -> Result<Self> {
        let mut child = Command::new("sh")
            .args(["-c", &format!("exec {cmd}")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let output = child.stdout.take().unwrap();
        let input = child.stdin.take().unwrap();
        nix::fcntl::fcntl(
            output.as_raw_fd(),
            nix::fcntl::F_SETFL(nix::fcntl::OFlag::O_NONBLOCK),
        )?;
        Ok(Self {
            child,
            output,
            input,
            protocol: Protocol::Unknown,
            buf: Vec::new(),
        })
    }

    pub fn read(&mut self) -> Result<Option<Vec<Block>>> {
        match read(self.output.as_raw_fd(), &mut self.buf) {
            Ok(0) => bail!("status command exited"),
            Ok(_n) => (),
            Err(Errno::EAGAIN) => return Ok(None),
            Err(e) => bail!(e),
        }

        let rem = self.protocol.process_new_bytes(&self.buf)?;
        let used = self.buf.len() - rem.len();
        self.buf.drain(..used);

        Ok(self.protocol.get_blocks())
    }

    pub fn send_click_event(&mut self, event: &Event) -> Result<()> {
        if self.protocol.supports_clicks() {
            writeln!(self.input, "{}", serde_json::to_string(event).unwrap())?;
        }
        Ok(())
    }
}

/// Read from a raw file descriptor to the vector.
///
/// Appends data at the end of the buffer. Resizes vector as needed.
pub fn read(fd: RawFd, buf: &mut Vec<u8>) -> nix::Result<usize> {
    if buf.capacity() - buf.len() < 1024 {
        buf.reserve(buf.capacity().max(1024));
    }

    let res = unsafe {
        libc::read(
            fd,
            buf.as_mut_ptr().add(buf.len()) as *mut libc::c_void,
            (buf.capacity() - buf.len()) as libc::size_t,
        )
    };

    let read = Errno::result(res).map(|r| r as usize)?;
    if read > 0 {
        unsafe {
            buf.set_len(buf.len() + read);
        }
    }

    Ok(read)
}
