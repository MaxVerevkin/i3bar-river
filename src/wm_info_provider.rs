use std::any::Any;

use wayrs_client::global::*;
use wayrs_client::Connection;

use crate::config::WmConfig;
use crate::event_loop::EventLoop;
use crate::output::Output;
use crate::pointer_btn::PointerBtn;
use crate::protocol::*;
use crate::state::State;

mod river;
pub use river::*;

mod hyprland;
pub use hyprland::*;

pub trait WmInfoProvider {
    fn new_ouput(&mut self, conn: &mut Connection<State>, output: WlOutput);
    fn output_removed(&mut self, conn: &mut Connection<State>, output: WlOutput);

    fn get_tags(&self, output: &Output) -> Vec<Tag>;
    fn get_layout_name(&self, output: &Output) -> Option<String>;
    fn get_mode_name(&self, output: &Output) -> Option<String>;

    fn click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        output: WlOutput,
        seat: WlSeat,
        tag_id: u32,
        btn: PointerBtn,
    );

    fn as_any(&mut self) -> &mut dyn Any;
}

pub fn bind(
    conn: &mut Connection<State>,
    globals: &Globals,
    event_loop: &mut EventLoop<(Connection<State>, State)>,
    config: &WmConfig,
) -> Option<Box<dyn WmInfoProvider>> {
    if let Some(river) = RiverInfoProvider::bind(conn, globals, config) {
        return Some(Box::new(river));
    }

    if let Some(hyprland) = Hyprland::new(event_loop) {
        return Some(Box::new(hyprland));
    }

    None
}

#[derive(Debug)]
pub struct Tag {
    pub id: u32,
    pub name: String,
    pub is_focused: bool,
    pub is_active: bool,
    pub is_urgent: bool,
}
