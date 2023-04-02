#![allow(clippy::single_component_path_imports)]

#[macro_use]
extern crate anyhow;

mod bar;
mod button_manager;
mod color;
mod config;
mod i3bar_protocol;
mod pointer_btn;
mod protocol;
mod shared_state;
mod state;
mod status_cmd;
mod text;
mod utils;
mod wm_info_provider;

use signal_hook::consts::*;

use std::{io::ErrorKind, os::fd::AsRawFd};

use wayrs_client::connection::Connection;
use wayrs_client::IoMode;

use nix::{
    errno::Errno,
    fcntl::OFlag,
    poll::{poll, PollFd, PollFlags},
};

use state::State;

fn main() -> anyhow::Result<()> {
    let (sig_read, sig_write) = nix::unistd::pipe2(OFlag::O_NONBLOCK | OFlag::O_CLOEXEC)?;
    signal_hook::low_level::pipe::register(SIGUSR1, sig_write)?;

    let mut conn = Connection::connect()?;
    let globals = conn.blocking_collect_initial_globals()?;
    let mut state = State::new(&mut conn, &globals);
    conn.flush(IoMode::Blocking)?;

    let mut fds = Vec::with_capacity(3);
    fds.push(PollFd::new(conn.as_raw_fd(), PollFlags::POLLIN));
    fds.push(PollFd::new(sig_read, PollFlags::POLLIN));
    if let Some(cmd_fd) = state.status_cmd_fd() {
        fds.push(PollFd::new(cmd_fd, PollFlags::POLLIN));
    }

    loop {
        match poll(&mut fds, -1) {
            Ok(_) => (),
            Err(Errno::EINTR) => continue,
            Err(e) => bail!(e),
        }

        if fds[0].any().unwrap_or(false) {
            match conn.recv_events(IoMode::NonBlocking) {
                Ok(()) => {
                    conn.dispatch_events(&mut state);
                    conn.flush(IoMode::Blocking)?;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => (),
                Err(e) => bail!(e),
            }
        }

        if fds[1].any().unwrap_or(false) {
            nix::unistd::read(sig_read, &mut [0; 1])?;
            state.toggle_visibility(&mut conn);
        }

        if fds.len() > 2 && fds[2].any().unwrap_or(false) {
            match state
                .shared_state
                .status_cmd
                .as_mut()
                .unwrap()
                .receive_blocks()
            {
                Ok(None) => (),
                Ok(Some(blocks)) => {
                    state.set_blocks(&mut conn, blocks);
                    conn.flush(IoMode::Blocking)?;
                }
                Err(e) => {
                    let _ = state.shared_state.status_cmd.take().unwrap().child.kill();
                    fds.pop().unwrap();
                    state.set_error(&mut conn, e);
                    conn.flush(IoMode::Blocking)?;
                }
            }
        }
    }
}
