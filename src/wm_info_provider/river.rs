use std::any::Any;
use std::ffi::CString;

use wayrs_client::connection::Connection;
use wayrs_client::global::*;

use super::*;
use crate::protocol::*;
use crate::state::State;

pub(super) struct RiverInfoProvider {
    status_manager: ZriverStatusManagerV1,
    control: ZriverControlV1,
    output_statuses: Vec<OutputStatus>,
    callback: WmInfoCallback,
}

struct OutputStatus {
    output: WlOutput,
    status: ZriverOutputStatusV1,
    focused_tags: u32,
    urgent_tags: u32,
    active_tags: u32,
    layout_name: Option<String>,
}

impl RiverInfoProvider {
    pub(super) fn bind(
        conn: &mut Connection<State>,
        globals: &Globals,
        callback: WmInfoCallback,
    ) -> Option<Self> {
        Some(Self {
            status_manager: globals.bind(conn, 1..=4).ok()?,
            control: globals.bind(conn, 1..=1).ok()?,
            output_statuses: Vec::new(),
            callback,
        })
    }
}

impl WmInfoProvider for RiverInfoProvider {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn new_outut(&mut self, conn: &mut Connection<State>, output: WlOutput) {
        let status =
            self.status_manager
                .get_river_output_status_with_cb(conn, output, output_status_cb);
        self.output_statuses.push(OutputStatus {
            output,
            status,
            focused_tags: 0,
            urgent_tags: 0,
            active_tags: 0,
            layout_name: None,
        });
    }

    fn output_removed(&mut self, conn: &mut Connection<State>, output: WlOutput) {
        let index = self
            .output_statuses
            .iter()
            .position(|s| s.output == output)
            .unwrap();
        let output_status = self.output_statuses.swap_remove(index);
        output_status.status.destroy(conn);
    }

    fn left_click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        _: WlOutput,
        seat: WlSeat,
        tag: &str,
    ) {
        let tag: u32 = tag.parse().unwrap();
        self.control
            .add_argument(conn, CString::new("set-focused-tags").unwrap());
        self.control
            .add_argument(conn, CString::new((1u32 << (tag - 1)).to_string()).unwrap());
        self.control
            .run_command_with_cb(conn, seat, river_command_cb);
    }

    fn right_click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        _: WlOutput,
        seat: WlSeat,
        tag: &str,
    ) {
        let tag: u32 = tag.parse().unwrap();
        self.control
            .add_argument(conn, CString::new("toggle-focused-tags").unwrap());
        self.control
            .add_argument(conn, CString::new((1u32 << (tag - 1)).to_string()).unwrap());
        self.control
            .run_command_with_cb(conn, seat, river_command_cb);
    }
}

fn output_status_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    output_status: ZriverOutputStatusV1,
    event: zriver_output_status_v1::Event,
) {
    let river: &mut RiverInfoProvider = state
        .shared_state
        .wm_info_provider
        .as_mut()
        .unwrap()
        .as_any()
        .downcast_mut()
        .unwrap();

    let status = river
        .output_statuses
        .iter_mut()
        .find(|s| s.status == output_status)
        .unwrap();

    use zriver_output_status_v1::Event;
    match event {
        Event::FocusedTags(tags) => status.focused_tags = tags,
        Event::ViewTags(tags) => {
            status.active_tags = tags
                .chunks_exact(4)
                .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                .fold(0, |a, b| a | b);
        }
        Event::UrgentTags(tags) => status.urgent_tags = tags,
        Event::LayoutName(ln) => status.layout_name = Some(ln.to_string_lossy().into()),
        Event::LayoutNameClear => status.layout_name = None,
    }

    let info = WmInfo {
        layout_name: status.layout_name.clone(),
        tags: (1..10)
            .map(|tag| Tag {
                name: tag.to_string(),
                is_focused: status.focused_tags & (1 << (tag - 1)) != 0,
                is_active: status.active_tags & (1 << (tag - 1)) != 0,
                is_urgent: status.urgent_tags & (1 << (tag - 1)) != 0,
            })
            .collect(),
    };

    let output = status.output;
    (river.callback)(conn, state, output, info);
}

fn river_command_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    _: ZriverCommandCallbackV1,
    event: zriver_command_callback_v1::Event,
) {
    if let zriver_command_callback_v1::Event::Failure(msg) = event {
        state.set_error(conn, msg.to_string_lossy())
    }
}
