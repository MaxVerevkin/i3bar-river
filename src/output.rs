use wayrs_client::global::{Global, GlobalExt};
use wayrs_client::Connection;
use wayrs_client::{protocol::*, EventCtx};

use crate::state::State;

#[derive(Debug)]
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

fn wl_output_cb(ctx: EventCtx<State, WlOutput>) {
    match ctx.event {
        wl_output::Event::Name(name) => {
            let i = ctx
                .state
                .pending_outputs
                .iter()
                .position(|o| o.wl == ctx.proxy)
                .unwrap();
            let output = ctx.state.pending_outputs.swap_remove(i);
            ctx.state.register_output(
                ctx.conn,
                output,
                name.to_str().expect("invalid output name"),
            );
        }
        wl_output::Event::Scale(scale) => {
            if let Some(bar) = ctx
                .state
                .bars
                .iter_mut()
                .find(|bar| bar.output.wl == ctx.proxy)
            {
                bar.output.scale = scale as u32;
            } else if let Some(output) = ctx
                .state
                .pending_outputs
                .iter_mut()
                .find(|o| o.wl == ctx.proxy)
            {
                output.scale = scale as u32;
            }
        }
        _ => (),
    }
}
