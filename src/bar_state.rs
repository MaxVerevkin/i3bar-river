use std::cell::{Cell, RefCell};
use std::collections::{BinaryHeap, HashMap};
use std::io;
use std::rc::Rc;

use smithay_client_toolkit::{
    reexports::{
        client::protocol::{wl_output::WlOutput, wl_seat::WlSeat, wl_shm, wl_surface::WlSurface},
        client::{Attached, Main},
        protocols::wlr::unstable::layer_shell::v1::client::{
            zwlr_layer_shell_v1, zwlr_layer_surface_v1,
        },
    },
    shm::AutoMemPool,
};

use pangocairo::cairo;

use crate::button_manager::ButtonManager;
use crate::config::Config;
use crate::i3bar_protocol::{self, Block, MinWidth};
use crate::ord_adaptor::DefaultLess;
use crate::pointer_btn::PointerBtn;
use crate::river_protocols::zriver_command_callback_v1;
use crate::river_protocols::zriver_control_v1;
use crate::river_protocols::zriver_output_status_v1;
use crate::river_protocols::zriver_status_manager_v1;
use crate::status_cmd::StatusCmd;
use crate::tags::{compute_tag_label, TagState, TagsInfo};
use crate::text;
use crate::text::{ComputedText, RenderOptions};

#[derive(PartialEq, Copy, Clone)]
enum RenderEvent {
    Configure { width: u32, height: u32 },
    TagsUpdated,
    Closed,
}

pub struct BarState {
    pub status_cmd: Option<StatusCmd>,
    config: Rc<RefCell<Config>>,
    is_error: bool,
    blocks: Vec<Block>,
    blocks_updated: bool,
    surfaces: Vec<Surface>,
}

impl BarState {
    pub fn new() -> Self {
        let mut error = Ok(());

        let config = Config::new()
            .map_err(|e| error = Err(e))
            .unwrap_or_default();

        let status_cmd = match &error {
            Err(_) => None,
            Ok(()) => config.command.as_deref().and_then(|cmd| {
                StatusCmd::new(cmd)
                    .map_err(|e| error = Err(anyhow!(e)))
                    .ok()
            }),
        };

        let mut s = Self {
            status_cmd,
            config: Rc::new(RefCell::new(config)),
            is_error: false,
            blocks: Vec::new(),
            blocks_updated: false,
            surfaces: Default::default(),
        };

        if let Err(e) = error {
            s.set_error(e.to_string());
        }

        s
    }

    pub fn set_blocks(&mut self, blocks: Vec<Block>) {
        self.blocks = blocks;
        self.blocks_updated = true;
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.is_error = true;
        self.set_blocks(vec![Block {
            full_text: error.into(),
            ..Default::default()
        }]);
    }

    pub fn remove_surface(&mut self, output_id: u32) {
        self.surfaces.retain(|s| s.output_id != output_id);
    }

    pub fn notify_available(&mut self) -> io::Result<()> {
        if let Some(cmd) = &mut self.status_cmd {
            if let Some(blocks) = cmd.notify_available()? {
                self.set_blocks(blocks);
            }
        }
        Ok(())
    }

    pub fn handle_click(
        &mut self,
        surface: &WlSurface,
        seat: &WlSeat,
        x: f64,
        y: f64,
        btn: PointerBtn,
    ) {
        if let Some(s) = self.surfaces.iter().find(|s| &s.surface == surface) {
            if let Some(event) = s.click(btn, seat, x, y) {
                if let Some(cmd) = &mut self.status_cmd {
                    if let Err(e) = cmd.send_click_event(&event) {
                        self.set_error(e.to_string());
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_surface(
        &mut self,
        output: &WlOutput,
        output_id: u32,
        surface: WlSurface,
        layer_shell: &Attached<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
        river_status: &Attached<zriver_status_manager_v1::ZriverStatusManagerV1>,
        river_control: Attached<zriver_control_v1::ZriverControlV1>,
        pool: AutoMemPool,
    ) {
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(output),
            zwlr_layer_shell_v1::Layer::Top,
            "i3bar-river".to_owned(),
        );

        // Set the height
        layer_surface.set_size(0, self.config.borrow().height);
        // Anchor to the top
        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );

        let next_render_event = Rc::new(Cell::new(None));
        let next_render_event_handle = Rc::clone(&next_render_event);
        layer_surface.quick_assign(move |layer_surface, event, _| {
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
                    tags_info_handle.borrow_mut().focused = tags;
                }
                zriver_output_status_v1::Event::UrgentTags { tags } => {
                    tags_info_handle.borrow_mut().urgent = tags;
                }
                _ => (),
            }
            if next_render_event_handle.get().is_none() {
                next_render_event_handle.set(Some(RenderEvent::TagsUpdated));
            }
        });

        self.surfaces.push(Surface {
            output_id,
            surface,
            layer_surface,
            next_render_event,
            pool,
            dimensions: (0, 0),
            config: self.config.clone(),
            river_output_status,
            river_control,
            tags_info,
            tags_computed: Vec::with_capacity(9),
            tags_btns: Default::default(),
            blocks_btns: Default::default(),
        });
    }

    pub fn handle_events(&mut self) {
        // This is ugly, let's hope that some version of drain_filter() gets stabilized soon
        // https://github.com/rust-lang/rust/issues/43244
        let mut i = 0;
        while i != self.surfaces.len() {
            if self.surfaces[i].handle_events(&self.blocks, self.blocks_updated) {
                self.surfaces.remove(i);
            } else {
                i += 1;
            }
        }
        self.blocks_updated = false;
    }
}

pub struct Surface {
    output_id: u32,
    surface: WlSurface,
    layer_surface: Main<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    next_render_event: Rc<Cell<Option<RenderEvent>>>,
    pool: AutoMemPool,
    dimensions: (u32, u32),
    config: Rc<RefCell<Config>>,
    // river stuff
    river_output_status: Main<zriver_output_status_v1::ZriverOutputStatusV1>,
    river_control: Attached<zriver_control_v1::ZriverControlV1>,
    // tags
    tags_info: Rc<RefCell<TagsInfo>>,
    tags_computed: Vec<ComputedText>,
    // buttons
    tags_btns: ButtonManager,
    blocks_btns: ButtonManager<(Option<String>, Option<String>)>,
}

impl Surface {
    /// Handles any events that have occurred since the last call, redrawing if needed.
    /// Returns true if the surface should be dropped.
    fn handle_events(&mut self, blocks: &[Block], blocks_updated: bool) -> bool {
        match self.next_render_event.take() {
            Some(RenderEvent::Closed) => return true,
            Some(RenderEvent::Configure { width, height }) => {
                if self.dimensions != (width, height) {
                    self.dimensions = (width, height);
                    self.layer_surface.set_exclusive_zone(height as _);
                    self.draw(blocks);
                    return false;
                }
            }
            Some(RenderEvent::TagsUpdated) => {
                self.draw(blocks);
                return false;
            }
            _ => (),
        }
        if blocks_updated {
            self.draw(blocks);
            return false;
        }
        false
    }

    fn click(
        &self,
        button: PointerBtn,
        seat: &WlSeat,
        x: f64,
        _y: f64,
    ) -> Option<i3bar_protocol::Event> {
        if let Some(id) = self.tags_btns.click(x) {
            let cmd = match button {
                PointerBtn::Left => "set-focused-tags",
                PointerBtn::Right => "toggle-focused-tags",
                _ => return None,
            };
            self.river_control.add_argument(cmd.into());
            self.river_control.add_argument((1u32 << id).to_string());
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
            return Some(i3bar_protocol::Event {
                name: name.as_deref(),
                instance: instance.as_deref(),
                button,
                ..Default::default()
            });
        }
        None
    }

    fn draw(&mut self, blocks: &[Block]) {
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
        render_blocks(
            &cairo_ctx,
            &*config,
            blocks,
            &mut self.blocks_btns,
            offset_left,
            width_f,
            height_f,
        );

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

fn render_blocks(
    context: &cairo::Context,
    config: &Config,
    blocks: &[Block],
    buttons: &mut ButtonManager<(Option<String>, Option<String>)>,
    offset_left: f64,
    full_width: f64,
    full_height: f64,
) {
    let mut blocks_computed = Vec::with_capacity(blocks.len());
    let mut blocks_width = 0.0;
    let mut deltas = HashMap::<&str, f64>::new();
    for (i, block) in blocks.iter().enumerate() {
        let markup = block.markup.as_deref() == Some("pango");
        let min_width = match &block.min_width {
            Some(MinWidth::Pixels(p)) => Some(*p as f64),
            Some(MinWidth::Text(t)) => Some(text::width_of(t, context, markup, &config.font.0)),
            None => None,
        };
        let full_text = text::Text {
            text: block.full_text.clone(),
            attr: text::Attributes {
                font: config.font.clone(),
                padding_left: 0.0,
                padding_right: 0.0,
                min_width,
                align: block.align.unwrap_or_default(),
                markup,
            },
        }
        .compute(context);
        let short_text = block.short_text.clone().map(|short_text| {
            text::Text {
                text: short_text,
                attr: text::Attributes {
                    font: config.font.clone(),
                    padding_left: 0.0,
                    padding_right: 0.0,
                    min_width,
                    align: block.align.unwrap_or_default(),
                    markup,
                },
            }
            .compute(context)
        });
        blocks_width += full_text.width;
        if i + 1 != blocks.len() {
            blocks_width += block.separator_block_width as f64;
        }
        if let Some(short) = &short_text {
            if let Some(name) = &block.name {
                *deltas.entry(name).or_insert(0.0) += full_text.width - short.width;
            }
        }
        blocks_computed.push((block.name.as_deref(), full_text, short_text));
    }

    // Progressively switch to short mode
    if offset_left + blocks_width > full_width {
        let mut heap = BinaryHeap::new();
        for (name, delta) in deltas {
            if delta > 0.0 {
                heap.push((DefaultLess(delta), name));
            }
        }
        while let Some((DefaultLess(delta), to_switch)) = heap.pop() {
            for (name, full, short) in &mut blocks_computed {
                if *name == Some(to_switch) {
                    if let Some(short) = short {
                        std::mem::swap(full, short);
                    }
                }
            }
            blocks_width -= delta;
            if offset_left + blocks_width <= full_width {
                break;
            }
        }
    }

    buttons.clear();
    for (i, (block, computed)) in blocks
        .iter()
        .zip(blocks_computed.iter().map(|(_, full, _)| full))
        .enumerate()
    {
        computed.render(
            context,
            RenderOptions {
                x_offset: full_width - blocks_width,
                bar_height: full_height,
                fg_color: block.color.unwrap_or(config.color),
                bg_color: block.background,
            },
        );
        buttons.push(
            full_width - blocks_width,
            computed.width,
            (block.name.clone(), block.instance.clone()),
        );
        blocks_width -= computed.width;
        if i + 1 != blocks.len() && block.separator_block_width > 0 {
            let w = block.separator_block_width as f64;
            if block.separator {
                config.separator.apply(context);
                context.set_line_width(2.0);
                context.move_to(full_width - blocks_width + w * 0.5, full_height * 0.1);
                context.line_to(full_width - blocks_width + w * 0.5, full_height * 0.9);
                context.stroke().unwrap();
            }
            blocks_width -= w;
        }
    }
}
