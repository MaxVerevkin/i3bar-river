use wayrs_client::global::{Global, GlobalExt};
use wayrs_client::protocol::*;
use wayrs_client::Connection;

use crate::state::State;

pub struct Output {
    pub wl: WlOutput,
    pub reg_name: u32,
    pub scale: u32,
}

impl Output {
    pub fn bind(conn: &mut Connection<State>, global: &Global) -> Self {
        Self {
            wl: global
                .bind_with_cb(conn, 4, wl_output_cb)
                .expect("could not bind wl_output"),
            reg_name: global.name,
            scale: 1,
        }
    }

    pub fn destroy(self, conn: &mut Connection<State>) {
        self.wl.release(conn);
    }
}

fn wl_output_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    output: WlOutput,
    event: wl_output::Event,
) {
    match event {
        wl_output::Event::Name(name) => {
            let i = state
                .pending_outputs
                .iter()
                .position(|o| o.wl == output)
                .unwrap();
            let output = state.pending_outputs.swap_remove(i);
            state.register_output(conn, output, name.to_str().expect("invalid output name"));
        }
        wl_output::Event::Scale(scale) => {
            if let Some(bar) = state.bars.iter_mut().find(|bar| bar.output.wl == output) {
                bar.scale = scale as u32;
            } else if let Some(output) = state.pending_outputs.iter_mut().find(|o| o.wl == output) {
                output.scale = scale as u32;
            }
        }
        _ => (),
    }
}
