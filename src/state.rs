use crate::config::Position;
use crate::cursor::CursorTheme;
use crate::protocol::*;
use crate::wm_info_provider::*;

use std::future::pending;

use wayrs_client::connection::Connection;
use wayrs_client::global::{Global, GlobalExt, Globals, GlobalsExt};
use wayrs_client::proxy::Proxy;
use wayrs_shm_alloc::{ShmAlloc, ShmAllocState};

use crate::{
    bar::Bar, config::Config, i3bar_protocol::Block, pointer_btn::PointerBtn,
    shared_state::SharedState, status_cmd::StatusCmd, text::ComputedText, wm_info_provider,
};

pub struct State {
    wl_compositor: WlCompositor,
    layer_shell: ZwlrLayerShellV1,
    viewporter: WpViewporter,
    fractional_scale_manager: Option<WpFractionalScaleManagerV1>,

    pub seats: Vec<Seat>,
    pub bars: Vec<Bar>,

    pub shared_state: SharedState,

    cursor_theme: Option<CursorTheme>,
}

pub struct Seat {
    seat: WlSeat,
    reg_name: u32,
    pointer: Option<WlPointer>,
    cursor_surface: WlSurface,
    current_surface: Option<WlSurface>,
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

        conn.set_callback_for(conn.registry(), wl_registry_cb);

        let shm = globals.bind(conn, 1..=1).expect("could not bind wl_shm");

        let mut cursor_theme = CursorTheme::new();
        let cursor_theme_ok = cursor_theme
            .ensure_cursor_is_loaded("default")
            .map_err(|e| error = Err(e.into()))
            .is_ok();

        let mut this = Self {
            wl_compositor: globals.bind(conn, 4..=5).unwrap(),
            layer_shell: globals.bind(conn, 1..=4).unwrap(),
            viewporter: globals.bind(conn, 1..=1).unwrap(),
            fractional_scale_manager: globals.bind(conn, 1..=1).ok(),

            seats: Vec::new(),
            bars: Vec::new(),

            shared_state: SharedState {
                shm: ShmAlloc::new(conn, shm, 1024),
                config,
                status_cmd,
                blocks: Vec::new(),
                blocks_cache: Vec::new(),
                wm_info_provider: wm_info_provider::bind_wayland(conn, globals, wm_info_cb),
            },

            cursor_theme: cursor_theme_ok.then_some(cursor_theme),
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
        let output = global
            .bind_with_cb(conn, 2..=4, wl_output_cb)
            .expect("could not bind wl_output");

        if let Some(wm_info_provider) = &mut self.shared_state.wm_info_provider {
            wm_info_provider.new_outut(conn, output);
        }

        let surface = self.wl_compositor.create_surface(conn);

        let fractional_scale = self
            .fractional_scale_manager
            .map(|mgr| mgr.get_fractional_scale_with_cb(conn, surface, fractional_scale_cb));

        let layer_surface = self.layer_shell.get_layer_surface_with_cb(
            conn,
            surface,
            output,
            zwlr_layer_shell_v1::Layer::Top,
            wayrs_client::cstr!("i3bar-river").into(),
            layer_surface_cb,
        );

        let config = &self.shared_state.config;

        layer_surface.set_size(conn, 0, config.height);
        layer_surface.set_anchor(conn, config.position.into());
        layer_surface.set_margin(
            conn,
            config.margin_top,
            config.margin_right,
            config.margin_bottom,
            config.margin_left,
        );
        layer_surface.set_exclusive_zone(
            conn,
            (self.shared_state.config.height) as i32
                + if config.position == Position::Top {
                    self.shared_state.config.margin_bottom
                } else {
                    self.shared_state.config.margin_top
                },
        );

        // Note: layer_surface is commited when we receive the scale factor of this output

        self.bars.push(Bar {
            output,
            output_reg_name: global.name,
            configured: false,
            frame_cb: None,
            width: 0,
            height: self.shared_state.config.height,
            scale: 1,
            scale120: None,
            surface,
            viewport: self.viewporter.get_viewport(conn, surface),
            fractional_scale,
            layer_surface,
            blocks_btns: Default::default(),
            wm_info: Default::default(),
            tags_btns: Default::default(),
            tags_computed: Vec::new(),
            layout_name_computed: None,
        });
    }

    fn bind_seat(&mut self, conn: &mut Connection<Self>, global: &Global) {
        let seat: WlSeat = global.bind_with_cb(conn, 5..=8, wl_seat_cb).unwrap();
        self.seats.push(Seat {
            seat,
            reg_name: global.name,
            pointer: None,
            cursor_surface: self.wl_compositor.create_surface(conn),
            current_surface: None,
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
        if let Some(wm_info_provider) = &mut self.shared_state.wm_info_provider {
            wm_info_provider.output_removed(conn, bar.output);
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

        seat.cursor_surface.destroy(conn);
    }
}

impl ShmAllocState for State {
    fn shm_alloc(&mut self) -> &mut ShmAlloc {
        &mut self.shared_state.shm
    }
}

fn wl_registry_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    _: WlRegistry,
    event: wl_registry::Event,
) {
    match event {
        wl_registry::Event::Global(global) if global.is::<WlOutput>() => {
            state.bind_output(conn, &global);
        }
        wl_registry::Event::Global(global) if global.is::<WlSeat>() => {
            state.bind_seat(conn, &global);
        }
        wl_registry::Event::GlobalRemove(name) => {
            if let Some(bar_index) = state
                .bars
                .iter()
                .position(|bar| bar.output_reg_name == name)
            {
                state.drop_bar(conn, bar_index);
            } else if let Some(seat_index) =
                state.seats.iter().position(|seat| seat.reg_name == name)
            {
                state.drop_seat(conn, seat_index);
            }
        }
        _ => (),
    }
}

fn wl_output_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    output: WlOutput,
    event: wl_output::Event,
) {
    if let wl_output::Event::Scale(scale) = event {
        let bar = state
            .bars
            .iter_mut()
            .find(|bar| bar.output == output)
            .unwrap();
        bar.scale = scale as u32;
        // If bar is not configured yet, it is because we were waiting for the "scale" event
        // before commiting the surface. Otherwise, there is no need to do any redrawing (we'll
        // do that after "configure" event).
        if !bar.configured {
            bar.surface.commit(conn);
        }
    }
}

fn layer_surface_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    event: zwlr_layer_surface_v1::Event,
) {
    match event {
        zwlr_layer_surface_v1::Event::Configure(args) => {
            let bar = state
                .bars
                .iter_mut()
                .find(|bar| bar.layer_surface == layer_surface)
                .unwrap();
            assert_ne!(args.width, 0);
            bar.width = args.width;
            bar.layer_surface.ack_configure(conn, args.serial);
            if bar.configured {
                bar.request_frame(conn);
            } else {
                bar.configured = true;
                bar.frame(conn, &mut state.shared_state);
            }
        }
        zwlr_layer_surface_v1::Event::Closed => {
            let bar_index = state
                .bars
                .iter()
                .position(|bar| bar.layer_surface == layer_surface)
                .unwrap();
            state.drop_bar(conn, bar_index);
        }
    }
}

fn wl_seat_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    seat: WlSeat,
    event: wl_seat::Event,
) {
    if let wl_seat::Event::Capabilities(capabilities) = event {
        let seat = state.seats.iter_mut().find(|s| s.seat == seat).unwrap();
        match &seat.pointer {
            Some(pointer) if !capabilities.contains(wl_seat::Capability::Pointer) => {
                if pointer.version() >= 3 {
                    pointer.release(conn);
                }
                seat.pointer = None;
            }
            None if capabilities.contains(wl_seat::Capability::Pointer) => {
                seat.pointer = Some(seat.seat.get_pointer_with_cb(conn, wl_pointer_cb));
            }
            _ => (),
        }
    }
}

fn wl_pointer_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    pointer: WlPointer,
    event: wl_pointer::Event,
) {
    let seat = state
        .seats
        .iter_mut()
        .find(|s| s.pointer == Some(pointer))
        .unwrap();

    use wl_pointer::Event;
    match event {
        Event::Frame => {
            let btn = seat.pending_button.take();
            let scroll = seat.scroll_frame.finalize();
            if let Some(surface) = seat.current_surface {
                let bar = state
                    .bars
                    .iter_mut()
                    .find(|bar| bar.surface == surface)
                    .unwrap();

                if let Some(btn) = btn {
                    bar.click(
                        conn,
                        &mut state.shared_state,
                        btn,
                        seat.seat,
                        seat.x,
                        seat.y,
                    )
                    .unwrap();
                }

                if scroll.is_finder && state.shared_state.config.invert_touchpad_scrolling {
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
                    bar.click(
                        conn,
                        &mut state.shared_state,
                        btn,
                        seat.seat,
                        seat.x,
                        seat.y,
                    )
                    .unwrap();
                }
            }
        }
        Event::Enter(args) => {
            let bar = state
                .bars
                .iter()
                .find(|bar| bar.surface.id() == args.surface)
                .unwrap();
            seat.current_surface = Some(bar.surface);
            seat.x = args.surface_x.as_f64();
            seat.y = args.surface_y.as_f64();
            if let Some(cursor_theme) = &mut state.cursor_theme {
                cursor_theme
                    .set_cursor(
                        conn,
                        "default",
                        bar.scale,
                        args.serial,
                        &mut state.shared_state.shm,
                        seat.cursor_surface,
                        pointer,
                    )
                    .unwrap();
            }
        }
        Event::Leave(_) => seat.current_surface = None,
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

fn fractional_scale_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    fractional_scale: WpFractionalScaleV1,
    event: wp_fractional_scale_v1::Event,
) {
    let wp_fractional_scale_v1::Event::PreferredScale(scale120) = event;
    let bar = state
        .bars
        .iter_mut()
        .find(|b| b.fractional_scale == Some(fractional_scale))
        .unwrap();
    if bar.scale120 != Some(scale120) {
        bar.scale120 = Some(scale120);
        bar.request_frame(conn);
    }
}

fn wm_info_cb(conn: &mut Connection<State>, state: &mut State, output: WlOutput, info: WmInfo) {
    let bar = state.bars.iter_mut().find(|b| b.output == output).unwrap();
    bar.set_wm_info(info);
    bar.request_frame(conn);
}

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
