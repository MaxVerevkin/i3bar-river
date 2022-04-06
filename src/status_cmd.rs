use std::cell::RefCell;
use std::io::{BufRead, BufReader, Result, Write};
use std::os::unix::prelude::AsRawFd;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::rc::Rc;

use smithay_client_toolkit::reexports::calloop::{self, LoopHandle};

use crate::i3bar_protocol::{Block, Event, Protocol};
use crate::BarState;

pub struct StatusCmd {
    child: Child,
    output: BufReader<ChildStdout>,
    input: ChildStdin,
    protocol: Protocol,
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
            output: BufReader::new(output),
            input,
            protocol: Protocol::Unknown,
        })
    }

    pub fn notify_available(&mut self) -> Result<Option<Vec<Block>>> {
        let buf = self.output.fill_buf()?;
        self.protocol.process_new_bytes(buf)?;
        let consumed = buf.len();
        self.output.consume(consumed);
        Ok(self.protocol.get_blocks())
    }

    pub fn send_click_event(&mut self, event: &Event) -> Result<()> {
        if self.protocol.supports_clicks() {
            writeln!(self.input, "{}", serde_json::to_string(event).unwrap(),)?;
        }
        Ok(())
    }

    pub fn quick_insert(&self, handle: LoopHandle<()>, bar_state: Rc<RefCell<BarState>>) {
        handle
            .insert_source(
                calloop::generic::Generic::new(
                    self.output.get_ref().as_raw_fd(),
                    calloop::Interest {
                        readable: true,
                        writable: false,
                    },
                    calloop::Mode::Level,
                ),
                move |ready, _, _| {
                    if ready.readable {
                        bar_state.borrow_mut().notify_available()?;
                        Ok(calloop::PostAction::Continue)
                    } else {
                        let mut bar_state = bar_state.borrow_mut();
                        bar_state.set_error("error reading from status command");
                        if let Some(mut child) = bar_state.status_cmd.take() {
                            let _ = child.child.kill();
                        }
                        Ok(calloop::PostAction::Remove)
                    }
                },
            )
            .expect("failed to inser calloop source");
    }
}
