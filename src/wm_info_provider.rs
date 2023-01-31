mod river;

use std::any::Any;

use wayrs_client::connection::Connection;
use wayrs_client::global::*;

use crate::protocol::*;
use crate::state::State;

pub type WmInfoCallback = fn(&mut Connection<State>, &mut State, WlOutput, WmInfo);

pub trait WmInfoProvider {
    fn as_any(&mut self) -> &mut dyn Any;

    fn new_outut(&mut self, conn: &mut Connection<State>, output: WlOutput);

    fn output_removed(&mut self, conn: &mut Connection<State>, output: WlOutput);

    fn left_click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        output: WlOutput,
        seat: WlSeat,
        tag: &str,
    );

    fn right_click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        output: WlOutput,
        seat: WlSeat,
        tag: &str,
    );
}

#[derive(Default)]
pub struct WmInfo {
    pub layout_name: Option<String>,
    pub tags: Vec<Tag>,
}

pub struct Tag {
    pub name: String,
    pub is_focused: bool,
    pub is_active: bool,
    pub is_urgent: bool,
}

pub fn bind_wayland(
    conn: &mut Connection<State>,
    globals: &Globals,
    callback: WmInfoCallback,
) -> Option<Box<dyn WmInfoProvider>> {
    // TODO: add more providers
    let river = river::RiverInfoProvider::bind(conn, globals, callback)?;
    Some(Box::new(river))
}
