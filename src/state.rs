use crate::cursor::Cursor;
use crate::protocol::*;

use std::convert::Infallible;
use std::future::pending;

use wayrs_client::connection::Connection;
use wayrs_client::global::{Global, GlobalExt, Globals, GlobalsExt};
use wayrs_client::proxy::{Dispatch, Dispatcher, Proxy};
use wayrs_shm_alloc::ShmAlloc;

use crate::{
    bar::Bar, config::Config, i3bar_protocol::Block, pointer_btn::PointerBtn,
    shared_state::SharedState, status_cmd::StatusCmd, text::ComputedText,
};

pub struct State {
    wl_compositor: WlCompositor,
    layer_shell: ZwlrLayerShellV1,

    river_status_manager: Option<ZriverStatusManagerV1>,
    river_control: Option<ZriverControlV1>,

    seats: Vec<Seat>,
    bars: Vec<Bar>,

    pub shared_state: SharedState,
}

struct Seat {
    seat: WlSeat,
    reg_name: u32,
    pointer: Option<WlPointer>,
    cursor: Cursor,
    cur_surface: Option<WlSurface>,
    x: f64,
    y: f64,
    pending_button: Option<PointerBtn>,
    pending_scroll: f64,
    scroll_frame: ScrollFrame,
}

impl State {
    pub fn new(conn: &mut Connection<Self>, globals: &Globals) -> Self {
        let mut error = Ok(());

        let config = Config::new()
            .map_err(|e| error = Err(e))
            .unwrap_or_default();

        let status_cmd = config
            .command
            .as_ref()
            .and_then(|cmd| StatusCmd::new(cmd).map_err(|e| error = Err(e)).ok());

        let shm = globals.bind(conn, 1..=1).expect("could not bind wl_shm");

        let mut this = Self {
            wl_compositor: globals
                .bind(conn, 4..=5)
                .expect("could not bind wl_compositor"),
            layer_shell: globals
                .bind(conn, 1..=4)
                .expect("could not bind layer_shell"),

            river_status_manager: globals.bind(conn, 1..=4).ok(),
            river_control: globals.bind(conn, 1..=1).ok(),

            seats: Vec::new(),
            bars: Vec::new(),

            shared_state: SharedState {
                shm: ShmAlloc::new(conn, shm, 1024),
                config,
                status_cmd,
                blocks: Vec::new(),
                blocks_cache: Vec::new(),
            },
        };

        globals
            .iter()
            .filter(|g| g.is::<WlSeat>())
            .for_each(|g| this.bind_seat(conn, g));
        globals
            .iter()
            .filter(|g| g.is::<WlOutput>())
            .for_each(|g| this.bind_output(conn, g));

        if let Err(e) = error {
            this.set_error(conn, e.to_string());
        }

        this
    }

    pub fn set_blocks(&mut self, conn: &mut Connection<Self>, blocks: Vec<Block>) {
        self.shared_state.blocks = blocks;
        self.draw_all(conn);
    }

    pub fn set_error(&mut self, conn: &mut Connection<Self>, error: impl Into<String>) {
        self.set_blocks(
            conn,
            vec![Block {
                full_text: error.into(),
                ..Default::default()
            }],
        );
    }

    pub fn draw_all(&mut self, conn: &mut Connection<Self>) {
        for bar in &mut self.bars {
            bar.request_frame(conn);
        }
    }

    pub async fn status_cmd_read(&mut self) -> anyhow::Result<()> {
        match &mut self.shared_state.status_cmd {
            Some(cmd) => cmd.read().await,
            None => {
                pending::<()>().await;
                unreachable!()
            }
        }
    }

    pub fn status_cmd_notify_available(
        &mut self,
        conn: &mut Connection<Self>,
    ) -> anyhow::Result<()> {
        if let Some(cmd) = &mut self.shared_state.status_cmd {
            if let Some(blocks) = cmd.notify_available()? {
                self.set_blocks(conn, blocks);
            }
        }
        Ok(())
    }

    fn bind_output(&mut self, conn: &mut Connection<Self>, global: &Global) {
        let output = global.bind(conn, 2..=4).expect("could not bind wl_output");
        let surface = self.wl_compositor.create_surface(conn);

        use zwlr_layer_shell_v1::Layer;
        use zwlr_layer_surface_v1::Anchor;
        let layer_surface = self.layer_shell.get_layer_surface(
            conn,
            surface,
            output,
            Layer::Top,
            wayrs_client::cstr!("i3bar-river").into(),
        );
        layer_surface.set_size(conn, 0, self.shared_state.config.height);
        layer_surface.set_anchor(conn, Anchor::Top | Anchor::Left | Anchor::Right); // Top + Left + Right
        layer_surface.set_exclusive_zone(conn, self.shared_state.config.height as i32 + 5); // TODO: make the margin configurable

        // Note: layer_surface is commited when we receive the scale factor of this output

        let river_output_status = self
            .river_status_manager
            .as_ref()
            .map(|s| s.get_river_output_status(conn, output));

        self.bars.push(Bar {
            output,
            output_reg_name: global.name,
            configured: false,
            frame_cb: None,
            width: 0,
            height: self.shared_state.config.height,
            scale: 1,
            surface,
            layer_surface,
            blocks_btns: Default::default(),
            river_output_status,
            river_control: self.river_control,
            layout_name: None,
            layout_name_computed: None,
            tags_btns: Default::default(),
            tags_info: Default::default(),
            tags_computed: Vec::new(),
        });
    }

    fn bind_seat(&mut self, conn: &mut Connection<Self>, global: &Global) {
        let seat: WlSeat = global.bind(conn, 5..=8).unwrap();
        let cursor = Cursor::new(conn, self.wl_compositor).unwrap();
        self.seats.push(Seat {
            seat,
            reg_name: global.name,
            pointer: None,
            cursor,
            cur_surface: None,
            x: 0.0,
            y: 0.0,
            pending_button: None,
            pending_scroll: 0.0,
            scroll_frame: ScrollFrame::default(),
        });
    }

    fn drop_bar(&mut self, conn: &mut Connection<Self>, bar_index: usize) {
        let bar = self.bars.swap_remove(bar_index);
        bar.surface.destroy(conn);
        bar.layer_surface.destroy(conn);
        if let Some(output_status) = bar.river_output_status {
            output_status.destroy(conn);
        }
        if bar.output.version() >= 3 {
            bar.output.release(conn);
        }
    }

    fn drop_seat(&mut self, conn: &mut Connection<Self>, seat_index: usize) {
        let seat = self.seats.swap_remove(seat_index);
        if let Some(pointer) = seat.pointer {
            if pointer.version() >= 3 {
                pointer.release(conn);
            }
        }
        if seat.seat.version() >= 5 {
            seat.seat.release(conn);
        }
    }
}

impl Dispatcher for State {
    type Error = Infallible;
}

impl Dispatch<WlRegistry> for State {
    fn event(&mut self, conn: &mut Connection<Self>, _: WlRegistry, event: wl_registry::Event) {
        match event {
            wl_registry::Event::Global(global) if global.is::<WlOutput>() => {
                self.bind_output(conn, &global);
            }
            wl_registry::Event::Global(global) if global.is::<WlSeat>() => {
                self.bind_seat(conn, &global);
            }
            wl_registry::Event::GlobalRemove(name) => {
                if let Some(bar_index) =
                    self.bars.iter().position(|bar| bar.output_reg_name == name)
                {
                    self.drop_bar(conn, bar_index);
                } else if let Some(seat_index) =
                    self.seats.iter().position(|seat| seat.reg_name == name)
                {
                    self.drop_seat(conn, seat_index);
                }
            }
            _ => (),
        }
    }
}

impl Dispatch<WlCallback> for State {
    fn event(&mut self, conn: &mut Connection<Self>, cb: WlCallback, event: wl_callback::Event) {
        let wl_callback::Event::Done(_) = event;
        if let Some(bar) = self.bars.iter_mut().find(|bar| bar.frame_cb == Some(cb)) {
            bar.frame_cb = None;
            bar.frame(conn, &mut self.shared_state);
        }
    }
}

impl Dispatch<WlOutput> for State {
    fn event(&mut self, conn: &mut Connection<Self>, output: WlOutput, event: wl_output::Event) {
        if let wl_output::Event::Scale(scale) = event {
            let bar = self
                .bars
                .iter_mut()
                .find(|bar| bar.output == output)
                .unwrap();
            bar.scale = scale;
            // If bar is not configured yet, it is because we were waiting for the "scale" event
            // before commiting the surface. Otherwise, there is no need to do any redrawing (we'll
            // do that after "configure" event).
            if !bar.configured {
                bar.surface.commit(conn);
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1> for State {
    fn event(
        &mut self,
        conn: &mut Connection<Self>,
        layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure(args) => {
                let bar = self
                    .bars
                    .iter_mut()
                    .find(|bar| bar.layer_surface == layer_surface)
                    .unwrap();
                assert_ne!(args.width, 0);
                bar.width = args.width;
                bar.configured = true;
                bar.layer_surface.ack_configure(conn, args.serial);
                bar.frame(conn, &mut self.shared_state);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                let bar_index = self
                    .bars
                    .iter()
                    .position(|bar| bar.layer_surface == layer_surface)
                    .unwrap();
                self.drop_bar(conn, bar_index);
            }
        }
    }
}

impl Dispatch<WlBuffer> for State {
    fn event(&mut self, _: &mut Connection<Self>, buffer: WlBuffer, event: wl_buffer::Event) {
        let wl_buffer::Event::Release = event;
        self.shared_state.shm.free_buffer(buffer);
    }
}

impl Dispatch<WlSeat> for State {
    fn event(&mut self, conn: &mut Connection<Self>, seat: WlSeat, event: wl_seat::Event) {
        if let wl_seat::Event::Capabilities(capabilities) = event {
            let seat = self.seats.iter_mut().find(|s| s.seat == seat).unwrap();
            match &seat.pointer {
                Some(pointer) if !capabilities.contains(wl_seat::Capability::Pointer) => {
                    if pointer.version() >= 3 {
                        pointer.release(conn);
                    }
                    seat.pointer = None;
                }
                None if capabilities.contains(wl_seat::Capability::Pointer) => {
                    let pointer = seat.seat.get_pointer(conn);
                    seat.pointer = Some(pointer);
                }
                _ => (),
            }
        }
    }
}

impl Dispatch<WlPointer> for State {
    fn event(&mut self, conn: &mut Connection<Self>, pointer: WlPointer, event: wl_pointer::Event) {
        let seat = self
            .seats
            .iter_mut()
            .find(|s| s.pointer == Some(pointer))
            .unwrap();

        use wl_pointer::Event;
        match event {
            Event::Frame => {
                let btn = seat.pending_button.take();
                let scroll = seat.scroll_frame.finalize();
                if let Some(surface) = seat.cur_surface {
                    let bar = self
                        .bars
                        .iter_mut()
                        .find(|bar| bar.surface == surface)
                        .unwrap();

                    if let Some(btn) = btn {
                        bar.click(conn, &mut self.shared_state, btn, seat.seat, seat.x, seat.y)
                            .unwrap();
                    }

                    if scroll.is_finder && self.shared_state.config.invert_touchpad_scrolling {
                        seat.pending_scroll -= scroll.absolute;
                    } else {
                        seat.pending_scroll += scroll.absolute;
                    }

                    if scroll.stop {
                        seat.pending_scroll = 0.0;
                    }

                    let btn = if seat.pending_scroll >= 15.0 {
                        seat.pending_scroll = 0.0;
                        Some(PointerBtn::WheelDown)
                    } else if seat.pending_scroll <= -15.0 {
                        seat.pending_scroll = 0.0;
                        Some(PointerBtn::WheelUp)
                    } else {
                        None
                    };

                    if let Some(btn) = btn {
                        bar.click(conn, &mut self.shared_state, btn, seat.seat, seat.x, seat.y)
                            .unwrap();
                    }
                }
            }
            Event::Enter(args) => {
                let bar = self
                    .bars
                    .iter()
                    .find(|bar| bar.surface.id() == args.surface)
                    .unwrap();
                seat.cur_surface = Some(bar.surface);
                seat.x = args.surface_x.as_f64();
                seat.y = args.surface_y.as_f64();
                seat.cursor.set(
                    conn,
                    args.serial,
                    pointer,
                    bar.scale as u32,
                    &mut self.shared_state.shm,
                );
            }
            Event::Leave(_) => seat.cur_surface = None,
            Event::Motion(args) => {
                seat.x = args.surface_x.as_f64();
                seat.y = args.surface_y.as_f64();
            }
            Event::Button(args) => {
                if args.state == wl_pointer::ButtonState::Pressed {
                    seat.pending_button = Some(args.button.into());
                }
            }
            Event::Axis(args) => {
                if args.axis == wl_pointer::Axis::VerticalScroll {
                    seat.scroll_frame.absolute += args.value.as_f64();
                }
            }
            Event::AxisSource(source) => {
                seat.scroll_frame.is_finder = source == wl_pointer::AxisSource::Finger;
            }
            Event::AxisStop(args) => {
                if args.axis == wl_pointer::Axis::VerticalScroll {
                    seat.scroll_frame.stop = true;
                }
            }
            Event::AxisDiscrete(_) | Event::AxisValue120(_) => (),
        }
    }
}

impl Dispatch<ZriverOutputStatusV1> for State {
    fn event(
        &mut self,
        conn: &mut Connection<Self>,
        status: ZriverOutputStatusV1,
        event: zriver_output_status_v1::Event,
    ) {
        let bar = self
            .bars
            .iter_mut()
            .find(|b| b.river_output_status == Some(status))
            .unwrap();

        use zriver_output_status_v1::Event;
        match event {
            Event::FocusedTags(tags) => bar.tags_info.focused = tags,
            Event::UrgentTags(tags) => bar.tags_info.urgent = tags,
            Event::LayoutName(name) => bar.set_layout_name(Some(name.into_string().unwrap())),
            Event::LayoutNameClear => bar.set_layout_name(None),
            Event::ViewTags(vt) => {
                bar.tags_info.active = vt
                    .chunks_exact(4)
                    .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
                    .fold(0, |a, b| a | b);
            }
        }

        bar.request_frame(conn);
    }
}

impl Dispatch<ZriverCommandCallbackV1> for State {
    fn event(
        &mut self,
        conn: &mut Connection<Self>,
        _: ZriverCommandCallbackV1,
        event: zriver_command_callback_v1::Event,
    ) {
        use zriver_command_callback_v1::Event;
        match event {
            Event::Success(msg) => info!("river_control: {msg:?}"),
            Event::Failure(msg) => self.set_error(conn, msg.into_string().unwrap()),
        }
    }
}

// Interfaces with no events
impl Dispatch<WlCompositor> for State {}
impl Dispatch<ZwlrLayerShellV1> for State {}
impl Dispatch<ZriverStatusManagerV1> for State {}
impl Dispatch<ZriverControlV1> for State {}
impl Dispatch<WlShmPool> for State {}

// Dont care
impl Dispatch<WlShm> for State {}
impl Dispatch<WlSurface> for State {}

#[derive(Debug)]
pub struct ComputedBlock {
    pub block: Block,
    pub full: ComputedText,
    pub short: Option<ComputedText>,
    pub min_width: Option<f64>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScrollFrame {
    stop: bool,
    absolute: f64,
    is_finder: bool,
}

impl ScrollFrame {
    fn finalize(&mut self) -> Self {
        let copy = *self;
        *self = Self::default();
        copy
    }
}
