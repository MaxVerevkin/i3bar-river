use pangocairo::cairo;

use wayrs_client::{Connection, EventCtx};
use wayrs_utils::shm_alloc::BufferSpec;

use crate::blocks_cache::ComputedBlock;
use crate::button_manager::ButtonManager;
use crate::color::Color;
use crate::config::{Config, Position};
use crate::i3bar_protocol;
use crate::output::Output;
use crate::pointer_btn::PointerBtn;
use crate::protocol::*;
use crate::shared_state::SharedState;
use crate::state::State;
use crate::text::{self, ComputedText, RenderOptions};
use crate::wm_info_provider::Tag;

pub struct Bar {
    pub output: Output,
    hidden: bool,
    mapped: bool,
    throttle: Option<WlCallback>,
    throttled: bool,
    width: u32,
    height: u32,
    scale120: Option<u32>,
    pub surface: WlSurface,
    layer_surface: ZwlrLayerSurfaceV1,
    viewport: WpViewport,
    fractional_scale: Option<WpFractionalScaleV1>,
    blocks_btns: ButtonManager<(Option<String>, Option<String>)>,
    tags: Vec<Tag>,
    layout_name: Option<String>,
    mode_name: Option<String>,
    tags_btns: ButtonManager<u32>,
    tags_computed: Vec<(u32, ColorPair, ComputedText)>,
    layout_name_computed: Option<ComputedText>,
    mode_computed: Option<ComputedText>,
}

#[derive(Debug, PartialEq)]
pub struct ColorPair {
    bg: Color,
    fg: Color,
}

impl Bar {
    pub fn new(conn: &mut Connection<State>, state: &State, output: Output) -> Self {
        let surface = state.wl_compositor.create_surface(conn);

        let fractional_scale = state
            .fractional_scale_manager
            .map(|mgr| mgr.get_fractional_scale_with_cb(conn, surface, fractional_scale_cb));

        let layer_surface = state.layer_shell.get_layer_surface_with_cb(
            conn,
            surface,
            Some(output.wl),
            state.shared_state.config.layer.into(),
            c"i3bar-river".into(),
            layer_surface_cb,
        );

        Self {
            output,
            hidden: true,
            mapped: false,
            throttle: None,
            throttled: false,
            width: 0,
            height: state.shared_state.config.height,
            scale120: None,
            surface,
            viewport: state.viewporter.get_viewport(conn, surface),
            fractional_scale,
            layer_surface,
            blocks_btns: Default::default(),
            tags: Vec::new(),
            layout_name: None,
            mode_name: None,
            tags_btns: Default::default(),
            tags_computed: Vec::new(),
            layout_name_computed: None,
            mode_computed: None,
        }
    }

    pub fn destroy(self, conn: &mut Connection<State>) {
        self.layer_surface.destroy(conn);
        self.viewport.destroy(conn);
        if let Some(fs) = self.fractional_scale {
            fs.destroy(conn);
        }
        self.surface.destroy(conn);
        self.output.destroy(conn);
    }

    pub fn set_tags(&mut self, tags: Vec<Tag>) {
        self.tags = tags;
        self.tags_btns.clear();
        self.tags_computed.clear();
    }

    pub fn set_layout_name(&mut self, layout_name: Option<String>) {
        self.layout_name = layout_name;
        self.layout_name_computed = None;
    }

    pub fn set_mode_name(&mut self, mode_name: Option<String>) {
        self.mode_name = mode_name;
        self.mode_computed = None;
    }

    pub fn click(
        &mut self,
        conn: &mut Connection<State>,
        ss: &mut SharedState,
        button: PointerBtn,
        seat: WlSeat,
        x: f64,
        _y: f64,
    ) -> anyhow::Result<()> {
        if let Some(tag_id) = self.tags_btns.click(x) {
            ss.wm_info_provider
                .click_on_tag(conn, &self.output, seat, Some(*tag_id), button);
        } else if self.tags_btns.is_between(x) {
            ss.wm_info_provider
                .click_on_tag(conn, &self.output, seat, None, button);
        } else if let Some((name, instance)) = self.blocks_btns.click(x) {
            if let Some(cmd) = &mut ss.status_cmd {
                cmd.send_click_event(&i3bar_protocol::Event {
                    name: name.as_deref(),
                    instance: instance.as_deref(),
                    button,
                    ..Default::default()
                })?;
            }
        }
        Ok(())
    }

    pub fn frame(&mut self, conn: &mut Connection<State>, ss: &mut SharedState) {
        if !self.mapped {
            return;
        }

        if self.throttle.is_some() {
            self.throttled = true;
            return;
        }

        let (pix_width, pix_height, scale_f) = match self.scale120 {
            Some(scale120) => (
                // rounding halfway away from zero
                (self.width * scale120 + 60) / 120,
                (self.height * scale120 + 60) / 120,
                scale120 as f64 / 120.0,
            ),
            None => (
                self.width * self.output.scale,
                self.height * self.output.scale,
                self.output.scale as f64,
            ),
        };

        let width_f = self.width as f64;
        let height_f = self.height as f64;

        let (buffer, canvas) = ss
            .shm
            .alloc_buffer(
                conn,
                BufferSpec {
                    width: pix_width,
                    height: pix_height,
                    stride: pix_width * 4,
                    format: wl_shm::Format::Argb8888,
                },
            )
            .unwrap();

        let cairo_surf = unsafe {
            cairo::ImageSurface::create_for_data_unsafe(
                canvas.as_mut_ptr(),
                cairo::Format::ARgb32,
                pix_width as i32,
                pix_height as i32,
                pix_width as i32 * 4,
            )
            .expect("cairo surface")
        };

        let cairo_ctx = cairo::Context::new(&cairo_surf).expect("cairo context");
        cairo_ctx.scale(scale_f, scale_f);

        if !ss.config.blend {
            cairo_ctx.set_operator(cairo::Operator::Source);
        }

        // Background
        if ss.config.blend {
            cairo_ctx.save().unwrap();
            cairo_ctx.set_operator(cairo::Operator::Source);
        }
        ss.config.background.apply(&cairo_ctx);
        cairo_ctx.paint().unwrap();
        if ss.config.blend {
            cairo_ctx.restore().unwrap();
        }

        // Compute tags
        if ss.config.show_tags && self.tags_computed.is_empty() {
            for tag in &self.tags {
                let (bg, fg) = if tag.is_urgent {
                    (ss.config.tag_urgent_bg, ss.config.tag_urgent_fg)
                } else if tag.is_focused {
                    (ss.config.tag_focused_bg, ss.config.tag_focused_fg)
                } else if tag.is_active {
                    (ss.config.tag_bg, ss.config.tag_fg)
                } else if !ss.config.hide_inactive_tags {
                    (ss.config.tag_inactive_bg, ss.config.tag_inactive_fg)
                } else {
                    continue;
                };
                let comp = compute_tag_label(&tag.name, &ss.config);
                self.tags_computed
                    .push((tag.id, ColorPair { bg, fg }, comp));
            }
        }

        // Display tags
        let mut offset_left = 0.0;
        self.tags_btns.clear();
        for (i, (id, color, computed)) in self.tags_computed.iter().enumerate() {
            let left_joined = i != 0 && self.tags_computed[i - 1].1 == *color;
            let right_joined =
                i + 1 != self.tags_computed.len() && self.tags_computed[i + 1].1 == *color;
            if i != 0 && !left_joined {
                offset_left += ss.config.tags_margin;
            }
            computed.render(
                &cairo_ctx,
                RenderOptions {
                    x_offset: offset_left,
                    bar_height: height_f,
                    fg_color: color.fg,
                    bg_color: Some(color.bg),
                    r_left: if left_joined { 0.0 } else { ss.config.tags_r },
                    r_right: if right_joined { 0.0 } else { ss.config.tags_r },
                    overlap: 0.0,
                },
            );
            self.tags_btns.push(offset_left, computed.width, *id);
            offset_left += computed.width;
        }

        // Display layout name
        if ss.config.show_layout_name {
            if let Some(layout_name) = &self.layout_name {
                let text = self.layout_name_computed.get_or_insert_with(|| {
                    ComputedText::new(
                        layout_name,
                        text::Attributes {
                            font: &ss.config.font,
                            padding_left: 25.0,
                            padding_right: 25.0,
                            min_width: None,
                            align: Default::default(),
                            markup: false,
                        },
                    )
                });
                text.render(
                    &cairo_ctx,
                    RenderOptions {
                        x_offset: offset_left,
                        bar_height: height_f,
                        fg_color: ss.config.tag_inactive_fg,
                        bg_color: None,
                        r_left: 0.0,
                        r_right: 0.0,
                        overlap: 0.0,
                    },
                );
                offset_left += text.width;
            }
        }

        // Display mode
        if ss.config.show_mode {
            if let Some(mode) = &self.mode_name {
                let text = self.mode_computed.get_or_insert_with(|| {
                    ComputedText::new(
                        mode,
                        text::Attributes {
                            font: &ss.config.font,
                            padding_left: 10.0,
                            padding_right: 10.0,
                            min_width: None,
                            align: Default::default(),
                            markup: false,
                        },
                    )
                });
                text.render(
                    &cairo_ctx,
                    RenderOptions {
                        x_offset: offset_left,
                        bar_height: height_f,
                        fg_color: ss.config.tag_urgent_fg,
                        bg_color: Some(ss.config.tag_urgent_bg),
                        r_left: ss.config.tags_r,
                        r_right: ss.config.tags_r,
                        overlap: 0.0,
                    },
                );
                offset_left += text.width;
            }
        }

        // Display the blocks
        render_blocks(
            &cairo_ctx,
            &ss.config,
            ss.blocks_cache.get_computed(),
            &mut self.blocks_btns,
            offset_left,
            width_f,
            height_f,
        );

        self.viewport
            .set_destination(conn, self.width as i32, self.height as i32);

        self.surface
            .attach(conn, Some(buffer.into_wl_buffer()), 0, 0);
        self.surface.damage(conn, 0, 0, i32::MAX, i32::MAX);

        self.throttle = Some(self.surface.frame_with_cb(conn, |ctx| {
            if let Some(bar) = ctx
                .state
                .bars
                .iter_mut()
                .find(|bar| bar.throttle == Some(ctx.proxy))
            {
                bar.throttle = None;
                if bar.throttled {
                    bar.throttled = false;
                    bar.frame(ctx.conn, &mut ctx.state.shared_state);
                }
            }
        }));

        self.surface.commit(conn);
    }

    pub fn show(&mut self, conn: &mut Connection<State>, shared_state: &SharedState) {
        assert!(!self.mapped);

        self.hidden = false;

        let config = &shared_state.config;

        self.layer_surface.set_size(conn, 0, config.height);
        self.layer_surface.set_anchor(conn, config.position.into());
        self.layer_surface.set_margin(
            conn,
            config.margin_top,
            config.margin_right,
            config.margin_bottom,
            config.margin_left,
        );
        self.layer_surface.set_exclusive_zone(
            conn,
            (shared_state.config.height) as i32
                + if config.position == Position::Top {
                    shared_state.config.margin_bottom
                } else {
                    shared_state.config.margin_top
                },
        );

        self.surface.commit(conn);
    }

    pub fn hide(&mut self, conn: &mut Connection<State>) {
        self.hidden = true;
        self.mapped = false;
        self.surface.attach(conn, None, 0, 0);
        self.surface.commit(conn);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_blocks(
    context: &cairo::Context,
    config: &Config,
    blocks: &[ComputedBlock],
    buttons: &mut ButtonManager<(Option<String>, Option<String>)>,
    offset_left: f64,
    full_width: f64,
    full_height: f64,
) {
    context.rectangle(offset_left, 0.0, full_width - offset_left, full_height);
    context.clip();

    struct LogialBlock<'a> {
        blocks: Vec<&'a ComputedBlock>,
        delta: f64,
        switched_to_short: bool,
        separator: bool,
        separator_block_width: u8,
    }

    let mut blocks_computed = Vec::new();
    let mut blocks_width = 0.0;
    let mut s_start = 0;
    while s_start < blocks.len() {
        let mut s_end = s_start + 1;
        let series_name = &blocks[s_start].block.name;
        while s_end < blocks.len()
            && blocks[s_end - 1].block.separator_block_width == 0
            && &blocks[s_end].block.name == series_name
        {
            s_end += 1;
        }

        let mut series = LogialBlock {
            blocks: Vec::with_capacity(s_end - s_start),
            delta: 0.0,
            switched_to_short: false,
            separator: blocks[s_end - 1].block.separator,
            separator_block_width: blocks[s_end - 1].block.separator_block_width,
        };

        for comp in &blocks[s_start..s_end] {
            blocks_width += comp.full.width;
            if let Some(short) = &comp.short {
                series.delta += comp.full.width - short.width;
            }
            series.blocks.push(comp);
        }
        if s_end != blocks.len() {
            blocks_width += series.separator_block_width as f64;
        }
        blocks_computed.push(series);
        s_start = s_end;
    }

    // Progressively switch to short mode
    if offset_left + blocks_width > full_width {
        let mut deltas: Vec<_> = blocks_computed
            .iter()
            .map(|b| b.delta)
            .enumerate()
            .filter(|(_, delta)| *delta > 0.0)
            .collect();
        // Sort in descending order
        deltas.sort_unstable_by(|(_, d1), (_, d2)| d2.total_cmp(d1));
        for (to_switch, delta) in deltas {
            blocks_computed[to_switch].switched_to_short = true;
            blocks_width -= delta;
            if offset_left + blocks_width <= full_width {
                break;
            }
        }
    }

    // Remove all the empty blocks
    for s in &mut blocks_computed {
        s.blocks.retain(|text| {
            (s.switched_to_short
                && text
                    .short
                    .as_ref()
                    .map_or(text.full.width > 0.0, |s| s.width > 0.0))
                || (!s.switched_to_short && text.full.width > 0.0)
        });
    }

    // Render blocks
    buttons.clear();
    let mut j = 0;
    for series in blocks_computed {
        let s_len = series.blocks.len();
        for (i, computed) in series.blocks.into_iter().enumerate() {
            let block = &computed.block;
            let to_render = if series.switched_to_short {
                computed.short.as_ref().unwrap_or(&computed.full)
            } else {
                &computed.full
            };
            j += 1;
            to_render.render(
                context,
                RenderOptions {
                    x_offset: full_width - blocks_width,
                    bar_height: full_height,
                    fg_color: block.color.unwrap_or(config.color),
                    bg_color: block.background,
                    r_left: if i == 0 { config.blocks_r } else { 0.0 },
                    r_right: if i + 1 == s_len { config.blocks_r } else { 0.0 },
                    overlap: config.blocks_overlap,
                },
            );
            buttons.push(
                full_width - blocks_width,
                to_render.width,
                (block.name.clone(), block.instance.clone()),
            );
            blocks_width -= to_render.width;
        }
        if j != blocks.len() && series.separator_block_width > 0 {
            let w = series.separator_block_width as f64;
            if series.separator && config.separator_width > 0.0 {
                config.separator.apply(context);
                context.set_line_width(config.separator_width);
                context.move_to(full_width - blocks_width + w * 0.5, full_height * 0.1);
                context.line_to(full_width - blocks_width + w * 0.5, full_height * 0.9);
                context.stroke().unwrap();
            }
            blocks_width -= w;
        }
    }

    context.reset_clip();
}

pub fn compute_tag_label(label: &str, config: &Config) -> ComputedText {
    ComputedText::new(
        label,
        text::Attributes {
            font: &config.font.0,
            padding_left: config.tags_padding,
            padding_right: config.tags_padding,
            min_width: None,
            align: Default::default(),
            markup: false,
        },
    )
}

fn layer_surface_cb(ctx: EventCtx<State, ZwlrLayerSurfaceV1>) {
    match ctx.event {
        zwlr_layer_surface_v1::Event::Configure(args) => {
            let bar = ctx
                .state
                .bars
                .iter_mut()
                .find(|bar| bar.layer_surface == ctx.proxy)
                .unwrap();
            if bar.hidden {
                return;
            }
            assert_ne!(args.width, 0);
            bar.width = args.width;
            bar.layer_surface.ack_configure(ctx.conn, args.serial);
            bar.mapped = true;
            bar.frame(ctx.conn, &mut ctx.state.shared_state);
        }
        zwlr_layer_surface_v1::Event::Closed => {
            let bar_index = ctx
                .state
                .bars
                .iter()
                .position(|bar| bar.layer_surface == ctx.proxy)
                .unwrap();
            ctx.state.drop_bar(ctx.conn, bar_index);
        }
        _ => (),
    }
}

fn fractional_scale_cb(ctx: EventCtx<State, WpFractionalScaleV1>) {
    let wp_fractional_scale_v1::Event::PreferredScale(scale120) = ctx.event else {
        return;
    };
    let bar = ctx
        .state
        .bars
        .iter_mut()
        .find(|b| b.fractional_scale == Some(ctx.proxy))
        .unwrap();
    if bar.scale120 != Some(scale120) {
        bar.scale120 = Some(scale120);
        bar.frame(ctx.conn, &mut ctx.state.shared_state);
    }
}
