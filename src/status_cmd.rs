use std::io::Write;
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::Result;

use crate::i3bar_protocol::{Block, Event, Protocol};

const INITIAL_BUF_CAPACITY: usize = 4096;

#[derive(Debug)]
pub struct StatusCmd {
    pub child: Child,
    pub output: ChildStdout,
    input: ChildStdin,
    protocol: Protocol,
    buf: Vec<u8>,
    buf_used: usize,
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
            buf: vec![0; INITIAL_BUF_CAPACITY],
            buf_used: 0,
        })
    }

    pub fn read(&mut self) -> Result<Option<Vec<Block>>> {
        assert!(self.buf.len() >= INITIAL_BUF_CAPACITY);
        if self.buf.len() == self.buf_used {
            // Double the capacity
            self.buf.resize(self.buf_used * 2, 0);
        }

        match nix::unistd::read(self.output.as_raw_fd(), &mut self.buf[self.buf_used..]) {
            Ok(0) => bail!("status command exited"),
            Ok(n) => self.buf_used += n,
            Err(nix::errno::Errno::EAGAIN) => return Ok(None),
            Err(e) => bail!(e),
        }

        let rem = self
            .protocol
            .process_new_bytes(&self.buf[..self.buf_used])?;

        if rem.is_empty() {
            self.buf_used = 0;
        } else {
            let rem_len = rem.len();
            self.buf
                .copy_within((self.buf_used - rem_len)..self.buf_used, 0);
            self.buf_used = rem_len;
        }

        Ok(self.protocol.get_blocks())
    }

    pub fn send_click_event(&mut self, event: &Event) -> Result<()> {
        if self.protocol.supports_clicks() {
            writeln!(self.input, "{}", serde_json::to_string(event).unwrap())?;
        }
        Ok(())
    }
}
