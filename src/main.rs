#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate log;

mod bar;
mod button_manager;
mod color;
mod config;
mod i3bar_protocol;
mod ord_adaptor;
mod pointer_btn;
mod river_protocols;
mod shared_state;
mod state;
mod status_cmd;
mod tags;
mod text;
mod utils;

use smithay_client_toolkit::reexports::client::Connection;
use tokio::io::{unix::AsyncFd, Interest};

use state::State;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let conn = Connection::connect_to_env()?;
    let mut event_queue = conn.new_event_queue();
    let mut state = State::new(&conn, &mut event_queue);

    let async_fd = AsyncFd::with_interest(
        event_queue.prepare_read()?.connection_fd(),
        Interest::READABLE,
    )?;

    event_queue.roundtrip(&mut state)?;
    event_queue.flush()?;

    loop {
        tokio::select! {
            readable = async_fd.readable() => {
                readable?.clear_ready();
                event_queue.prepare_read()?.read()?;
                event_queue.dispatch_pending(&mut state)?;
                event_queue.flush()?;
            }
            readable = state.wait_for_status_cmd() => {
                readable?.clear_ready();
                if let Err(e) = state.notify_available() {
                    if let Some(mut status_cmd) = state.shared_state.as_mut().unwrap().status_cmd.take() {
                        let _ = status_cmd.child.kill();
                    }
                    state.set_error(e.to_string());
                    state.draw_all();
                }
                event_queue.flush()?;
            }
        }
    }
}
