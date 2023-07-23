use wayrs_client::global::*;
use wayrs_client::Connection;

use crate::pointer_btn::PointerBtn;
use crate::protocol::*;
use crate::state::State;

#[cfg(feature = "hyprland")]
mod ext_workspace_unstable;
#[cfg(feature = "hyprland")]
use ext_workspace_unstable::*;

mod river;
use river::*;

pub enum WmInfoProvider {
    None,
    River(RiverInfoProvider),
    #[cfg(feature = "hyprland")]
    Ewu(ExtWorkspaceUnstable),
}

pub type WmInfoCallback = fn(&mut Connection<State>, &mut State, WlOutput, WmInfo);

impl WmInfoProvider {
    pub fn bind(
        conn: &mut Connection<State>,
        globals: &Globals,
        callback: WmInfoCallback,
    ) -> WmInfoProvider {
        if let Some(river) = RiverInfoProvider::bind(conn, globals, callback) {
            return Self::River(river);
        }

        #[cfg(feature = "hyprland")]
        if let Some(ext_wp_u) = ExtWorkspaceUnstable::bind(conn, globals, callback) {
            return Self::Ewu(ext_wp_u);
        }

        Self::None
    }

    pub fn new_ouput(&mut self, conn: &mut Connection<State>, output: WlOutput) {
        match self {
            Self::None => (),
            Self::River(x) => x.new_output(conn, output),
            #[cfg(feature = "hyprland")]
            Self::Ewu(x) => x.new_ouput(conn, output),
        }
    }

    pub fn output_removed(&mut self, conn: &mut Connection<State>, output: WlOutput) {
        match self {
            Self::None => (),
            Self::River(x) => x.output_removed(conn, output),
            #[cfg(feature = "hyprland")]
            Self::Ewu(x) => x.output_removed(conn, output),
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
            Self::River(x) => x.click_on_tag(conn, output, seat, tag, btn),
            #[cfg(feature = "hyprland")]
            Self::Ewu(x) => x.click_on_tag(conn, output, seat, tag, btn),
        }
    }
}

#[derive(Default, Debug)]
pub struct WmInfo {
    pub layout_name: Option<String>,
    pub tags: Vec<Tag>,
}

#[derive(Debug)]
pub struct Tag {
    pub name: String,
    pub is_focused: bool,
    pub is_active: bool,
    pub is_urgent: bool,
}
