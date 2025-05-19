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

#[cfg(feature = "river")]
mod river;
#[cfg(feature = "river")]
pub use river::*;

#[cfg(feature = "hyprland")]
mod hyprland;
#[cfg(feature = "hyprland")]
pub use hyprland::*;

#[cfg(feature = "niri")]
mod niri;
#[cfg(feature = "niri")]
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
    #[cfg(feature = "river")]
    if let Some(river) = RiverInfoProvider::bind(conn, config) {
        return Box::new(river);
    }

    #[cfg(feature = "hyprland")]
    if let Some(hyprland) = HyprlandInfoProvider::new() {
        return Box::new(hyprland);
    }

    #[cfg(feature = "niri")]
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
