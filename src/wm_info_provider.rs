mod river;

use wayrs_client::connection::Connection;
use wayrs_client::global::*;

use crate::pointer_btn::PointerBtn;
use crate::protocol::*;
use crate::state::State;

pub enum WmInfoProvider {
    None,
    River(river::RiverInfoProvider),
}

pub type WmInfoCallback = fn(&mut Connection<State>, &mut State, WlOutput, WmInfo);

impl WmInfoProvider {
    pub fn bind(
        conn: &mut Connection<State>,
        globals: &Globals,
        callback: WmInfoCallback,
    ) -> WmInfoProvider {
        let Some(river) = river::RiverInfoProvider::bind(conn, globals, callback) else { return Self::None };
        Self::River(river)
    }

    pub fn new_ouput(&mut self, conn: &mut Connection<State>, output: WlOutput) {
        match self {
            Self::None => (),
            Self::River(river) => river.new_output(conn, output),
        }
    }

    pub fn output_removed(&mut self, conn: &mut Connection<State>, output: WlOutput) {
        match self {
            Self::None => (),
            Self::River(river) => river.output_removed(conn, output),
        }
    }

    pub fn click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        output: WlOutput,
        seat: WlSeat,
        tag: &str,
        btn: PointerBtn,
    ) {
        match self {
            Self::None => (),
            Self::River(river) => river.click_on_tag(conn, output, seat, tag, btn),
        }
    }
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
