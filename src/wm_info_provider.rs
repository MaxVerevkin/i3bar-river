use std::any::Any;

use wayrs_client::global::*;
use wayrs_client::Connection;

use crate::config::WmConfig;
use crate::pointer_btn::PointerBtn;
use crate::protocol::*;
use crate::state::State;

mod river;
pub use river::*;

pub trait WmInfoProvider {
    fn new_ouput(&mut self, conn: &mut Connection<State>, output: WlOutput);
    fn output_removed(&mut self, conn: &mut Connection<State>, output: WlOutput);

    fn get_tags(&self, output: WlOutput) -> Vec<Tag>;
    fn get_layout_name(&self, output: WlOutput) -> Option<String>;
    fn get_mode_name(&self, output: WlOutput) -> Option<String>;

    fn click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        output: WlOutput,
        seat: WlSeat,
        tag: &str,
        btn: PointerBtn,
    );

    fn as_any(&mut self) -> &mut dyn Any;
}

pub fn bind(
    conn: &mut Connection<State>,
    globals: &Globals,
    config: &WmConfig,
) -> Option<Box<dyn WmInfoProvider>> {
    if let Some(river) = RiverInfoProvider::bind(conn, globals, config) {
        return Some(Box::new(river));
    }

    None
}

#[derive(Debug)]
pub struct Tag {
    pub name: String,
    pub is_focused: bool,
    pub is_active: bool,
    pub is_urgent: bool,
}
