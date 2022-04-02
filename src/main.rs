#[macro_use]
extern crate log;

use smithay_client_toolkit::{
    self as sctk, default_environment,
    environment::SimpleGlobal,
    new_default_environment,
    output::{with_output_info, OutputInfo},
    reexports::{
        calloop,
        client::protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
        client::{Attached, Main},
        protocols::wlr::unstable::layer_shell::v1::client::{
            zwlr_layer_shell_v1, zwlr_layer_surface_v1,
        },
    },
    shm::AutoMemPool,
    WaylandSource,
};

use pangocairo::cairo;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod button_manager;
mod color;
mod config;
mod i3bar_protocol;
mod lines_buffer;
mod pointer_btn;
mod river_protocols;
mod status_cmd;
mod tags;
mod text;

use button_manager::ButtonManager;
use config::Config;
use i3bar_protocol::{Block, MinWidth};
use pointer_btn::PointerBtn;
use river_protocols::zriver_command_callback_v1;
use river_protocols::zriver_control_v1;
use river_protocols::zriver_output_status_v1;
use river_protocols::zriver_status_manager_v1;
use status_cmd::StatusCmd;
use tags::{compute_tag_label, TagState, TagsInfo};
use text::{ComputedText, RenderOptions};

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

    let (env, display, queue) = new_default_environment!(
        Env,
        fields = [
            layer_shell: SimpleGlobal::new(),
            river_status_manager: SimpleGlobal::new(),
            river_control: SimpleGlobal::new(),
        ]
    )
    .expect("Initial roundtrip failed!");

    let config = Rc::new(RefCell::new(Config::new().unwrap()));
    let surfaces = Rc::new(RefCell::new(Vec::<Surface>::new()));
    let blocks = Rc::new(RefCell::new(Vec::<Block>::new()));

    let cmd = config.borrow_mut().command.take().unwrap();
    let status_cmd = StatusCmd::new(&cmd, Rc::clone(&blocks), Rc::clone(&surfaces))
        .expect("failed run status command");

    let layer_shell = env.require_global::<zwlr_layer_shell_v1::ZwlrLayerShellV1>();
    let river_status = env.require_global::<zriver_status_manager_v1::ZriverStatusManagerV1>();
    let river_control = env.require_global::<zriver_control_v1::ZriverControlV1>();

    let env_handle = env.clone();
    let surfaces_handle = Rc::clone(&surfaces);
    let config_handle = Rc::clone(&config);
    let blocks_handle = Rc::clone(&blocks);
    let status_cmd_handle = status_cmd.clone();
    let output_handler = move |output: wl_output::WlOutput, info: &OutputInfo| {
        if info.obsolete {
            info!("Output removed");
            surfaces_handle
                .borrow_mut()
                .retain(|s| s.output_id != info.id);
            output.release();
        } else {
            info!("Output detected");
            let surface = env_handle.create_surface().detach();
            let pool = env_handle
                .create_auto_pool()
                .expect("Failed to create a memory pool!");
            (*surfaces_handle.borrow_mut()).push(Surface::new(
                &output,
                info.id,
                surface,
                &layer_shell,
                &river_status,
                river_control.clone(),
                pool,
                config_handle.clone(),
                blocks_handle.clone(),
                status_cmd_handle.clone(),
            ));
        }
    };

    // Process currently existing outputs
    for output in env.get_all_outputs() {
        if let Some(info) = with_output_info(&output, Clone::clone) {
            output_handler(output, &info);
        }
    }

    // Setup a listener for changes
    // The listener will live for as long as we keep this handle alive
    let _listner_handle =
        env.listen_for_outputs(move |output, info, _| output_handler(output, info));

    // Right now river only supports one seat: default
    let pointer_output_id = Rc::new(Cell::new(None));
    for seat in env.get_all_seats() {
        sctk::seat::with_seat_data(&seat, |seat_data| {
            if seat_data.has_pointer && !seat_data.defunct && seat_data.name == "default" {
                let pointer = seat.get_pointer();
                let surfaces_handle = Rc::clone(&surfaces);
                let pointer_output_id_handle = Rc::clone(&pointer_output_id);
                let seat = seat.clone();
                pointer.quick_assign(move |_, event, _| {
                    info!("pointer event");
                    match event {
                        wl_pointer::Event::Enter {
                            serial: _,
                            surface,
                            surface_x: y,
                            surface_y: x,
                        } => {
                            if let Some(surf) = surfaces_handle
                                .borrow_mut()
                                .iter_mut()
                                .find(|s| s.surface == surface)
                            {
                                surf.pointer = Some((x, y));
                                pointer_output_id_handle.set(Some(surf.output_id));
                            }
                        }
                        wl_pointer::Event::Leave { serial: _, surface } => {
                            if let Some(surf) = surfaces_handle
                                .borrow_mut()
                                .iter_mut()
                                .find(|s| s.surface == surface)
                            {
                                surf.pointer = None;
                                pointer_output_id_handle.set(None);
                            }
                        }
                        wl_pointer::Event::Motion {
                            time: _,
                            surface_x: x,
                            surface_y: y,
                        } => {
                            if let Some(output_id) = pointer_output_id_handle.get() {
                                if let Some(surf) = surfaces_handle
                                    .borrow_mut()
                                    .iter_mut()
                                    .find(|s| s.output_id == output_id)
                                {
                                    surf.pointer = Some((x, y));
                                }
                            }
                        }
                        wl_pointer::Event::Button {
                            serial: _,
                            time: _,
                            button,
                            state,
                        } if state == wl_pointer::ButtonState::Pressed => {
                            if let Some(output_id) = pointer_output_id_handle.get() {
                                if let Some(surf) = surfaces_handle
                                    .borrow_mut()
                                    .iter_mut()
                                    .find(|s| s.output_id == output_id)
                                {
                                    surf.click(button.into(), &seat);
                                }
                            }
                        }
                        wl_pointer::Event::Axis {
                            time: _,
                            axis,
                            value,
                        } if axis == wl_pointer::Axis::VerticalScroll => {
                            if let Some(output_id) = pointer_output_id_handle.get() {
                                if let Some(surf) = surfaces_handle
                                    .borrow_mut()
                                    .iter_mut()
                                    .find(|s| s.output_id == output_id)
                                {
                                    surf.click(
                                        if value > 0.0 {
                                            PointerBtn::WheelDown
                                        } else {
                                            PointerBtn::WheelUp
                                        },
                                        &seat,
                                    );
                                }
                            }
                        }
                        _ => (),
                    }
                });
            }
        });
    }

    let mut event_loop = calloop::EventLoop::<()>::try_new().unwrap();

    WaylandSource::new(queue)
        .quick_insert(event_loop.handle())
        .unwrap();

    // Run command
    let blocks_handle = Rc::clone(&blocks);
    let surfaces_handle = Rc::clone(&surfaces);
    event_loop
        .handle()
        .insert_source(
            calloop::generic::Generic::new(
                status_cmd,
                calloop::Interest {
                    readable: true,
                    writable: false,
                },
                calloop::Mode::Edge,
            ),
            move |ready, cmd, _| {
                info!("status command update");
                if ready.readable {
                    cmd.notify_available()?;
                    Ok(calloop::PostAction::Continue)
                } else {
                    *blocks_handle.borrow_mut() = vec![Block {
                        full_text: "[error reading from status command]".into(),
                        ..Default::default()
                    }];
                    for s in &mut *surfaces_handle.borrow_mut() {
                        s.blocks_need_update = true;
                    }
                    Ok(calloop::PostAction::Remove)
                }
            },
        )
        .expect("failed to inser calloop source");

    loop {
        // This is ugly, let's hope that some version of drain_filter() gets stabilized soon
        // https://github.com/rust-lang/rust/issues/43244
        {
            let mut surfaces = surfaces.borrow_mut();
            let mut i = 0;
            while i != surfaces.len() {
                if surfaces[i].handle_events() {
                    surfaces.remove(i);
                } else {
                    i += 1;
                }
            }
        }

        display.flush().unwrap();
        event_loop.dispatch(None, &mut ()).unwrap();
    }
}

#[derive(PartialEq, Copy, Clone)]
enum RenderEvent {
    Configure { width: u32, height: u32 },
    TagsUpdated,
    Closed,
}

pub struct Surface {
    output_id: u32,
    surface: wl_surface::WlSurface,
    layer_surface: Main<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    next_render_event: Rc<Cell<Option<RenderEvent>>>,
    pool: AutoMemPool,
    dimensions: (u32, u32),
    config: Rc<RefCell<Config>>,
    // river stuff
    river_output_status: Main<zriver_output_status_v1::ZriverOutputStatusV1>,
    river_control: Attached<zriver_control_v1::ZriverControlV1>,
    // blocks
    blocks: Rc<RefCell<Vec<Block>>>,
    blocks_need_update: bool,
    status_cmd: StatusCmd,
    // tags
    tags_info: Rc<RefCell<TagsInfo>>,
    tags_computed: Vec<ComputedText>,
    // Clicking stuff
    pointer: Option<(f64, f64)>,
    tags_btns: ButtonManager,
    blocks_btns: ButtonManager<(Option<String>, Option<String>)>,
}

impl Surface {
    #[allow(clippy::too_many_arguments)]
    fn new(
        output: &wl_output::WlOutput,
        output_id: u32,
        surface: wl_surface::WlSurface,
        layer_shell: &Attached<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
        river_status: &Attached<zriver_status_manager_v1::ZriverStatusManagerV1>,
        river_control: Attached<zriver_control_v1::ZriverControlV1>,
        pool: AutoMemPool,
        config: Rc<RefCell<Config>>,
        blocks: Rc<RefCell<Vec<Block>>>,
        status_cmd: StatusCmd,
    ) -> Self {
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(output),
            zwlr_layer_shell_v1::Layer::Top,
            "i3bar-river".to_owned(),
        );

        // Set the height
        layer_surface.set_size(0, config.borrow().height);
        // Anchor to the top
        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );

        let next_render_event = Rc::new(Cell::new(None));
        let next_render_event_handle = Rc::clone(&next_render_event);
        layer_surface.quick_assign(move |layer_surface, event, _| {
            info!("layer_surface event");
            match (event, next_render_event_handle.get()) {
                (zwlr_layer_surface_v1::Event::Closed, _) => {
                    next_render_event_handle.set(Some(RenderEvent::Closed));
                }
                (
                    zwlr_layer_surface_v1::Event::Configure {
                        serial,
                        width,
                        height,
                    },
                    next,
                ) if next != Some(RenderEvent::Closed) => {
                    layer_surface.ack_configure(serial);
                    next_render_event_handle.set(Some(RenderEvent::Configure { width, height }));
                }
                (_, _) => (),
            }
        });

        // Commit so that the server will send a configure event
        surface.commit();

        let river_output_status = river_status.get_river_output_status(output);
        let tags_info = Rc::new(RefCell::new(TagsInfo::default()));
        let tags_info_handle = Rc::clone(&tags_info);
        let next_render_event_handle = Rc::clone(&next_render_event);
        river_output_status.quick_assign(move |_, event, _| {
            match event {
                zriver_output_status_v1::Event::FocusedTags { tags } => {
                    info!("Focused tags updated: {tags}");
                    tags_info_handle.borrow_mut().focused = tags;
                }
                zriver_output_status_v1::Event::UrgentTags { tags } => {
                    info!("Urgent tags updated: {tags}");
                    tags_info_handle.borrow_mut().urgent = tags;
                }
                _ => (),
            }
            if next_render_event_handle.get().is_none() {
                next_render_event_handle.set(Some(RenderEvent::TagsUpdated));
            }
        });

        Self {
            output_id,
            surface,
            layer_surface,
            next_render_event,
            pool,
            dimensions: (0, 0),
            config,
            river_output_status,
            river_control,
            blocks,
            blocks_need_update: false,
            status_cmd,
            tags_info,
            tags_computed: Vec::with_capacity(9),
            pointer: None,
            tags_btns: Default::default(),
            blocks_btns: Default::default(),
        }
    }

    /// Handles any events that have occurred since the last call, redrawing if needed.
    /// Returns true if the surface should be dropped.
    fn handle_events(&mut self) -> bool {
        match self.next_render_event.take() {
            Some(RenderEvent::Closed) => return true,
            Some(RenderEvent::Configure { width, height }) => {
                if self.dimensions != (width, height) {
                    self.dimensions = (width, height);
                    self.layer_surface.set_exclusive_zone(height as _);
                    self.draw();
                    return false;
                }
            }
            Some(RenderEvent::TagsUpdated) => {
                self.draw();
                return false;
            }
            _ => (),
        }
        if self.blocks_need_update {
            self.draw();
            return false;
        }
        false
    }

    fn click(&mut self, button: PointerBtn, seat: &wl_seat::WlSeat) {
        if let Some((x, _)) = self.pointer {
            if let Some(id) = self.tags_btns.click(x) {
                let cmd = match button {
                    PointerBtn::Left => "set-focused-tags",
                    PointerBtn::Right => "toggle-focused-tags",
                    _ => return,
                };
                self.river_control.add_argument(cmd.into());
                self.river_control.add_argument((1 << id).to_string());
                let result = self.river_control.run_command(seat);
                result.quick_assign(|_, event, _| match event {
                    zriver_command_callback_v1::Event::Success { output } => {
                        info!("River cmd output: '{output}'");
                    }
                    zriver_command_callback_v1::Event::Failure { failure_message: f } => {
                        error!("River error: '{f}'");
                    }
                });
            } else if let Some((name, instance)) = self.blocks_btns.click(x) {
                let _ =
                    self.status_cmd
                        .send_click_event(button, name.as_deref(), instance.as_deref());
            }
        }
    }

    fn draw(&mut self) {
        let config = self.config.borrow();

        let stride = 4 * self.dimensions.0 as i32;
        let width = self.dimensions.0 as i32;
        let height = self.dimensions.1 as i32;
        let width_f = width as f64;
        let height_f = height as f64;

        let (canvas, buffer) = self
            .pool
            .buffer(width, height, stride, wl_shm::Format::Argb8888)
            .unwrap();

        let cairo_surf = unsafe {
            cairo::ImageSurface::create_for_data_unsafe(
                canvas.as_mut_ptr(),
                cairo::Format::ARgb32,
                width,
                height,
                stride,
            )
            .expect("cairo surface")
        };

        let cairo_ctx = cairo::Context::new(&cairo_surf).expect("cairo context");
        let pango_layout = pangocairo::create_layout(&cairo_ctx).expect("pango layout");
        pango_layout.set_font_description(Some(&config.font));
        pango_layout.set_height(height);

        // Background
        config.background.apply(&cairo_ctx);
        cairo_ctx.paint().expect("cairo paint");

        // Compute tags
        if self.tags_computed.is_empty() {
            let mut x_offset = 0.0;
            //  TODO make configurable
            for (id, text) in ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
                .iter()
                .enumerate()
            {
                let tag = compute_tag_label(text.to_string(), config.font.clone(), &cairo_ctx);
                self.tags_btns.push(x_offset, tag.width, id);
                x_offset += tag.width;
                self.tags_computed.push(tag);
            }
        }

        // Display tags
        let mut offset_left = 0.0;
        let tags_info = self.tags_info.borrow();
        for (i, label) in self.tags_computed.iter().enumerate() {
            let (bg, fg) = match tags_info.get_state(i) {
                TagState::Focused => (config.tag_focused_bg, config.tag_focused_fg),
                TagState::Inactive => (config.tag_bg, config.tag_fg),
                TagState::Urgent => (config.tag_urgent_bg, config.tag_urgent_fg),
            };
            label.render(
                &cairo_ctx,
                RenderOptions {
                    x_offset: offset_left,
                    bar_height: height_f,
                    fg_color: fg,
                    bg_color: Some(bg),
                },
            );
            offset_left += label.width;
        }

        // Display the blocks
        // TODO: handle short_text
        let mut offset_right = 0.0;
        self.blocks_btns.clear();
        let mut first_block = true;
        for block in self.blocks.borrow().iter().rev() {
            if !first_block && block.separator && block.separator_block_width > 0 {
                let w = block.separator_block_width as f64;
                config.separator.apply(&cairo_ctx);
                cairo_ctx.set_line_width(2.0);
                cairo_ctx.move_to(width_f - offset_right - w * 0.5, height_f * 0.1);
                cairo_ctx.line_to(width_f - offset_right - w * 0.5, height_f * 0.9);
                cairo_ctx.stroke().unwrap();
                offset_right += w;
            }
            let markup = block.markup.as_deref() == Some("pango");
            let text = text::Text {
                text: block.full_text.clone(),
                attr: text::Attributes {
                    font: config.font.clone(),
                    padding_left: 0.0,
                    padding_right: 0.0,
                    min_width: match &block.min_width {
                        Some(MinWidth::Pixels(p)) => Some(*p as f64),
                        Some(MinWidth::Text(t)) => {
                            Some(text::width_of(t, &cairo_ctx, markup, &config.font.0))
                        }
                        None => None,
                    },
                    align: block.align.unwrap_or_default(),
                    markup,
                },
            };
            let comp = text.compute(&cairo_ctx);
            comp.render(
                &cairo_ctx,
                RenderOptions {
                    x_offset: width_f - comp.width - offset_right,
                    bar_height: height_f,
                    fg_color: block.color.unwrap_or(config.color),
                    bg_color: block.background,
                },
            );
            offset_right += comp.width;
            self.blocks_btns.push(
                width_f - offset_right,
                comp.width,
                (block.name.clone(), block.instance.clone()),
            );
            first_block = false;
        }
        self.blocks_need_update = false;

        // Attach the buffer to the surface and mark the entire surface as damaged
        self.surface.attach(Some(&buffer), 0, 0);
        self.surface
            .damage_buffer(0, 0, width as i32, height as i32);

        // Finally, commit the surface
        self.surface.commit();
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.layer_surface.destroy();
        self.surface.destroy();
        self.river_output_status.destroy();
    }
}
