#![allow(clippy::single_component_path_imports)]

#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate log;

mod bar;
mod button_manager;
mod color;
mod config;
mod cursor;
mod i3bar_protocol;
mod ord_adaptor;
mod pointer_btn;
mod protocol;
mod shared_state;
mod state;
mod status_cmd;
mod tags;
mod text;
mod utils;

use wayrs_client::connection::Connection;

use state::State;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut conn = Connection::connect()?;
    let globals = conn.async_collect_initial_globals().await?;
    let mut state = State::new(&mut conn, &globals);
    conn.async_flush().await?;

    loop {
        tokio::select! {
            recv_events = conn.async_recv_events() => {
                recv_events?;
                conn.dispatch_events(&mut state);
                conn.async_flush().await?;
            }
            reat_res = state.status_cmd_read() => {
                reat_res?;
                if let Err(e) = state.status_cmd_notify_available(&mut conn) {
                    if let Some(mut status_cmd) = state.shared_state.status_cmd.take() {
                        let _ = status_cmd.child.kill();
                    }
                    state.set_error(&mut conn, e.to_string());
                }
                conn.async_flush().await?;
            }
        }
    }
}
