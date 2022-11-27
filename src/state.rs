use std::{future::pending, io};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::client::{
        protocol::{wl_output, wl_pointer, wl_seat, wl_surface},
        Connection, EventQueue, QueueHandle,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer},
        Capability, SeatHandler, SeatState,
    },
    shell::layer::{
        Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
    },
    shm::{ShmHandler, ShmState},
};
use tokio::io::unix::AsyncFdReadyGuard;
use wayland_client::globals::GlobalList;

use crate::{
    bar::Bar,
    config::Config,
    delegate_river_control, delegate_river_status,
    i3bar_protocol::Block,
    pointer_btn::PointerBtn,
    river_protocols::{
        control::{RiverControlHandler, RiverControlState},
        status::{RiverOutputStatus, RiverStatusHandler, RiverStatusState},
    },
    shared_state::SharedState,
    status_cmd::StatusCmd,
    text::ComputedText,
};

pub struct State {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor_state: CompositorState,
    layer_state: LayerShell,
    river_status_state: Option<RiverStatusState>,
    river_control_state: Option<RiverControlState>,

    seats: Vec<Seat>,
    bars: Vec<Bar>,
    pub shared_state: SharedState,
}

struct Seat {
    seat: wl_seat::WlSeat,
    pointer: wl_pointer::WlPointer,
    themed_pointer: ThemedPointer,
    pointer_surface: wl_surface::WlSurface,
    finger_scroll: f64,
}

impl State {
    pub fn new(event_queue: &mut EventQueue<Self>, globals: &GlobalList) -> Self {
        let mut error = Ok(());

        let qh = event_queue.handle();

        let config = Config::new()
            .map_err(|e| error = Err(e))
            .unwrap_or_default();

        let status_cmd = match &error {
            Err(_) => None,
            Ok(()) => config.command.as_ref().and_then(|cmd| {
                StatusCmd::new(cmd)
                    .map_err(|e| error = Err(anyhow!(e)))
                    .ok()
            }),
        };

        let mut this = Self {
            registry_state: RegistryState::new(globals),
            seat_state: SeatState::new(globals, &qh),
            output_state: OutputState::new(globals, &qh),
            compositor_state: CompositorState::bind(globals, &qh).unwrap(),
            layer_state: LayerShell::bind(globals, &qh).unwrap(),
            river_status_state: RiverStatusState::new(globals, &qh).ok(),
            river_control_state: RiverControlState::new(globals, &qh).ok(),

            seats: Vec::new(),
            bars: Vec::new(),
            shared_state: SharedState {
                qh: event_queue.handle(),
                shm_state: ShmState::bind(globals, &qh).unwrap(),
                pool: None,
                config,
                status_cmd,
                blocks: Vec::new(),
                blocks_cache: Vec::new(),
            },
        };

        if let Err(e) = error {
            this.set_error(e.to_string());
        }

        this
    }

    pub fn set_blocks(&mut self, blocks: Vec<Block>) {
        self.shared_state.blocks = blocks;
        self.draw_all();
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.set_blocks(vec![Block {
            full_text: error.into(),
            ..Default::default()
        }]);
    }

    pub fn notify_available(&mut self) -> anyhow::Result<()> {
        if let Some(cmd) = &mut self.shared_state.status_cmd {
            if let Some(blocks) = cmd.notify_available()? {
                self.set_blocks(blocks);
            }
        }
        Ok(())
    }

    pub fn draw_all(&mut self) {
        for bar in &mut self.bars {
            bar.draw(&mut self.shared_state);
        }
    }

    pub async fn wait_for_status_cmd(&self) -> io::Result<AsyncFdReadyGuard<i32>> {
        match &self.shared_state.status_cmd {
            Some(cmd) => cmd.async_fd.readable().await,
            None => {
                pending::<()>().await;
                unreachable!()
            }
        }
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if let Some(bar) = self
            .bars
            .iter_mut()
            .find(|bar| bar.layer.wl_surface() == surface)
        {
            bar.scale = scale;
            bar.draw(&mut self.shared_state);
        }
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let height = self.shared_state.config.height;
        let surface = self.compositor_state.create_surface(qh).unwrap();
        let layer = LayerSurface::builder()
            .output(&output)
            .size((0, height))
            .anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT)
            .namespace("i3bar-river")
            .map(qh, &self.layer_state, surface, Layer::Top)
            .expect("layer surface creation");
        let river_output_status = self
            .river_status_state
            .as_mut()
            .map(|s| s.new_output_status(qh, &output));
        self.bars.push(Bar {
            configured: false,
            width: 0,
            height,
            scale: 1,
            layer,
            blocks_btns: Default::default(),
            river_output_status,
            river_control: self.river_control_state.clone(),
            layout_name: None,
            tags_btns: Default::default(),
            tags_info: Default::default(),
            tags_computed: Vec::new(),
        });
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        self.bars.retain(|b| &b.layer != layer)
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.bars
            .iter_mut()
            .find(|b| &b.layer == layer)
            .unwrap()
            .configure(&mut self.shared_state, configure.new_size.0);
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            let (pointer, themed_pointer) = self
                .seat_state
                .get_pointer_with_theme(qh, &seat, ThemeSpec::System, 1)
                .expect("Failed to create pointer");
            self.seats.push(Seat {
                seat,
                pointer,
                themed_pointer,
                pointer_surface: self.compositor_state.create_surface(qh).unwrap(), // TODO: make a PR to remove this unwrap
                finger_scroll: 0.0,
            });
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            self.seats.retain(|p| p.seat == seat);
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let seat = self
            .seats
            .iter_mut()
            .find(|p| &p.pointer == pointer)
            .unwrap();
        for event in events {
            if let Some(bar) = self
                .bars
                .iter_mut()
                .find(|b| b.layer.wl_surface() == &event.surface)
            {
                match event.kind {
                    PointerEventKind::Enter { .. } => {
                        seat.themed_pointer
                            .set_cursor(
                                conn,
                                "default",
                                self.shared_state.shm_state.wl_shm(),
                                &seat.pointer_surface,
                            )
                            .unwrap();
                    }
                    PointerEventKind::Press { button, .. } => {
                        bar.click(
                            &mut self.shared_state,
                            button.into(),
                            &seat.seat,
                            event.position.0,
                            event.position.1,
                        )
                        .unwrap();
                    }
                    PointerEventKind::Axis {
                        vertical, source, ..
                    } => {
                        if source == Some(wl_pointer::AxisSource::Finger) {
                            if self.shared_state.config.invert_touchpad_scrolling {
                                seat.finger_scroll -= vertical.absolute;
                            } else {
                                seat.finger_scroll += vertical.absolute;
                            }
                            if vertical.stop {
                                seat.finger_scroll = 0.0;
                            }
                        }

                        let btn = if vertical.discrete > 0 {
                            PointerBtn::WheelDown
                        } else if vertical.discrete < 0 {
                            PointerBtn::WheelUp
                        } else if seat.finger_scroll >= 15.0 {
                            seat.finger_scroll = 0.0;
                            PointerBtn::WheelDown
                        } else if seat.finger_scroll <= -15.0 {
                            seat.finger_scroll = 0.0;
                            PointerBtn::WheelUp
                        } else {
                            continue;
                        };

                        bar.click(
                            &mut self.shared_state,
                            btn,
                            &seat.seat,
                            event.position.0,
                            event.position.1,
                        )
                        .unwrap();
                    }
                    _ => (),
                }
            }
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut ShmState {
        &mut self.shared_state.shm_state
    }
}

impl RiverStatusHandler for State {
    fn river_status_state(&mut self) -> &mut RiverStatusState {
        self.river_status_state.as_mut().unwrap()
    }

    fn focused_tags_updated(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        focused: u32,
    ) {
        let bar = self
            .bars
            .iter_mut()
            .find(|b| b.river_output_status.as_ref() == Some(output_status))
            .unwrap();
        bar.tags_info.focused = focused;
        bar.draw(&mut self.shared_state);
    }

    fn urgent_tags_updated(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        urgent: u32,
    ) {
        let bar = self
            .bars
            .iter_mut()
            .find(|b| b.river_output_status.as_ref() == Some(output_status))
            .unwrap();
        bar.tags_info.urgent = urgent;
        bar.draw(&mut self.shared_state);
    }

    fn views_tags_updated(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        tags: Vec<u32>,
    ) {
        let bar = self
            .bars
            .iter_mut()
            .find(|b| b.river_output_status.as_ref() == Some(output_status))
            .unwrap();
        bar.tags_info.active = tags.into_iter().fold(0, |a, b| a | b);
        bar.draw(&mut self.shared_state);
    }

    fn layout_name_updated(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output_status: &RiverOutputStatus,
        layout_name: Option<String>,
    ) {
        let bar = self
            .bars
            .iter_mut()
            .find(|b| b.river_output_status.as_ref() == Some(output_status))
            .unwrap();
        bar.layout_name = layout_name;
        bar.draw(&mut self.shared_state);
    }
}

impl RiverControlHandler for State {
    fn river_control_state(&mut self) -> &mut RiverControlState {
        self.river_control_state.as_mut().unwrap()
    }

    fn command_failure(&mut self, _: &Connection, _: &QueueHandle<Self>, message: String) {
        self.set_error(format!("[river_control] {message}"));
    }

    fn command_success(&mut self, _: &Connection, _: &QueueHandle<Self>, message: String) {
        info!("river_control: {message}");
    }
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_pointer!(State);
delegate_layer!(State);
delegate_registry!(State);
delegate_river_status!(State);
delegate_river_control!(State);

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState,];
}

#[derive(Debug)]
pub struct ComputedBlock {
    pub block: Block,
    pub full: ComputedText,
    pub short: Option<ComputedText>,
    pub min_width: Option<f64>,
}
