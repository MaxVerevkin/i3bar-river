use serde::Deserialize;

use crate::color::Color;

use super::Palette;

mod source {
    use serde::Deserialize;

    use crate::{color::Color, config::Palette};

    #[derive(Deserialize, Debug, Default, Clone, Copy)]
    #[serde(deny_unknown_fields, default)]
    pub struct MaybePalette {
        // colors
        pub background: Option<Color>,
        pub color: Option<Color>,
        pub separator: Option<Color>,
        pub tag_fg: Option<Color>,
        pub tag_bg: Option<Color>,
        pub tag_focused_fg: Option<Color>,
        pub tag_focused_bg: Option<Color>,
        pub tag_urgent_fg: Option<Color>,
        pub tag_urgent_bg: Option<Color>,
        pub tag_inactive_fg: Option<Color>,
        pub tag_inactive_bg: Option<Color>,

        // Additional shown stuff
        pub hide_inactive_tags: Option<bool>,
        pub show_tags: Option<bool>,
        pub show_layout_name: Option<bool>,
        pub blend: Option<bool>,
        pub show_mode: Option<bool>,
    }

    #[derive(Deserialize, Default)]
    #[serde(deny_unknown_fields, default)]
    pub(super) struct Theme {
        pub(super) palette: MaybePalette,
        pub(super) unfocused_palette: MaybePalette,
    }

    impl MaybePalette {
        pub(super) fn to_palette_with(self, p: Palette) -> Palette {
            Palette {
                background: self.background.unwrap_or(p.background),
                color: self.color.unwrap_or(p.color),
                separator: self.separator.unwrap_or(p.separator),
                tag_fg: self.tag_fg.unwrap_or(p.tag_fg),
                tag_bg: self.tag_bg.unwrap_or(p.tag_bg),
                tag_focused_fg: self.tag_focused_fg.unwrap_or(p.tag_focused_fg),
                tag_focused_bg: self.tag_focused_bg.unwrap_or(p.tag_focused_bg),
                tag_urgent_fg: self.tag_urgent_fg.unwrap_or(p.tag_urgent_fg),
                tag_urgent_bg: self.tag_urgent_bg.unwrap_or(p.tag_urgent_bg),
                tag_inactive_fg: self.tag_inactive_fg.unwrap_or(p.tag_inactive_fg),
                tag_inactive_bg: self.tag_inactive_bg.unwrap_or(p.tag_inactive_bg),

                hide_inactive_tags: self.hide_inactive_tags.unwrap_or(p.hide_inactive_tags),
                show_tags: self.show_tags.unwrap_or(p.show_tags),
                show_layout_name: self.show_layout_name.unwrap_or(p.show_layout_name),
                blend: self.blend.unwrap_or(p.blend),
                show_mode: self.show_mode.unwrap_or(p.show_mode),
            }
        }
    }
}

#[derive(Debug)]
pub struct Theme {
    pub palette: Palette,
    pub unfocused_output: Palette, // inherits from `palette`
}
impl<'de> Deserialize<'de> for Theme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let src = source::Theme::deserialize(deserializer)?;

        Ok(Theme {
            palette: src.palette.to_palette_with(Palette::default()),
            unfocused_output: src
                .unfocused_palette
                .to_palette_with(src.palette.to_palette_with(unfocused_palette())),
        })
    }
}

fn unfocused_palette() -> Palette {
    Palette {
        // Use a darker white, to show that its unfocused
        tag_fg: Color::from_rgba_hex(0xbbbbbbbb),
        tag_focused_fg: Color::from_rgba_hex(0xbbbbbbbb),
        tag_inactive_fg: Color::from_rgba_hex(0xbbbbbbbb),
        tag_urgent_fg: Color::from_rgba_hex(0xbbbbbbbb),
        color: Color::from_rgba_hex(0xbbbbbbbb),

        ..Palette::default()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            unfocused_output: unfocused_palette(),
        }
    }
}
