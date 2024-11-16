#![allow(clippy::collapsible_else_if)]

use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use serde::de::IgnoredAny;

use super::*;
use crate::event_loop;
use crate::utils::read_to_vec;

pub struct NiriInfoProvider {
    ipc: Ipc,
    workspaces: Vec<IpcWorkspace>,
}

impl NiriInfoProvider {
    pub fn new() -> Option<Self> {
        let ns = std::env::var("NIRI_SOCKET").ok()?;
        let ipc = Ipc::new(&ns)?;
        Some(Self {
            workspaces: Vec::new(),
            ipc,
        })
    }

    fn set_workspace(&self, idx: u32) {
        let _ = self.ipc.exec(
            &format!(r#"{{"Action":{{"FocusWorkspace":{{"reference":{{"Index":{idx}}}}}}}}}"#)
        );
    }
}

impl WmInfoProvider for NiriInfoProvider {
    fn register(&self, event_loop: &mut EventLoop) {
        event_loop.register_with_fd(self.ipc.sock.as_raw_fd(), |ctx| {
            match niri_cb(ctx.conn, ctx.state) {
                Ok(()) => Ok(event_loop::Action::Keep),
                Err(e) => {
                    ctx.state.set_error(ctx.conn, "niri", e);
                    Ok(event_loop::Action::Unregister)
                }
            }
        });
    }

    fn get_tags(&self, output: &Output) -> Vec<Tag> {
        // Niri always generates an empty workspace rather than having an explicit workspace
        // creation command, so we make the last workspace active only if the user is looking at
        // it. This makes the behavior of `hide_inactive_tags` useful for Niri.
        self.workspaces
            .iter()
            .enumerate()
            .filter(|(_, ws)| ws.output == output.name)
            .map(|(i, ws)| Tag {
                id: ws.idx,
                name: ws.name.clone().map_or_else(
                    || ws.idx.to_string(),
                    |name| format!("{0} / {1}", ws.idx, name)),
                is_focused: ws.is_active,
                is_active: i < self.workspaces.len() - 1 || ws.is_focused,
                is_urgent: false,
            })
            .collect()
    }

    fn click_on_tag(
        &mut self,
        _: &mut Connection<State>,
        output: &Output,
        _: WlSeat,
        tag_id: Option<u32>,
        btn: PointerBtn,
    ) {
        match btn {
            PointerBtn::Left => {
                if let Some(tag_id) = tag_id {
                    self.set_workspace(tag_id);
                }
            }
            PointerBtn::WheelUp | PointerBtn::WheelDown => {
                if let Some(active_i) = self
                    .workspaces
                    .iter()
                    .position(|ws| ws.output == output.name && ws.is_focused)
                {
                    if btn == PointerBtn::WheelUp {
                        if let Some(prev) = self.workspaces[..active_i]
                            .iter()
                            .rfind(|ws| ws.output == output.name)
                        {
                            self.set_workspace(prev.idx);
                        }
                    } else {
                        if let Some(next) = self.workspaces[active_i..]
                            .iter()
                            .skip(1)
                            .find(|ws| ws.output == output.name)
                        {
                            self.set_workspace(next.idx);
                        }
                    }
                }
            }
            _ => (),
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

fn niri_cb(conn: &mut Connection<State>, state: &mut State) -> io::Result<()> {
    let niri = state.shared_state.get_niri().unwrap();
    let mut updated = false;
    loop {
        match niri.ipc.next_event() {
            Ok(IpcEvent::WorkspacesChanged{ workspaces }) => {
                niri.workspaces = workspaces;
                niri.workspaces.sort_by_key(|w| w.idx);
                updated = true;
            }
            Ok(IpcEvent::WorkspaceActivated { id }) => {
                if let Some(new_active) = niri.workspaces.iter().position(|ws| ws.id == id) {
                    // Clear the previous active workspace and apply it to the new one.
                    if let Some(previous_active) = niri
                        .workspaces
                        .iter()
                        .position(|ws| ws.is_active && ws.output == niri.workspaces[new_active].output)
                    {
                        niri.workspaces[previous_active].is_active = false;
                        niri.workspaces[new_active].is_active = true;
                        updated = true;
                    }
                }
            }
            Ok(IpcEvent::Ok(_)) => continue,
            Ok(IpcEvent::Ignored(_)) => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        }
    }
    if updated {
        state.tags_updated(conn, None);
    }
    Ok(())
}

#[derive(Debug)]
struct Ipc {
    sock_path: PathBuf,
    sock: UnixStream,
    sock_buf: Vec<u8>,
}

impl Ipc {
    fn new(ns: &str) -> Option<Self> {
        let sock_path = PathBuf::from(ns);
        let mut sock = UnixStream::connect(sock_path.clone()).ok()?;
        sock.set_nonblocking(true).ok()?;
        sock.write_all("\"EventStream\"\n".as_bytes()).ok()?;
        Some(Self {
            sock_path,
            sock,
            sock_buf: Vec::new(),
        })
    }

    fn exec(&self, cmd: &str) -> io::Result<()> {
        let mut sock = UnixStream::connect(&self.sock_path)?;
        sock.write_all(cmd.as_bytes())?;
        sock.flush()?;
        Ok(())
    }

    fn next_event(&mut self) -> io::Result<IpcEvent> {
        loop {
            if let Some(i) = memchr::memchr(b'\n', &self.sock_buf) {
                let event = String::from_utf8_lossy(&self.sock_buf[..i]).into_owned();
                self.sock_buf.drain(..=i);
                return Ok(serde_json::from_str(&event)?);
            }
            if read_to_vec(&self.sock, &mut self.sock_buf)? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "niri socked disconnected",
                ));
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct IpcWorkspace {
    id: u32,  // Niri's internal id is monotonic, only used for comparison.
    idx: u32,  // idx is the user-facing workspace number.
    name: Option<String>,
    output: String,
    is_focused: bool,
    is_active: bool  // Niri's is_active means the workspace is visible on a display.
    // active_window_id is unneeded.
}

#[derive(Debug, serde::Deserialize)]
enum IpcEvent {
    Ok(IgnoredAny),
    WorkspacesChanged {
        workspaces: Vec<IpcWorkspace>,
    },
    WorkspaceActivated {
        id: u32,
        // focused doesn't matter for our purpose.
    },
    #[serde(untagged)]
    Ignored(IgnoredAny),
}
