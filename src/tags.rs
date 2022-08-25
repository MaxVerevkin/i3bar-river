use crate::text::{Attributes, ComputedText};
use cairo::Context;
use pango::FontDescription;
use pangocairo::{cairo, pango};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagState {
    Urgent,
    Focused,
    Active,
    Inactive,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TagsInfo {
    pub focused: u32,
    pub urgent: u32,
    pub active: u32,
}

impl TagsInfo {
    pub fn get_state(self, tag: usize) -> TagState {
        if self.urgent >> tag & 1 == 1 {
            TagState::Urgent
        } else if self.focused >> tag & 1 == 1 {
            TagState::Focused
        } else if self.active >> tag & 1 == 1 {
            TagState::Active
        } else {
            TagState::Inactive
        }
    }
}

pub fn compute_tag_label(label: &str, font: FontDescription, context: &Context) -> ComputedText {
    ComputedText::new(
        label,
        Attributes {
            font,
            padding_left: 25.0,
            padding_right: 25.0,
            min_width: None,
            align: Default::default(),
            markup: false,
        },
        context,
    )
}
