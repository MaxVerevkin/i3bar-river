use std::ffi::CString;

use wayrs_client::EventCtx;
use wayrs_client::proxy::Proxy;

use super::*;

pub struct RiverInfoProvider {
    status_manager: ZriverStatusManagerV1,
    control: ZriverControlV1,
    output_statuses: Vec<OutputStatus>,
    max_tag: u8,
    mode: Option<String>,
}

struct OutputStatus {
    output: WlOutput,
    status: ZriverOutputStatusV1,
    focused_tags: u32,
    urgent_tags: u32,
    active_tags: u32,
    layout_name: Option<String>,
    is_focused: bool,
}

impl RiverInfoProvider {
    pub fn bind(conn: &mut Connection<State>, config: &WmConfig) -> Option<Self> {
        let control = conn.bind_singleton(1).ok()?;
        let status_manager: ZriverStatusManagerV1 = conn.bind_singleton(..=4).ok()?;
        let wl_seat = conn.bind_singleton(..=5).ok()?; // river supports just one seat
        status_manager.get_river_seat_status_with_cb(conn, wl_seat, seat_status_cb);
        if wl_seat.version() >= 5 {
            wl_seat.release(conn);
        }
        Some(Self {
            status_manager,
            control,
            output_statuses: Vec::new(),
            max_tag: config.river.max_tag,
            mode: None,
        })
    }

    fn set_focused_tags(&self, seat: WlSeat, conn: &mut Connection<State>, tags: u32) {
        self.control
            .add_argument(conn, c"set-focused-tags".to_owned());
        self.control
            .add_argument(conn, CString::new(tags.to_string()).unwrap());
        self.control
            .run_command_with_cb(conn, seat, river_command_cb);
    }
}

impl WmInfoProvider for RiverInfoProvider {
    fn new_ouput(&mut self, conn: &mut Connection<State>, output: &Output) {
        let status =
            self.status_manager
                .get_river_output_status_with_cb(conn, output.wl, output_status_cb);
        self.output_statuses.push(OutputStatus {
            output: output.wl,
            status,
            focused_tags: 0,
            urgent_tags: 0,
            active_tags: 0,
            layout_name: None,
            is_focused: true,
        });
    }

    fn output_removed(&mut self, conn: &mut Connection<State>, output: &Output) {
        let index = self
            .output_statuses
            .iter()
            .position(|s| s.output == output.wl)
            .unwrap();
        let output_status = self.output_statuses.swap_remove(index);
        output_status.status.destroy(conn);
    }

    fn get_tags(&self, output: &Output) -> Vec<Tag> {
        let Some(status) = self.output_statuses.iter().find(|s| s.output == output.wl) else {
            return Vec::new();
        };
        (1..=u8::min(self.max_tag, 32))
            .map(|tag| Tag {
                id: tag as u32,
                name: tag.to_string(),
                is_focused: status.focused_tags & (1 << (tag - 1)) != 0,
                is_active: status.active_tags & (1 << (tag - 1)) != 0,
                is_urgent: status.urgent_tags & (1 << (tag - 1)) != 0,
            })
            .collect()
    }

    fn get_layout_name(&self, output: &Output) -> Option<String> {
        let status = self
            .output_statuses
            .iter()
            .find(|s| s.output == output.wl)?;
        status.layout_name.clone()
    }

    fn get_mode_name(&self, _output: &Output) -> Option<String> {
        self.mode.clone()
    }

    fn is_output_focused(&self, output: &Output) -> bool {
        self.output_statuses
            .iter()
            .find(|s| s.output == output.wl)
            .map(|s| s.is_focused)
            .unwrap_or(true)
    }

    fn click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        output: &Output,
        seat: WlSeat,
        tag_id: Option<u32>,
        btn: PointerBtn,
    ) {
        match btn {
            PointerBtn::Left => {
                if let Some(tag_id) = tag_id {
                    self.set_focused_tags(seat, conn, 1u32 << (tag_id - 1));
                }
            }
            PointerBtn::Right => {
                if let Some(tag_id) = tag_id {
                    self.control
                        .add_argument(conn, c"toggle-focused-tags".to_owned());
                    self.control.add_argument(
                        conn,
                        CString::new((1u32 << (tag_id - 1)).to_string()).unwrap(),
                    );
                    self.control
                        .run_command_with_cb(conn, seat, river_command_cb);
                }
            }
            PointerBtn::WheelUp | PointerBtn::WheelDown => {
                if let Some(status) = self.output_statuses.iter().find(|s| s.output == output.wl) {
                    let mut new_tags = if btn == PointerBtn::WheelUp {
                        status.focused_tags >> 1
                    } else {
                        status.focused_tags << 1
                    };
                    if new_tags == 0 {
                        new_tags |= status.focused_tags & 0x8000_0001;
                    }
                    self.set_focused_tags(seat, conn, new_tags);
                }
            }
            _ => (),
        }
    }
}

fn output_status_cb(ctx: EventCtx<State, ZriverOutputStatusV1>) {
    let river = ctx.state.shared_state.get_river().unwrap();
    let status = river
        .output_statuses
        .iter_mut()
        .find(|s| s.status == ctx.proxy)
        .unwrap();
    let output = status.output;

    use zriver_output_status_v1::Event;
    match ctx.event {
        Event::FocusedTags(tags) => {
            status.focused_tags = tags;
            ctx.state.tags_updated(ctx.conn, Some(output));
        }
        Event::ViewTags(tags) => {
            status.active_tags = tags
                .chunks_exact(4)
                .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                .fold(0, |a, b| a | b);
            ctx.state.tags_updated(ctx.conn, Some(output));
        }
        Event::UrgentTags(tags) => {
            status.urgent_tags = tags;
            ctx.state.tags_updated(ctx.conn, Some(output));
        }
        Event::LayoutName(ln) => {
            status.layout_name = Some(ln.to_string_lossy().into());
            ctx.state.layout_name_updated(ctx.conn, Some(output));
        }
        Event::LayoutNameClear => {
            status.layout_name = None;
            ctx.state.layout_name_updated(ctx.conn, Some(output));
        }
    }
}

fn seat_status_cb(ctx: EventCtx<State, ZriverSeatStatusV1>) {
    match ctx.event {
        zriver_seat_status_v1::Event::Mode(mode) => {
            let river = ctx.state.shared_state.get_river().unwrap();
            let mode = mode.to_string_lossy().into_owned();
            river.mode = (mode != "normal").then_some(mode);
            ctx.state.mode_name_updated(ctx.conn, None);
        }
        zriver_seat_status_v1::Event::FocusedOutput(output) => {
            let river = ctx.state.shared_state.get_river().unwrap();
            let Some(status) = river
                .output_statuses
                .iter_mut()
                .find(|s| s.output == output)
            else {
                return;
            };
            status.is_focused = true;
            ctx.state.tags_updated(ctx.conn, None);
        }
        zriver_seat_status_v1::Event::UnfocusedOutput(output) => {
            let river = ctx.state.shared_state.get_river().unwrap();
            let Some(status) = river
                .output_statuses
                .iter_mut()
                .find(|s| s.output == output)
            else {
                return;
            };
            status.is_focused = false;
            ctx.state.tags_updated(ctx.conn, None);
        }
        _ => {}
    }
}

fn river_command_cb(ctx: EventCtx<State, ZriverCommandCallbackV1>) {
    if let zriver_command_callback_v1::Event::Failure(msg) = ctx.event {
        ctx.state
            .set_error(ctx.conn, "river", msg.to_string_lossy())
    }
}
