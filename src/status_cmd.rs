use crate::Surface;
use std::cell::RefCell;
use std::io::Result;
use std::io::Write;
use std::os::unix::prelude::{AsRawFd, RawFd};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::rc::Rc;

use crate::i3bar_protocol::{Block, Event, Protocol};
use crate::lines_buffer::LinesBuffer;
use crate::pointer_btn::PointerBtn;

#[derive(Clone)]
pub struct StatusCmd {
    inner: Rc<RefCell<Inner>>,
}

struct Inner {
    #[allow(dead_code)]
    child: Child,
    output: LinesBuffer<ChildStdout>,
    input: ChildStdin,
    protocol: Protocol,
    blocks: Rc<RefCell<Vec<Block>>>,
    surfaces: Rc<RefCell<Vec<Surface>>>,
}

impl AsRawFd for StatusCmd {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.borrow().output.inner().as_raw_fd()
    }
}

impl Inner {
    pub fn notify_available(&mut self) -> Result<()> {
        self.output.fill_buf()?;
        for line in &mut self.output {
            self.protocol.process_line(line)?;
        }
        if let Some(new_blocks) = self.protocol.get_blocks() {
            *self.blocks.borrow_mut() = new_blocks;
            for s in &mut *self.surfaces.borrow_mut() {
                s.blocks_need_update = true;
            }
        }
        Ok(())
    }
}

impl StatusCmd {
    pub fn new(
        cmd: &str,
        blocks: Rc<RefCell<Vec<Block>>>,
        surfaces: Rc<RefCell<Vec<Surface>>>,
    ) -> Result<Self> {
        let mut child = Command::new("sh")
            .args(["-c", &format!("exec {cmd}")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let output = LinesBuffer::new(child.stdout.take().unwrap());
        let input = child.stdin.take().unwrap();
        Ok(Self {
            inner: Rc::new(RefCell::new(Inner {
                child,
                output,
                input,
                protocol: Protocol::Unknown,
                blocks,
                surfaces,
            })),
        })
    }

    pub fn notify_available(&mut self) -> Result<()> {
        self.inner.borrow_mut().notify_available()
    }

    pub fn send_click_event(
        &mut self,
        button: PointerBtn,
        name: Option<&str>,
        instance: Option<&str>,
    ) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        if inner.protocol.supports_clicks() {
            writeln!(
                inner.input,
                "{}",
                serde_json::to_string(&Event {
                    name,
                    instance,
                    button,
                    ..Default::default()
                })
                .unwrap()
            )?;
        }
        Ok(())
    }
}
