#![allow(clippy::explicit_auto_deref)] // TODO: remove once https://github.com/rust-lang/rust-clippy/issues/9101 is resloved

#[macro_use]
extern crate log;
#[macro_use]
extern crate anyhow;

use smithay_client_toolkit::{
    self as sctk, default_environment,
    environment::SimpleGlobal,
    new_default_environment,
    output::{with_output_info, OutputInfo},
    reexports::{
        calloop,
        client::protocol::{wl_output, wl_pointer},
        protocols::wlr::unstable::layer_shell::v1::client::zwlr_layer_shell_v1,
    },
    seat::pointer,
    WaylandSource,
};

mod bar_state;
mod button_manager;
mod color;
mod config;
mod i3bar_protocol;
mod ord_adaptor;
mod pointer_btn;
mod river_protocols;
mod status_cmd;
mod tags;
mod text;
mod utils;

use bar_state::BarState;
use pointer_btn::PointerBtn;
use river_protocols::zriver_control_v1;
use river_protocols::zriver_status_manager_v1;

default_environment!(Env,
    fields = [
        layer_shell: SimpleGlobal<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
        river_status_manager: SimpleGlobal<zriver_status_manager_v1::ZriverStatusManagerV1>,
        river_control: SimpleGlobal<zriver_control_v1::ZriverControlV1>,
    ],
    singles = [
        zwlr_layer_shell_v1::ZwlrLayerShellV1 => layer_shell,
        zriver_status_manager_v1::ZriverStatusManagerV1 => river_status_manager,
        zriver_control_v1::ZriverControlV1 => river_control,
    ],
);

fn main() {
    env_logger::init();

    let (env, _display, queue) = new_default_environment!(
        Env,
        fields = [
            layer_shell: SimpleGlobal::new(),
            river_status_manager: SimpleGlobal::new(),
            river_control: SimpleGlobal::new(),
        ]
    )
    .expect("Initial roundtrip failed!");

    let mut bar_state = BarState::new(&env);

    let env_handle = env.clone();
    let output_handler =
        move |output: wl_output::WlOutput, info: &OutputInfo, bar_state: &mut BarState| {
            if info.obsolete {
                info!("Output removed");
                bar_state.remove_surface(info.id);
                output.release();
            } else {
                info!("Output detected");
                let surface = env_handle.create_surface();
                let pool = env_handle
                    .create_auto_pool()
                    .expect("Failed to create a memory pool!");
                bar_state.add_surface(&output, info.id, surface, pool);
            }
        };

    // Process currently existing outputs
    for output in env.get_all_outputs() {
        if let Some(info) = with_output_info(&output, Clone::clone) {
            output_handler(output, &info, &mut bar_state);
        }
    }

    // Setup a listener for changes
    // The listener will live for as long as we keep this handle alive
    let _listner_handle = env.listen_for_outputs(move |output, info, mut data| {
        output_handler(output, info, data.get().unwrap())
    });

    // Right now river only supports one seat
    let cursor_theme = pointer::ThemeManager::init(
        pointer::ThemeSpec::System,
        env.require_global(),
        env.require_global(),
    );
    for seat in env.get_all_seats() {
        sctk::seat::with_seat_data(&seat, |seat_data| {
            if seat_data.has_pointer && !seat_data.defunct {
                let pointer = seat.get_pointer();
                let themed_pointer = cursor_theme.theme_pointer(pointer.detach());
                let seat = seat.clone();
                let mut pos = (0.0, 0.0);
                let mut cur_surface = None;
                pointer.quick_assign(move |_, event, mut data| {
                    let bar_state: &mut BarState = data.get().unwrap();
                    match event {
                        wl_pointer::Event::Enter {
                            serial,
                            surface,
                            surface_x: y,
                            surface_y: x,
                        } => {
                            let _ = themed_pointer.set_cursor("default", Some(serial));
                            cur_surface = Some(surface);
                            pos = (x, y);
                        }
                        wl_pointer::Event::Leave {
                            serial: _,
                            surface: _,
                        } => {
                            cur_surface = None;
                        }
                        wl_pointer::Event::Motion {
                            time: _,
                            surface_x: x,
                            surface_y: y,
                        } => {
                            pos = (x, y);
                        }
                        wl_pointer::Event::Button {
                            serial: _,
                            time: _,
                            button,
                            state,
                        } if state == wl_pointer::ButtonState::Pressed => {
                            if let Some(cur_surface) = &cur_surface {
                                bar_state.handle_click(
                                    cur_surface,
                                    &seat,
                                    pos.0,
                                    pos.1,
                                    button.into(),
                                );
                            }
                        }
                        wl_pointer::Event::Axis {
                            time: _,
                            axis,
                            value,
                        } if axis == wl_pointer::Axis::VerticalScroll => {
                            if let Some(cur_surface) = &cur_surface {
                                bar_state.handle_click(
                                    cur_surface,
                                    &seat,
                                    pos.0,
                                    pos.1,
                                    if value > 0.0 {
                                        PointerBtn::WheelDown
                                    } else {
                                        PointerBtn::WheelUp
                                    },
                                );
                            }
                        }
                        _ => (),
                    }
                });
            }
        });
    }

    let mut event_loop = calloop::EventLoop::<BarState>::try_new().unwrap();

    WaylandSource::new(queue)
        .quick_insert(event_loop.handle())
        .unwrap();

    if let Some(cmd) = &bar_state.status_cmd {
        cmd.quick_insert(event_loop.handle());
    }

    loop {
        if let Err(e) = event_loop.dispatch(None, &mut bar_state) {
            bar_state.set_error(e.to_string());
        }
    }
}
