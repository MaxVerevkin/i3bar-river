use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use serde::de::DeserializeOwned;

use super::*;
use crate::event_loop;
use crate::utils::read_to_vec;

pub struct HyprlandInfoProvider {
    ipc: Ipc,
    workspaces: Vec<IpcWorkspace>,
    active_id: u32,
}

impl HyprlandInfoProvider {
    pub fn new() -> Option<Self> {
        let his = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let ipc = Ipc::new(&his)?;
        Some(Self {
            workspaces: ipc.query_sorted_workspaces().ok()?,
            active_id: ipc.query_json::<IpcWorkspace>("j/activeworkspace").ok()?.id,
            ipc,
        })
    }
}

impl WmInfoProvider for HyprlandInfoProvider {
    fn register(&self, event_loop: &mut EventLoop) {
        event_loop.register_with_fd(self.ipc.sock2.as_raw_fd(), |ctx| {
            match hyprland_cb(ctx.conn, ctx.state) {
                Ok(()) => Ok(event_loop::Action::Keep),
                Err(e) => {
                    ctx.state.set_error(ctx.conn, e);
                    Ok(event_loop::Action::Unregister)
                }
            }
        });
    }

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
                    hyprland.active_id = active_ws
                        .parse()
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    updated = true;
                } else if let Some(data) = event.strip_prefix("focusedmon>>") {
                    let (_monitor, active_ws) = data.split_once(',').ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "Too few fields in data")
                    })?;

                    hyprland.active_id = active_ws
                        .parse()
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    updated = true;
                } else if event.contains("workspace>>") {
                    hyprland.workspaces = hyprland.ipc.query_sorted_workspaces()?;
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
    sock1_path: PathBuf,
    sock2: UnixStream,
    sock2_buf: Vec<u8>,
}

impl Ipc {
    fn new(his: &str) -> Option<Self> {
        let mut path = PathBuf::from(std::env::var("XDG_RUNTIME_DIR").ok()?);
        path.push("hypr");
        if !path.exists() {
            path.push("/tmp/hypr");
        }
        path.push(his);
        let sock1_path = path.join(".socket.sock");
        let sock2_path = path.join(".socket2.sock");
        let sock2 = UnixStream::connect(sock2_path).ok()?;
        sock2.set_nonblocking(true).ok()?;
        Some(Self {
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

    fn query_sorted_workspaces(&self) -> io::Result<Vec<IpcWorkspace>> {
        let mut workspaces = self.query_json::<Vec<IpcWorkspace>>("j/workspaces")?;
        workspaces.sort_unstable_by_key(|x| x.id);
        Ok(workspaces)
    }

    fn next_event(&mut self) -> io::Result<String> {
        loop {
            if let Some(i) = memchr::memchr(b'\n', &self.sock2_buf) {
                let event = String::from_utf8_lossy(&self.sock2_buf[..i]).into_owned();
                self.sock2_buf.drain(..=i);
                return Ok(event);
            }
            if read_to_vec(&self.sock2, &mut self.sock2_buf)? == 0 {
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
