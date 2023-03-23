use std::collections::BinaryHeap;

use pangocairo::cairo;

use wayrs_client::connection::Connection;
use wayrs_utils::shm_alloc::BufferSpec;

use crate::button_manager::ButtonManager;
use crate::color::Color;
use crate::config::Config;
use crate::i3bar_protocol::{self, Block, MinWidth};
use crate::ord_adaptor::DefaultLess;
use crate::pointer_btn::PointerBtn;
use crate::protocol::*;
use crate::shared_state::SharedState;
use crate::state::{ComputedBlock, State};
use crate::text::{self, ComputedText, RenderOptions};
use crate::wm_info_provider::WmInfo;

pub struct Bar {
    pub output: WlOutput,
    pub configured: bool,
    pub frame_cb: Option<WlCallback>,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
    pub scale120: Option<u32>,
    pub surface: WlSurface,
    pub layer_surface: ZwlrLayerSurfaceV1,
    pub viewport: WpViewport,
    pub fractional_scale: Option<WpFractionalScaleV1>,
    pub blocks_btns: ButtonManager<(Option<String>, Option<String>)>,
    pub wm_info: WmInfo,
    pub tags_btns: ButtonManager<String>,
    pub tags_computed: Vec<(ColorPair, ComputedText)>,
    pub layout_name_computed: Option<ComputedText>,
}

#[derive(Debug, PartialEq)]
pub struct ColorPair {
    bg: Color,
    fg: Color,
}

impl Bar {
    pub fn set_wm_info(&mut self, info: WmInfo) {
        self.wm_info = info;
        self.tags_btns.clear();
        self.tags_computed.clear();
        self.layout_name_computed = None;
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
        if let Some(tag) = self.tags_btns.click(x) {
            if let Some(wm_info_provider) = &mut ss.wm_info_provider {
                match button {
                    PointerBtn::Left => {
                        wm_info_provider.left_click_on_tag(conn, self.output, seat, tag)
                    }
                    PointerBtn::Right => {
                        wm_info_provider.right_click_on_tag(conn, self.output, seat, tag)
                    }
                    _ => return Ok(()),
                }
            }
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
        assert!(self.configured);

        let (pix_width, pix_height, scale_f) = match self.scale120 {
            Some(scale120) => (
                // rounding halfway away from zero
                (self.width * scale120 + 60) / 120,
                (self.height * scale120 + 60) / 120,
                scale120 as f64 / 120.0,
            ),
            None => (
                self.width * self.scale,
                self.height * self.scale,
                self.scale as f64,
            ),
        };

        let width_f = self.width as f64;
        let height_f = self.height as f64;

        let (buffer, canvas) = ss.shm.alloc_buffer(
            conn,
            BufferSpec {
                width: pix_width,
                height: pix_height,
                stride: pix_width * 4,
                format: wl_shm::Format::Argb8888,
            },
        );

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

        // Background
        cairo_ctx.save().unwrap();
        cairo_ctx.set_operator(cairo::Operator::Source);
        ss.config.background.apply(&cairo_ctx);
        cairo_ctx.paint().unwrap();
        cairo_ctx.restore().unwrap();

        // Compute tags
        if self.tags_computed.is_empty() {
            let mut offset_left = 0.0;
            self.tags_btns.clear();
            for tag in &self.wm_info.tags {
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
                let comp = compute_tag_label(&tag.name, &ss.config, &cairo_ctx);
                self.tags_btns
                    .push(offset_left, comp.width, tag.name.clone());
                offset_left += comp.width;
                self.tags_computed.push((ColorPair { bg, fg }, comp));
            }
        }

        // Display tags
        let mut offset_left = 0.0;
        for (i, label) in self.tags_computed.iter().enumerate() {
            label.1.render(
                &cairo_ctx,
                RenderOptions {
                    x_offset: offset_left,
                    bar_height: height_f,
                    fg_color: label.0.fg,
                    bg_color: Some(label.0.bg),
                    r_left: if i == 0 || self.tags_computed[i - 1].0 != label.0 {
                        ss.config.tags_r
                    } else {
                        0.0
                    },
                    r_right: if i + 1 == self.tags_computed.len()
                        || self.tags_computed[i + 1].0 != label.0
                    {
                        ss.config.tags_r
                    } else {
                        0.0
                    },
                    overlap: 0.0,
                },
            );
            offset_left += label.1.width;
        }

        // Display layout name
        if ss.config.show_layout_name {
            if let Some(layout_name) = &self.wm_info.layout_name {
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
                        &cairo_ctx,
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

        // Display the blocks
        render_blocks(
            &cairo_ctx,
            &ss.config,
            &ss.blocks,
            &mut ss.blocks_cache,
            &mut self.blocks_btns,
            offset_left,
            width_f,
            height_f,
        );

        self.viewport
            .set_destination(conn, self.width as i32, self.height as i32);

        self.surface.attach(conn, buffer.into_wl_buffer(), 0, 0);
        self.surface.damage(conn, 0, 0, i32::MAX, i32::MAX);
        self.surface.commit(conn);
    }

    pub fn request_frame(&mut self, conn: &mut Connection<State>) {
        if self.configured && self.frame_cb.is_none() {
            self.frame_cb = Some(self.surface.frame_with_cb(conn, |conn, state, cb, _| {
                if let Some(bar) = state.bars.iter_mut().find(|bar| bar.frame_cb == Some(cb)) {
                    bar.frame_cb = None;
                    bar.frame(conn, &mut state.shared_state);
                }
            }));
            self.surface.commit(conn);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_blocks(
    context: &cairo::Context,
    config: &Config,
    blocks: &[Block],
    blocks_cache: &mut Vec<ComputedBlock>,
    buttons: &mut ButtonManager<(Option<String>, Option<String>)>,
    offset_left: f64,
    full_width: f64,
    full_height: f64,
) {
    let comp_min_width = |block: &Block| {
        let markup = block.markup.as_deref() == Some("pango");
        match &block.min_width {
            Some(MinWidth::Pixels(p)) => Some(*p as f64),
            Some(MinWidth::Text(t)) => Some(text::width_of(t, context, markup, &config.font.0)),
            None => None,
        }
    };
    let comp_full = |block: &Block, min_width: Option<f64>| {
        let markup = block.markup.as_deref() == Some("pango");
        text::ComputedText::new(
            &block.full_text,
            text::Attributes {
                font: &config.font,
                padding_left: 0.0,
                padding_right: 0.0,
                min_width,
                align: block.align.unwrap_or_default(),
                markup,
            },
            context,
        )
    };
    let comp_short = |block: &Block, min_width: Option<f64>| {
        let markup = block.markup.as_deref() == Some("pango");
        block.short_text.as_ref().map(|short_text| {
            text::ComputedText::new(
                short_text,
                text::Attributes {
                    font: &config.font,
                    padding_left: 0.0,
                    padding_right: 0.0,
                    min_width,
                    align: block.align.unwrap_or_default(),
                    markup,
                },
                context,
            )
        })
    };

    // update cashe
    if blocks.len() != blocks_cache.len() {
        blocks_cache.clear();
        for block in blocks {
            let mw = comp_min_width(block);
            blocks_cache.push(ComputedBlock {
                block: block.clone(),
                full: comp_full(block, mw),
                short: comp_short(block, mw),
                min_width: mw,
            });
        }
    } else {
        for (block, computed) in blocks.iter().zip(blocks_cache.iter_mut()) {
            if block.min_width != computed.block.min_width || block.markup != computed.block.markup
            {
                let mw = comp_min_width(block);
                *computed = ComputedBlock {
                    block: block.clone(),
                    full: comp_full(block, mw),
                    short: comp_short(block, mw),
                    min_width: mw,
                };
            } else {
                if block.full_text != computed.block.full_text {
                    computed.block.full_text = block.full_text.clone();
                    computed.full = comp_full(block, computed.min_width);
                }
                if block.short_text != computed.block.short_text {
                    computed.block.full_text = block.full_text.clone();
                    computed.short = comp_short(block, computed.min_width);
                }
                computed.block.color = block.color;
                computed.block.background = block.background;
                computed.block.align = block.align;
                computed.block.name = block.name.clone();
                computed.block.instance = block.instance.clone();
                computed.block.separator = block.separator;
                computed.block.separator_block_width = block.separator_block_width;
            }
        }
    }

    #[derive(Debug)]
    struct LogialBlock<'a> {
        blocks: Vec<(&'a ComputedBlock, bool)>,
        delta: f64,
        separator: bool,
        separator_block_width: u8,
    }

    let mut blocks_computed = Vec::new();
    let mut blocks_width = 0.0;
    let mut s_start = 0;
    while s_start < blocks.len() {
        let mut s_end = s_start + 1;
        let series_name = &blocks[s_start].name;
        while s_end < blocks.len()
            && blocks[s_end - 1].separator_block_width == 0
            && &blocks[s_end].name == series_name
        {
            s_end += 1;
        }

        let mut series = LogialBlock {
            blocks: Vec::with_capacity(s_end - s_start),
            delta: 0.0,
            separator: blocks[s_end - 1].separator,
            separator_block_width: blocks[s_end - 1].separator_block_width,
        };

        for comp in &blocks_cache[s_start..s_end] {
            blocks_width += comp.full.width;
            if let Some(short) = &comp.short {
                series.delta += comp.full.width - short.width;
            }
            series.blocks.push((comp, false));
        }
        if s_end != blocks.len() {
            blocks_width += series.separator_block_width as f64;
        }
        blocks_computed.push(series);
        s_start = s_end;
    }

    // Progressively switch to short mode
    if offset_left + blocks_width > full_width {
        let mut heap = BinaryHeap::new();
        for (i, b) in blocks_computed.iter().enumerate() {
            if b.delta > 0.0 {
                heap.push((DefaultLess(b.delta), i));
            }
        }
        while let Some((DefaultLess(delta), to_switch)) = heap.pop() {
            for comp in &mut blocks_computed[to_switch].blocks {
                comp.1 = true;
            }
            blocks_width -= delta;
            if offset_left + blocks_width <= full_width {
                break;
            }
        }
    }

    // Remove all the empy blocks
    for s in &mut blocks_computed {
        s.blocks.retain(|(text, is_short)| {
            (*is_short
                && text
                    .short
                    .as_ref()
                    .map_or(text.full.width > 0.0, |s| s.width > 0.0))
                || (!is_short && text.full.width > 0.0)
        });
    }

    // Render blocks
    buttons.clear();
    let mut j = 0;
    for series in blocks_computed {
        let s_len = series.blocks.len();
        for (i, (computed, is_short)) in series.blocks.into_iter().enumerate() {
            let block = &computed.block;
            let to_render = if is_short {
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
}

pub fn compute_tag_label(label: &str, config: &Config, context: &cairo::Context) -> ComputedText {
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
        context,
    )
}
