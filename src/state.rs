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
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::layer::{Anchor, Layer, LayerHandler, LayerState, LayerSurface, LayerSurfaceConfigure},
    shm::{ShmHandler, ShmState},
};
use tokio::io::unix::AsyncFdReadyGuard;

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
    layer_state: LayerState,
    river_status_state: RiverStatusState,
    river_control_state: RiverControlState,

    seats: Vec<Seat>,
    bars: Vec<Bar>,
    pub shared_state: SharedState,
}

struct Seat {
    seat: wl_seat::WlSeat,
    pointer: wl_pointer::WlPointer,
    finger_scroll: f64,
}

impl State {
    pub fn new(conn: &Connection, event_queue: &mut EventQueue<Self>) -> Self {
        let mut error = Ok(());

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
            registry_state: RegistryState::new(conn, &event_queue.handle()),
            seat_state: SeatState::new(),
            output_state: OutputState::new(),
            compositor_state: CompositorState::new(),
            layer_state: LayerState::new(),
            river_status_state: RiverStatusState::new(),
            river_control_state: RiverControlState::new(),

            seats: Vec::new(),
            bars: Vec::new(),
            shared_state: SharedState {
                qh: event_queue.handle(),
                shm_state: ShmState::new(),
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
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

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
        let river_output_status = self.river_status_state.new_output_status(qh, &output).ok();
        self.bars.push(Bar {
            configured: false,
            width: 0,
            height,
            scale: 1,
            layer,
            blocks_btns: Default::default(),
            river_output_status,
            river_control: self.river_control_state.clone(),
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

impl LayerHandler for State {
    fn layer_state(&mut self) -> &mut LayerState {
        &mut self.layer_state
    }

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
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");
            self.seats.push(Seat {
                seat,
                pointer,
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
        _conn: &Connection,
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
                        // TODO: set_cursor()
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
        &mut self.river_status_state
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
}

impl RiverControlHandler for State {
    fn river_control_state(&mut self) -> &mut RiverControlState {
        &mut self.river_control_state
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
    registry_handlers![
        CompositorState,
        OutputState,
        ShmState,
        SeatState,
        LayerState,
        RiverStatusState,
        RiverControlState,
    ];
}

#[derive(Debug)]
pub struct ComputedBlock {
    pub block: Block,
    pub full: ComputedText,
    pub short: Option<ComputedText>,
    pub min_width: Option<f64>,
}
