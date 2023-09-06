use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use serde::de::DeserializeOwned;
use wayrs_client::{Connection, IoMode};

use super::*;
use crate::event_loop::{self, EventLoop};
use crate::state::State;
use crate::utils::read_to_vec;

pub struct Hyprland {
    ipc: Ipc,
    workspaces: Vec<IpcWorkspace>,
    active_id: u32,
}

impl Hyprland {
    pub fn new(event_loop: &mut EventLoop<(Connection<State>, State)>) -> Option<Self> {
        let his = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let ipc = Ipc::new(&his).ok()?;
        let active_id = ipc.query_json::<IpcWorkspace>("j/activeworkspace").ok()?.id;
        let mut this = Self {
            ipc,
            workspaces: Vec::new(),
            active_id,
        };
        this.fetch_workspaces().ok()?;
        event_loop.register_with_fd(this.ipc.sock2.as_raw_fd(), |state| {
            match hyprland_cb(&mut state.0, &mut state.1) {
                Ok(()) => Ok(event_loop::Action::Keep),
                Err(e) => {
                    state.1.set_error(&mut state.0, e);
                    state.0.flush(IoMode::Blocking)?;
                    Ok(event_loop::Action::Unregister)
                }
            }
        });
        Some(this)
    }

    fn fetch_workspaces(&mut self) -> io::Result<()> {
        self.workspaces = self.ipc.query_json::<Vec<IpcWorkspace>>("j/workspaces")?;
        self.workspaces.sort_unstable_by_key(|x| x.id);
        Ok(())
    }
}

impl WmInfoProvider for Hyprland {
    fn new_ouput(&mut self, _: &mut Connection<State>, _: WlOutput) {}

    fn output_removed(&mut self, _: &mut Connection<State>, _: WlOutput) {}

    fn get_tags(&self, output: &Output) -> Vec<Tag> {
        self.workspaces
            .iter()
            .filter(|ws| ws.monitor == output.name)
            .map(|ws| Tag {
                id: ws.id,
                name: ws.name.clone(),
                is_focused: ws.id == self.active_id,
                is_active: true,
                is_urgent: false,
            })
            .collect()
    }

    fn get_layout_name(&self, _: &Output) -> Option<String> {
        None
    }

    fn get_mode_name(&self, _: &Output) -> Option<String> {
        None
    }

    fn click_on_tag(
        &mut self,
        _: &mut Connection<State>,
        _: WlOutput,
        _: WlSeat,
        tag_id: u32,
        btn: PointerBtn,
    ) {
        if btn == PointerBtn::Left {
            let _ = self.ipc.exec(&format!("/dispatch workspace {tag_id}"));
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

fn hyprland_cb(conn: &mut Connection<State>, state: &mut State) -> io::Result<()> {
    let hyprland = state.shared_state.get_hyprland().unwrap();
    let mut updated = false;
    loop {
        match hyprland.ipc.next_event() {
            Ok(event) => {
                if let Some(active_ws) = event.strip_prefix("workspace>>") {
                    hyprland.active_id = active_ws.parse().unwrap();
                    updated = true;
                } else if event.contains("workspace>>") {
                    hyprland.fetch_workspaces()?;
                    updated = true;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        }
    }
    if updated {
        state.tags_updated(conn, None);
    }
    Ok(())
}

struct Ipc {
    sock1_path: String,
    sock2: UnixStream,
    sock2_buf: Vec<u8>,
}

impl Ipc {
    fn new(his: &str) -> io::Result<Self> {
        let sock1_path = format!("/tmp/hypr/{his}/.socket.sock");
        let sock2_path = format!("/tmp/hypr/{his}/.socket2.sock");
        let sock2 = UnixStream::connect(sock2_path)?;
        sock2.set_nonblocking(true)?;
        Ok(Self {
            sock1_path,
            sock2,
            sock2_buf: Vec::new(),
        })
    }

    fn exec(&self, cmd: &str) -> io::Result<()> {
        let mut sock = UnixStream::connect(&self.sock1_path)?;
        sock.write_all(cmd.as_bytes())?;
        sock.flush()?;
        Ok(())
    }

    fn query_json<T: DeserializeOwned>(&self, cmd: &str) -> io::Result<T> {
        let mut sock = UnixStream::connect(&self.sock1_path)?;
        sock.write_all(cmd.as_bytes())?;
        sock.flush()?;
        serde_json::from_reader(&mut sock).map_err(Into::into)
    }

    fn next_event(&mut self) -> io::Result<String> {
        loop {
            if let Some(i) = memchr::memchr(b'\n', &self.sock2_buf) {
                let event = String::from_utf8_lossy(&self.sock2_buf[..i]).into_owned();
                self.sock2_buf.drain(..=i);
                return Ok(event);
            }
            if read_to_vec(&mut self.sock2, &mut self.sock2_buf)? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "hyprland socked disconnected",
                ));
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct IpcWorkspace {
    id: u32,
    name: String,
    monitor: String,
}
