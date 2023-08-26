use std::ffi::CString;

use wayrs_client::global::*;
use wayrs_client::{cstr, Connection};

use super::*;
use crate::state::State;

pub struct RiverInfoProvider {
    status_manager: ZriverStatusManagerV1,
    control: ZriverControlV1,
    output_statuses: Vec<OutputStatus>,
    max_tag: u8,
    seat_status: SeatStatus,
}

struct OutputStatus {
    output: WlOutput,
    status: ZriverOutputStatusV1,
    focused_tags: u32,
    urgent_tags: u32,
    active_tags: u32,
    layout_name: Option<String>,
}

struct SeatStatus {
    _status: ZriverSeatStatusV1,
    mode: Option<String>,
}

impl RiverInfoProvider {
    pub fn bind(
        conn: &mut Connection<State>,
        globals: &Globals,
        config: &WmConfig,
    ) -> Option<Self> {
        let status_manager = globals.bind(conn, 1..=4).ok()?;
        let wl_seat: WlSeat = globals.bind(conn, ..).ok()?; // river supports just one seat
        Some(Self {
            status_manager,
            control: globals.bind(conn, 1..=1).ok()?,
            output_statuses: Vec::new(),
            max_tag: config.river.max_tag,
            seat_status: SeatStatus {
                _status: status_manager.get_river_seat_status_with_cb(
                    conn,
                    wl_seat,
                    seat_status_cb,
                ),
                mode: None,
            },
        })
    }
}

impl WmInfoProvider for RiverInfoProvider {
    fn new_ouput(&mut self, conn: &mut Connection<State>, output: WlOutput) {
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

    fn get_tags(&self, output: WlOutput) -> Vec<Tag> {
        let Some(status) = self.output_statuses.iter().find(|s| s.output == output) else {
            return Vec::new();
        };
        (1..=self.max_tag)
            .map(|tag| Tag {
                name: tag.to_string(),
                is_focused: status.focused_tags & (1 << (tag - 1)) != 0,
                is_active: status.active_tags & (1 << (tag - 1)) != 0,
                is_urgent: status.urgent_tags & (1 << (tag - 1)) != 0,
            })
            .collect()
    }

    fn get_layout_name(&self, output: WlOutput) -> Option<String> {
        let status = self.output_statuses.iter().find(|s| s.output == output)?;
        status.layout_name.clone()
    }

    fn get_mode_name(&self, _output: WlOutput) -> Option<String> {
        self.seat_status.mode.clone()
    }

    fn click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        _output: WlOutput,
        seat: WlSeat,
        tag: &str,
        btn: PointerBtn,
    ) {
        let tag: u32 = tag.parse().unwrap();
        let cmd = match btn {
            PointerBtn::Left => cstr!("set-focused-tags"),
            PointerBtn::Right => cstr!("toggle-focused-tags"),
            _ => return,
        };
        self.control.add_argument(conn, cmd.to_owned());
        self.control
            .add_argument(conn, CString::new((1u32 << (tag - 1)).to_string()).unwrap());
        self.control
            .run_command_with_cb(conn, seat, river_command_cb);
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

fn output_status_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    output_status: ZriverOutputStatusV1,
    event: zriver_output_status_v1::Event,
) {
    let river = state.shared_state.get_river().unwrap();
    let status = river
        .output_statuses
        .iter_mut()
        .find(|s| s.status == output_status)
        .unwrap();
    let output = status.output;

    use zriver_output_status_v1::Event;
    match event {
        Event::FocusedTags(tags) => {
            status.focused_tags = tags;
            state.tags_updated(conn, Some(output));
        }
        Event::ViewTags(tags) => {
            status.active_tags = tags
                .chunks_exact(4)
                .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                .fold(0, |a, b| a | b);
            state.tags_updated(conn, Some(output));
        }
        Event::UrgentTags(tags) => {
            status.urgent_tags = tags;
            state.tags_updated(conn, Some(output));
        }
        Event::LayoutName(ln) => {
            status.layout_name = Some(ln.to_string_lossy().into());
            state.layout_name_updated(conn, Some(output));
        }
        Event::LayoutNameClear => {
            status.layout_name = None;
            state.layout_name_updated(conn, Some(output));
        }
    }
}

fn seat_status_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    _: ZriverSeatStatusV1,
    event: zriver_seat_status_v1::Event,
) {
    if let zriver_seat_status_v1::Event::Mode(mode) = event {
        let river = state.shared_state.get_river().unwrap();
        let mode = mode.to_string_lossy().into_owned();
        river.seat_status.mode = (mode != "normal").then_some(mode);
        state.mode_name_updated(conn, None);
    }
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
