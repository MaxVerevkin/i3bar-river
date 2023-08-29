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

use std::io::{self, ErrorKind};
use std::os::fd::{AsRawFd, RawFd};
use std::path::PathBuf;

use clap::Parser;
use nix::fcntl::OFlag;
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

    loop {
        let poll = Poll::new(conn.as_raw_fd(), sig_read, state.status_cmd_fd())?;

        if poll.wayland {
            match conn.recv_events(IoMode::NonBlocking) {
                Ok(()) => {
                    conn.dispatch_events(&mut state);
                    conn.flush(IoMode::Blocking)?;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => (),
                Err(e) => bail!(e),
            }
        }

        if poll.signal {
            nix::unistd::read(sig_read.as_raw_fd(), &mut [0; 1])?;
            state.toggle_visibility(&mut conn);
            conn.flush(IoMode::Blocking)?;
        }

        if let Some(status_cmd) = &mut state.shared_state.status_cmd {
            if poll.cmd {
                match status_cmd.receive_blocks() {
                    Ok(None) => (),
                    Ok(Some(blocks)) => {
                        state.set_blocks(&mut conn, blocks);
                        conn.flush(IoMode::Blocking)?;
                    }
                    Err(e) => {
                        let _ = state.shared_state.status_cmd.take().unwrap().child.kill();
                        state.set_error(&mut conn, e);
                        conn.flush(IoMode::Blocking)?;
                    }
                }
            }
        }
    }
}

struct Poll {
    wayland: bool,
    signal: bool,
    cmd: bool,
}

impl Poll {
    fn new(wayland: RawFd, signal: RawFd, cmd: Option<RawFd>) -> io::Result<Self> {
        let mut fds = [libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        }; 3];

        fds[0].fd = wayland.as_raw_fd();
        fds[1].fd = signal.as_raw_fd();
        fds[2].fd = cmd.map_or(0, |cmd| cmd.as_raw_fd());

        loop {
            // nix' (0.27) poll() implementation is hard to work with because of the lifetimes. In
            // particular, it makes reusing the same buffer across poll()s very hard. It is better
            // to just call directly into the libc at this point, it's not that hard. At least it is
            // safer that trying to trick nix by using `BorrowedFd::borrow_raw()`.
            let result =
                unsafe { libc::poll(fds.as_mut_ptr(), if cmd.is_some() { 3 } else { 2 }, -1) };

            if result == -1 {
                let err = io::Error::last_os_error();
                if err.kind() != ErrorKind::Interrupted {
                    return Err(io::Error::last_os_error());
                }
            } else {
                return Ok(Self {
                    wayland: fds[0].revents & libc::POLLIN != 0,
                    signal: fds[1].revents & libc::POLLIN != 0,
                    cmd: fds[2].revents & libc::POLLIN != 0,
                });
            }
        }
    }
}
