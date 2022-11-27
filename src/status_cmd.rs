use std::io::{BufRead, BufReader, Write};
use std::os::unix::prelude::AsRawFd;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::Result;

use tokio::io::unix::AsyncFd;
use tokio::io::Interest;

use crate::i3bar_protocol::{Block, Event, Protocol};

#[derive(Debug)]
pub struct StatusCmd {
    pub child: Child,
    output: BufReader<ChildStdout>,
    input: ChildStdin,
    protocol: Protocol,
    buf: Vec<u8>,
    pub async_fd: AsyncFd<i32>,
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
        let async_fd = AsyncFd::with_interest(output.as_raw_fd(), Interest::READABLE)?;
        Ok(Self {
            child,
            output: BufReader::new(output),
            input,
            protocol: Protocol::Unknown,
            buf: Vec::new(),
            async_fd,
        })
    }

    pub fn notify_available(&mut self) -> Result<Option<Vec<Block>>> {
        let buf = self.output.fill_buf()?;
        if buf.is_empty() {
            bail!("status command exited");
        }
        if self.buf.is_empty() {
            let rem = self.protocol.process_new_bytes(buf)?;
            if !rem.is_empty() {
                self.buf.extend_from_slice(rem);
            }
        } else {
            self.buf.extend_from_slice(buf);
            let rem = self.protocol.process_new_bytes(&self.buf)?;
            if rem.is_empty() {
                self.buf.clear();
            } else {
                let used = self.buf.len() - rem.len();
                self.buf.drain(..used);
            }
        }
        let consumed = buf.len();
        self.output.consume(consumed);
        Ok(self.protocol.get_blocks())
    }

    pub fn send_click_event(&mut self, event: &Event) -> Result<()> {
        if self.protocol.supports_clicks() {
            writeln!(self.input, "{}", serde_json::to_string(event).unwrap())?;
        }
        Ok(())
    }

    // pub fn quick_insert(&self, handle: LoopHandle<BarState>) {
    //     handle
    //         .insert_source(
    //             calloop::generic::Generic::new(
    //                 self.output.get_ref().as_raw_fd(),
    //                 calloop::Interest {
    //                     readable: true,
    //                     writable: false,
    //                 },
    //                 calloop::Mode::Level,
    //             ),
    //             move |ready, _, bar_state| {
    //                 if ready.readable {
    //                     bar_state.notify_available()?;
    //                     Ok(calloop::PostAction::Continue)
    //                 } else {
    //                     bar_state.set_error("error reading from status command");
    //                     if let Some(mut child) = bar_state.status_cmd.take() {
    //                         let _ = child.child.kill();
    //                     }
    //                     Ok(calloop::PostAction::Remove)
    //                 }
    //             },
    //         )
    //         .expect("failed to inser calloop source");
    // }
}
