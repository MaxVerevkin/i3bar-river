use crate::config::Config;
use crate::i3bar_protocol::{Block, MinWidth};
use crate::text::{self, ComputedText};

#[derive(Default)]
pub struct BlocksCache {
    computed: Vec<ComputedBlock>,
}

pub struct ComputedBlock {
    pub block: Block,
    pub full: ComputedText,
    pub short: Option<ComputedText>,
    pub min_width: Option<f64>,
}

impl BlocksCache {
    pub fn process_new_blocks(&mut self, config: &Config, blocks: &[Block]) {
        let comp_min_width = |block: &Block| {
            let markup = block.markup.as_deref() == Some("pango");
            match &block.min_width {
                Some(MinWidth::Pixels(p)) => Some(*p as f64),
                Some(MinWidth::Text(t)) => Some(text::width_of(t, markup, &config.font.0)),
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
                )
            })
        };

        // update cashe
        if blocks.len() != self.computed.len() {
            self.computed.clear();
            for block in blocks {
                let mw = comp_min_width(block);
                self.computed.push(ComputedBlock {
                    block: block.clone(),
                    full: comp_full(block, mw),
                    short: comp_short(block, mw),
                    min_width: mw,
                });
            }
        } else {
            for (block, computed) in blocks.iter().zip(self.computed.iter_mut()) {
                if block.min_width != computed.block.min_width
                    || block.markup != computed.block.markup
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
    }

    pub fn get_computed(&self) -> &[ComputedBlock] {
        &self.computed
    }
}
