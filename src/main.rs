#[macro_use]
extern crate anyhow;

mod bar;
mod blocks_cache;
mod button_manager;
mod color;
mod config;
mod i3bar_protocol;
mod output;
mod pointer_btn;
mod protocol;
mod shared_state;
mod state;
mod status_cmd;
mod text;
mod utils;
mod wm_info_provider;

use std::io::ErrorKind;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use clap::Parser;
use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::poll::{poll, PollFd, PollFlags};
use signal_hook::consts::*;
use wayrs_client::{Connection, IoMode};

use state::State;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The path to a config file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    let (sig_read, sig_write) = nix::unistd::pipe2(OFlag::O_NONBLOCK | OFlag::O_CLOEXEC)?;
    signal_hook::low_level::pipe::register(SIGUSR1, sig_write)?;

    let (mut conn, globals) = Connection::connect_and_collect_globals()?;
    let mut state = State::new(&mut conn, &globals, args.config.as_deref());
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
            conn.flush(IoMode::Blocking)?;
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
