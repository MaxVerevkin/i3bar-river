use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

use anyhow::Result;

use tokio::io::AsyncReadExt;
use tokio::process::ChildStdout;

use crate::i3bar_protocol::{Block, Event, Protocol};

const INITIAL_BUF_CAPACITY: usize = 4096;

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
        Ok(Self {
            child,
            output: ChildStdout::from_std(output)?,
            input,
            protocol: Protocol::Unknown,
            buf: Vec::with_capacity(INITIAL_BUF_CAPACITY),
        })
    }

    pub fn notify_available(&mut self) -> Result<Option<Vec<Block>>> {
        let rem = self.protocol.process_new_bytes(&self.buf)?;
        if rem.is_empty() {
            self.buf.clear();
        } else {
            let used = self.buf.len() - rem.len();
            self.buf.drain(..used);
        }
        Ok(self.protocol.get_blocks())
    }

    pub async fn read(&mut self) -> Result<()> {
        assert!(self.buf.capacity() >= INITIAL_BUF_CAPACITY);
        if self.buf.capacity() == self.buf.len() {
            // Double the capacity
            self.buf.reserve(self.buf.len());
        }

        let read_len = self.output.read_buf(&mut self.buf).await?;
        if read_len == 0 {
            bail!("status command exited");
        }

        Ok(())
    }

    pub fn send_click_event(&mut self, event: &Event) -> Result<()> {
        if self.protocol.supports_clicks() {
            writeln!(self.input, "{}", serde_json::to_string(event).unwrap())?;
        }
        Ok(())
    }
}
