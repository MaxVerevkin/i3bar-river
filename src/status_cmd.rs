use std::io::{self, BufWriter, ErrorKind, Write};
use std::os::unix::io::AsRawFd;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::Result;

use crate::i3bar_protocol::{Block, Event, Protocol};
use crate::utils::read_to_vec;

#[derive(Debug)]
pub struct StatusCmd {
    pub child: Child,
    pub output: ChildStdout,
    input: BufWriter<ChildStdin>,
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
        let input = BufWriter::new(child.stdin.take().unwrap());
        if unsafe { libc::fcntl(output.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } == -1 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(Self {
            child,
            output,
            input,
            protocol: Protocol::Unknown,
            buf: Vec::new(),
        })
    }

    pub fn receive_blocks(&mut self) -> Result<Option<Vec<Block>>> {
        match read_to_vec(&mut self.output, &mut self.buf) {
            Ok(0) => bail!("status command exited"),
            Ok(_n) => (),
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(e) => bail!(e),
        }

        let rem = self.protocol.process_new_bytes(&self.buf)?;
        let used = self.buf.len() - rem.len();
        self.buf.drain(..used);

        Ok(self.protocol.get_blocks())
    }

    pub fn send_click_event(&mut self, event: &Event) -> Result<()> {
        if self.protocol.supports_clicks() {
            serde_json::to_writer(&mut self.input, event)?;
            self.input.write_all(b"\n")?;
            self.input.flush()?;
        }
        Ok(())
    }
}
