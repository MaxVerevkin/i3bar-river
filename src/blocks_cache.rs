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
    pub fn process_new_blocks(&mut self, config: &Config, blocks: Vec<Block>) {
        if blocks.len() != self.computed.len() {
            self.computed.clear();
            self.computed.reserve(blocks.len());
            self.computed
                .extend(blocks.into_iter().map(|b| ComputedBlock::new(b, config)));
            return;
        }

        for (block, computed) in blocks.into_iter().zip(self.computed.iter_mut()) {
            computed.update(block, config);
        }
    }

    pub fn get_computed(&self) -> &[ComputedBlock] {
        &self.computed
    }
}

impl ComputedBlock {
    fn new(block: Block, config: &Config) -> Self {
        let mw = comp_min_width(&block, config);
        Self {
            full: comp_full(&block, mw, config),
            short: comp_short(&block, mw, config),
            min_width: mw,
            block,
        }
    }

    fn update(&mut self, block: Block, config: &Config) {
        if block.min_width != self.block.min_width || block.markup != self.block.markup {
            *self = ComputedBlock::new(block, config);
        } else {
            if block.full_text != self.block.full_text {
                self.full = comp_full(&block, self.min_width, config);
            }
            if block.short_text != self.block.short_text {
                self.short = comp_short(&block, self.min_width, config);
            }
            self.block = block;
        }
    }
}

fn comp_min_width(block: &Block, config: &Config) -> Option<f64> {
    let markup = block.markup.as_deref() == Some("pango");
    match &block.min_width {
        Some(MinWidth::Pixels(p)) => Some(*p as f64),
        Some(MinWidth::Text(t)) => Some(text::width_of(t, markup, &config.font.0)),
        None => None,
    }
}

fn comp_full(block: &Block, min_width: Option<f64>, config: &Config) -> ComputedText {
    let markup = block.markup.as_deref() == Some("pango");
    ComputedText::new(
        &block.full_text,
        text::Attributes {
            font: &config.font,
            padding_left: 0.0,
            padding_right: 0.0,
            min_width,
            align: block.align,
            markup,
        },
    )
}

fn comp_short(block: &Block, min_width: Option<f64>, config: &Config) -> Option<ComputedText> {
    let markup = block.markup.as_deref() == Some("pango");
    block.short_text.as_ref().map(|short_text| {
        text::ComputedText::new(
            short_text,
            text::Attributes {
                font: &config.font,
                padding_left: 0.0,
                padding_right: 0.0,
                min_width,
                align: block.align,
                markup,
            },
        )
    })
}
