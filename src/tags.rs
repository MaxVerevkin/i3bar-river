use crate::text::{Attributes, ComputedText, Text};
use cairo::Context;
use pango::FontDescription;
use pangocairo::{cairo, pango};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagState {
    Focused,
    Inactive,
    Urgent,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TagsInfo {
    pub focused: u32,
    pub urgent: u32,
}

impl TagsInfo {
    pub fn get_state(self, tag: usize) -> TagState {
        if self.urgent >> tag & 1 == 1 {
            TagState::Urgent
        } else if self.focused >> tag & 1 == 1 {
            TagState::Focused
        } else {
            TagState::Inactive
        }
    }
}

pub fn compute_tag_label(label: String, font: FontDescription, context: &Context) -> ComputedText {
    let text = Text {
        attr: Attributes {
            font,
            padding_left: 25.0,
            padding_right: 25.0,
            min_width: None,
            align: Default::default(),
            markup: false,
        },
        text: label,
    };
    text.compute(context)
}
