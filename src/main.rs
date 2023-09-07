#[macro_use]
extern crate anyhow;

mod bar;
mod blocks_cache;
mod button_manager;
mod color;
mod config;
mod event_loop;
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
use std::os::fd::AsRawFd;
use std::path::PathBuf;

use clap::Parser;
use event_loop::EventLoop;
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
    let mut el = EventLoop::new();
    let mut state = State::new(&mut conn, &globals, &mut el, args.config.as_deref());
    conn.flush(IoMode::Blocking)?;

    el.register_with_fd(sig_read, move |ctx| {
        nix::unistd::read(sig_read, &mut [0; 1])?;
        ctx.state.toggle_visibility(ctx.conn);
        Ok(event_loop::Action::Keep)
    });

    el.register_with_fd(conn.as_raw_fd(), |ctx| {
        match ctx.conn.recv_events(IoMode::NonBlocking) {
            Ok(()) => {
                ctx.conn.dispatch_events(ctx.state);
                ctx.conn.flush(IoMode::Blocking)?;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => (),
            Err(e) => bail!(e),
        }
        Ok(event_loop::Action::Keep)
    });

    if let Some(fd) = state.status_cmd_fd() {
        el.register_with_fd(fd, |ctx| {
            match ctx
                .state
                .shared_state
                .status_cmd
                .as_mut()
                .unwrap()
                .receive_blocks()
            {
                Ok(None) => Ok(event_loop::Action::Keep),
                Ok(Some(blocks)) => {
                    ctx.state.set_blocks(ctx.conn, blocks);
                    ctx.conn.flush(IoMode::Blocking)?;
                    Ok(event_loop::Action::Keep)
                }
                Err(e) => {
                    let _ = ctx
                        .state
                        .shared_state
                        .status_cmd
                        .take()
                        .unwrap()
                        .child
                        .kill();
                    ctx.state.set_error(ctx.conn, e);
                    ctx.conn.flush(IoMode::Blocking)?;
                    Ok(event_loop::Action::Unregister)
                }
            }
        });
    }

    loop {
        el.run(&mut conn, &mut state)?;
    }
}
