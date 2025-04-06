use std::any::Any;

use wayrs_client::Connection;

use crate::config::WmConfig;
use crate::event_loop::EventLoop;
use crate::output::Output;
use crate::pointer_btn::PointerBtn;
use crate::protocol::*;
use crate::state::State;

mod dummy;
pub use dummy::*;

mod river;
pub use river::*;

mod hyprland;
pub use hyprland::*;

mod niri;
pub use niri::*;

pub trait WmInfoProvider: Any {
    fn register(&self, _: &mut EventLoop) {}

    fn new_ouput(&mut self, _: &mut Connection<State>, _: &Output) {}
    fn output_removed(&mut self, _: &mut Connection<State>, _: &Output) {}

    fn get_tags(&self, _: &Output) -> Vec<Tag> {
        Vec::new()
    }
    fn get_layout_name(&self, _: &Output) -> Option<String> {
        None
    }
    fn get_mode_name(&self, _: &Output) -> Option<String> {
        None
    }

    fn click_on_tag(
        &mut self,
        _conn: &mut Connection<State>,
        _output: &Output,
        _seat: WlSeat,
        _tag_id: Option<u32>,
        _btn: PointerBtn,
    ) {
    }
}

pub fn bind(conn: &mut Connection<State>, config: &WmConfig) -> Box<dyn WmInfoProvider> {
    if let Some(river) = RiverInfoProvider::bind(conn, config) {
        return Box::new(river);
    }

    if let Some(hyprland) = HyprlandInfoProvider::new() {
        return Box::new(hyprland);
    }

    if let Some(niri) = NiriInfoProvider::new() {
        return Box::new(niri);
    }

    Box::new(DummyInfoProvider)
}

#[derive(Debug)]
pub struct Tag {
    pub id: u32,
    pub name: String,
    pub is_focused: bool,
    pub is_active: bool,
    pub is_urgent: bool,
}
