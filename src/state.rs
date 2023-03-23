use crate::config::Position;
use crate::protocol::*;
use crate::wm_info_provider::*;

use std::future::pending;

use wayrs_client::connection::Connection;
use wayrs_client::global::{Globals, GlobalsExt};
use wayrs_client::proxy::Proxy;
use wayrs_utils::cursor::CursorTheme;
use wayrs_utils::outputs::{OutputHandler, Outputs};
use wayrs_utils::seats::{SeatHandler, Seats};
use wayrs_utils::shm_alloc::ShmAlloc;

use crate::{
    bar::Bar, config::Config, i3bar_protocol::Block, pointer_btn::PointerBtn,
    shared_state::SharedState, status_cmd::StatusCmd, text::ComputedText, wm_info_provider,
};

pub struct State {
    wl_compositor: WlCompositor,
    layer_shell: ZwlrLayerShellV1,
    viewporter: WpViewporter,
    fractional_scale_manager: Option<WpFractionalScaleManagerV1>,

    seats: Seats,
    pointers: Vec<Pointer>,

    outptus: Outputs,
    pub bars: Vec<Bar>,

    pub shared_state: SharedState,

    cursor_theme: Option<CursorTheme>,
}

struct Pointer {
    seat: WlSeat,
    pointer: WlPointer,
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

        let wl_shm = globals.bind(conn, 1..=1).expect("could not bind wl_shm");

        let mut cursor_theme = CursorTheme::new(None, None);
        let cursor_theme_ok = cursor_theme
            .ensure_cursor_is_loaded("default")
            .map_err(|e| error = Err(e.into()))
            .is_ok();

        let mut this = Self {
            wl_compositor: globals.bind(conn, 4..=5).unwrap(),
            layer_shell: globals.bind(conn, 1..=4).unwrap(),
            viewporter: globals.bind(conn, 1..=1).unwrap(),
            fractional_scale_manager: globals.bind(conn, 1..=1).ok(),

            seats: Seats::bind(conn, globals),
            pointers: Vec::new(),

            outptus: Outputs::bind(conn, globals),
            bars: Vec::new(),

            shared_state: SharedState {
                shm: ShmAlloc::new(wl_shm),
                config,
                status_cmd,
                blocks: Vec::new(),
                blocks_cache: Vec::new(),
                wm_info_provider: wm_info_provider::bind_wayland(conn, globals, wm_info_cb),
            },

            cursor_theme: cursor_theme_ok.then_some(cursor_theme),
        };

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

    fn drop_bar(&mut self, conn: &mut Connection<Self>, bar_index: usize) {
        let bar = self.bars.swap_remove(bar_index);
        bar.layer_surface.destroy(conn);
        bar.surface.destroy(conn);
        if let Some(wm_info_provider) = &mut self.shared_state.wm_info_provider {
            wm_info_provider.output_removed(conn, bar.output);
        }
    }
}

impl SeatHandler for State {
    fn get_seats(&mut self) -> &mut Seats {
        &mut self.seats
    }

    fn pointer_added(&mut self, conn: &mut Connection<Self>, seat: WlSeat) {
        assert!(seat.version() >= 5);
        self.pointers.push(Pointer {
            seat,
            pointer: seat.get_pointer_with_cb(conn, wl_pointer_cb),
            cursor_surface: self.wl_compositor.create_surface(conn),
            current_surface: None,
            x: 0.0,
            y: 0.0,
            pending_button: None,
            pending_scroll: 0.0,
            scroll_frame: ScrollFrame::default(),
        });
    }

    fn pointer_removed(&mut self, conn: &mut Connection<Self>, seat: WlSeat) {
        let pointer_i = self.pointers.iter().position(|p| p.seat == seat).unwrap();
        let pointer = self.pointers.swap_remove(pointer_i);
        pointer.pointer.release(conn);
        pointer.cursor_surface.destroy(conn);
    }
}

impl OutputHandler for State {
    fn get_outputs(&mut self) -> &mut Outputs {
        &mut self.outptus
    }

    fn output_added(&mut self, conn: &mut Connection<Self>, output: WlOutput) {
        let scale = self
            .outptus
            .iter()
            .find(|o| o.wl_output == output)
            .unwrap()
            .scale;

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

        surface.commit(conn);

        self.bars.push(Bar {
            output,
            configured: false,
            frame_cb: None,
            width: 0,
            height: self.shared_state.config.height,
            scale,
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

    fn output_removed(&mut self, conn: &mut Connection<Self>, output: WlOutput) {
        if let Some(bar_i) = self.bars.iter().position(|o| o.output == output) {
            self.drop_bar(conn, bar_i);
        }
    }

    fn info_updated(
        &mut self,
        _: &mut Connection<Self>,
        output: WlOutput,
        mask: wayrs_utils::outputs::UpdatesMask,
    ) {
        if mask.scale {
            let info = self.outptus.iter().find(|o| o.wl_output == output).unwrap();
            let bar = self.bars.iter_mut().find(|b| b.output == output).unwrap();
            bar.scale = info.scale;
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
            if let Some(bar_i) = state
                .bars
                .iter()
                .position(|o| o.layer_surface == layer_surface)
            {
                state.drop_bar(conn, bar_i);
            }
        }
    }
}

fn wl_pointer_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    pointer: WlPointer,
    event: wl_pointer::Event,
) {
    let pointer = state
        .pointers
        .iter_mut()
        .find(|p| p.pointer == pointer)
        .unwrap();

    use wl_pointer::Event;
    match event {
        Event::Frame => {
            let btn = pointer.pending_button.take();
            let scroll = pointer.scroll_frame.finalize();
            if let Some(surface) = pointer.current_surface {
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
                        pointer.seat,
                        pointer.x,
                        pointer.y,
                    )
                    .unwrap();
                }

                if scroll.is_finder && state.shared_state.config.invert_touchpad_scrolling {
                    pointer.pending_scroll -= scroll.absolute;
                } else {
                    pointer.pending_scroll += scroll.absolute;
                }

                if scroll.stop {
                    pointer.pending_scroll = 0.0;
                }

                let btn = if pointer.pending_scroll >= 15.0 {
                    pointer.pending_scroll = 0.0;
                    Some(PointerBtn::WheelDown)
                } else if pointer.pending_scroll <= -15.0 {
                    pointer.pending_scroll = 0.0;
                    Some(PointerBtn::WheelUp)
                } else {
                    None
                };

                if let Some(btn) = btn {
                    bar.click(
                        conn,
                        &mut state.shared_state,
                        btn,
                        pointer.seat,
                        pointer.x,
                        pointer.y,
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
            pointer.current_surface = Some(bar.surface);
            pointer.x = args.surface_x.as_f64();
            pointer.y = args.surface_y.as_f64();
            if let Some(cursor_theme) = &mut state.cursor_theme {
                cursor_theme
                    .set_cursor(
                        conn,
                        &mut state.shared_state.shm,
                        "default",
                        bar.scale,
                        args.serial,
                        pointer.cursor_surface,
                        pointer.pointer,
                    )
                    .unwrap();
            }
        }
        Event::Leave(_) => pointer.current_surface = None,
        Event::Motion(args) => {
            pointer.x = args.surface_x.as_f64();
            pointer.y = args.surface_y.as_f64();
        }
        Event::Button(args) => {
            if args.state == wl_pointer::ButtonState::Pressed {
                pointer.pending_button = Some(args.button.into());
            }
        }
        Event::Axis(args) => {
            if args.axis == wl_pointer::Axis::VerticalScroll {
                pointer.scroll_frame.absolute += args.value.as_f64();
            }
        }
        Event::AxisSource(source) => {
            pointer.scroll_frame.is_finder = source == wl_pointer::AxisSource::Finger;
        }
        Event::AxisStop(args) => {
            if args.axis == wl_pointer::Axis::VerticalScroll {
                pointer.scroll_frame.stop = true;
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
