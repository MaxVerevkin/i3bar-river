use crate::blocks_cache::BlocksCache;
use crate::event_loop::EventLoop;
use crate::output::{Output, PendingOutput};
use crate::protocol::*;
use crate::wm_info_provider;

use std::fmt::Display;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;

use wayrs_client::global::GlobalExt;
use wayrs_client::proxy::Proxy;
use wayrs_client::{Connection, EventCtx};
use wayrs_utils::cursor::{CursorImage, CursorShape, CursorTheme, ThemedPointer};
use wayrs_utils::seats::{SeatHandler, Seats};
use wayrs_utils::shm_alloc::ShmAlloc;

use crate::{
    bar::Bar, config::Config, i3bar_protocol::Block, pointer_btn::PointerBtn,
    shared_state::SharedState, status_cmd::StatusCmd,
};

pub struct State {
    pub wl_compositor: WlCompositor,
    pub layer_shell: ZwlrLayerShellV1,
    pub viewporter: WpViewporter,
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>,

    seats: Seats,
    pointers: Vec<Pointer>,

    // Outputs that haven't yet advertised their names
    pub pending_outputs: Vec<PendingOutput>,

    pub hidden: bool,
    pub has_error: bool,
    pub bars: Vec<Bar>,

    pub shared_state: SharedState,

    cursor_theme: CursorTheme,
    default_cursor: Option<CursorImage>,
}

struct Pointer {
    seat: WlSeat,
    pointer: WlPointer,
    themed_pointer: ThemedPointer,
    current_surface: Option<WlSurface>,
    x: f64,
    y: f64,
    pending_button: Option<PointerBtn>,
    pending_scroll: f64,
    scroll_frame: ScrollFrame,
}

impl State {
    pub fn new(
        conn: &mut Connection<Self>,
        event_loop: &mut EventLoop,
        config_path: Option<&Path>,
    ) -> Self {
        let mut error = Ok(());

        let config = Config::new(config_path)
            .map_err(|e| error = Err(e))
            .unwrap_or_default();

        let status_cmd = config
            .command
            .as_ref()
            .and_then(|cmd| StatusCmd::new(cmd).map_err(|e| error = Err(e)).ok());

        conn.add_registry_cb(wl_registry_cb);
        let wl_compositor = conn.bind_singleton(4..=5).unwrap();

        let cursor_theme = CursorTheme::new(conn, wl_compositor);
        let default_cursor = cursor_theme
            .get_image(CursorShape::Default)
            .map_err(|e| error = Err(e.into()))
            .ok();

        let wm_info_provider = wm_info_provider::bind(conn, &config.wm);
        wm_info_provider.register(event_loop);

        let mut this = Self {
            wl_compositor,
            layer_shell: conn.bind_singleton(1..=4).unwrap(),
            viewporter: conn.bind_singleton(1).unwrap(),
            fractional_scale_manager: conn.bind_singleton(1).ok(),

            seats: Seats::bind(conn),
            pointers: Vec::new(),

            pending_outputs: Vec::new(),

            hidden: config.start_hidden,
            has_error: false,
            bars: Vec::new(),

            shared_state: SharedState {
                shm: ShmAlloc::bind(conn).unwrap(),
                config,
                status_cmd,
                blocks_cache: BlocksCache::default(),
                wm_info_provider,
            },

            cursor_theme,
            default_cursor,
        };

        if let Err(e) = error {
            this.set_error(conn, "init", e.to_string());
        }

        this
    }

    pub fn set_blocks(&mut self, conn: &mut Connection<Self>, blocks: Vec<Block>) {
        if !self.has_error {
            self.shared_state
                .blocks_cache
                .process_new_blocks(&self.shared_state.config, blocks);
            self.draw_all(conn);
        }
    }

    pub fn set_error(&mut self, conn: &mut Connection<Self>, context: &str, error: impl Display) {
        self.set_blocks(
            conn,
            vec![Block {
                full_text: format!("{context}: {error}"),
                ..Default::default()
            }],
        );
        self.has_error = true;
    }

    pub fn draw_all(&mut self, conn: &mut Connection<Self>) {
        for bar in &mut self.bars {
            bar.frame(conn, &mut self.shared_state);
        }
    }

    pub fn status_cmd_fd(&self) -> Option<RawFd> {
        self.shared_state
            .status_cmd
            .as_ref()
            .map(|cmd| cmd.output.as_raw_fd())
    }

    pub fn register_output(&mut self, conn: &mut Connection<Self>, output: Output) {
        if !self.shared_state.config.output_enabled(&output.name) {
            return;
        }

        self.shared_state.wm_info_provider.new_ouput(conn, &output);

        let mut bar = Bar::new(conn, self, output);

        bar.set_tags(self.shared_state.wm_info_provider.get_tags(&bar.output));

        if !self.hidden {
            bar.show(conn, &self.shared_state);
        }

        self.bars.push(bar);
    }

    pub fn drop_bar(&mut self, conn: &mut Connection<Self>, bar_index: usize) {
        let bar = self.bars.swap_remove(bar_index);
        self.shared_state
            .wm_info_provider
            .output_removed(conn, &bar.output);
        bar.destroy(conn);
    }

    pub fn toggle_visibility(&mut self, conn: &mut Connection<Self>) {
        self.hidden = !self.hidden;
        for bar in &mut self.bars {
            if self.hidden {
                bar.hide(conn);
            } else {
                bar.show(conn, &self.shared_state);
            }
        }
    }

    fn for_each_bar<F: FnMut(&mut Bar, &mut SharedState)>(
        &mut self,
        output: Option<WlOutput>,
        mut f: F,
    ) {
        match output {
            Some(output) => f(
                self.bars
                    .iter_mut()
                    .find(|b| b.output.wl == output)
                    .unwrap(),
                &mut self.shared_state,
            ),
            None => self
                .bars
                .iter_mut()
                .for_each(|b| f(b, &mut self.shared_state)),
        }
    }

    pub fn tags_updated(&mut self, conn: &mut Connection<Self>, output: Option<WlOutput>) {
        self.for_each_bar(output, |bar, ss| {
            bar.set_tags(ss.wm_info_provider.get_tags(&bar.output));
            bar.frame(conn, ss);
        });
    }

    pub fn layout_name_updated(&mut self, conn: &mut Connection<Self>, output: Option<WlOutput>) {
        self.for_each_bar(output, |bar, ss| {
            bar.set_layout_name(ss.wm_info_provider.get_layout_name(&bar.output));
            bar.frame(conn, ss);
        });
    }

    pub fn mode_name_updated(&mut self, conn: &mut Connection<Self>, output: Option<WlOutput>) {
        self.for_each_bar(output, |bar, ss| {
            bar.set_mode_name(ss.wm_info_provider.get_mode_name(&bar.output));
            bar.frame(conn, ss);
        });
    }
}

impl SeatHandler for State {
    fn get_seats(&mut self) -> &mut Seats {
        &mut self.seats
    }

    fn pointer_added(&mut self, conn: &mut Connection<Self>, seat: WlSeat) {
        assert!(seat.version() >= 5);
        let pointer = seat.get_pointer_with_cb(conn, wl_pointer_cb);
        self.pointers.push(Pointer {
            seat,
            pointer,
            themed_pointer: self.cursor_theme.get_themed_pointer(conn, pointer),
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
        pointer.themed_pointer.destroy(conn);
        pointer.pointer.release(conn);
    }
}

fn wl_registry_cb(conn: &mut Connection<State>, state: &mut State, event: &wl_registry::Event) {
    match event {
        wl_registry::Event::Global(global) if global.is::<WlOutput>() => {
            state
                .pending_outputs
                .push(PendingOutput::bind(conn, global));
        }
        wl_registry::Event::GlobalRemove(name) => {
            if let Some(bar_index) = state
                .bars
                .iter()
                .position(|bar| bar.output.reg_name == *name)
            {
                state.drop_bar(conn, bar_index);
            }
        }
        _ => (),
    }
}

fn wl_pointer_cb(ctx: EventCtx<State, WlPointer>) {
    let pointer = ctx
        .state
        .pointers
        .iter_mut()
        .find(|p| p.pointer == ctx.proxy)
        .unwrap();

    use wl_pointer::Event;
    match ctx.event {
        Event::Frame => {
            let btn = pointer.pending_button.take();
            let scroll = pointer.scroll_frame.finalize();
            if let Some(surface) = pointer.current_surface {
                let bar = ctx
                    .state
                    .bars
                    .iter_mut()
                    .find(|bar| bar.surface == surface)
                    .unwrap();

                if let Some(btn) = btn {
                    bar.click(
                        ctx.conn,
                        &mut ctx.state.shared_state,
                        btn,
                        pointer.seat,
                        pointer.x,
                        pointer.y,
                    )
                    .unwrap();
                }

                if scroll.is_finger && ctx.state.shared_state.config.invert_touchpad_scrolling {
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
                        ctx.conn,
                        &mut ctx.state.shared_state,
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
            let bar = ctx
                .state
                .bars
                .iter()
                .find(|bar| bar.surface.id() == args.surface)
                .unwrap();
            pointer.current_surface = Some(bar.surface);
            pointer.x = args.surface_x.as_f64();
            pointer.y = args.surface_y.as_f64();
            if let Some(default_cursor) = &ctx.state.default_cursor {
                pointer.themed_pointer.set_cursor(
                    ctx.conn,
                    &mut ctx.state.shared_state.shm,
                    default_cursor,
                    bar.output.scale,
                    args.serial,
                );
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
            pointer.scroll_frame.is_finger = source == wl_pointer::AxisSource::Finger;
        }
        Event::AxisStop(args) => {
            if args.axis == wl_pointer::Axis::VerticalScroll {
                pointer.scroll_frame.stop = true;
            }
        }
        _ => (),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScrollFrame {
    stop: bool,
    absolute: f64,
    is_finger: bool,
}

impl ScrollFrame {
    fn finalize(&mut self) -> Self {
        let copy = *self;
        *self = Self::default();
        copy
    }
}
