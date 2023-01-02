use crate::color::Color;
use anyhow::{Context, Result};
use dirs_next::config_dir;
use pangocairo::pango::FontDescription;
use serde::{de, Deserialize};
use std::fmt;
use std::fs::read_to_string;
use std::ops::Deref;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    // colors
    pub background: Color,
    pub color: Color,
    pub separator: Color,
    pub tag_fg: Color,
    pub tag_bg: Color,
    pub tag_focused_fg: Color,
    pub tag_focused_bg: Color,
    pub tag_urgent_fg: Color,
    pub tag_urgent_bg: Color,
    pub tag_inactive_fg: Color,
    pub tag_inactive_bg: Color,
    // font and size
    pub font: Font,
    pub height: u32,
    pub separator_width: f64,
    pub tags_r: f64,
    pub blocks_r: f64,
    pub blocks_overlap: f64,
    // command
    pub command: Option<String>,
    // misc
    pub hide_inactive_tags: bool,
    pub invert_touchpad_scrolling: bool,
    pub show_layout_name: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // A kind of gruvbox theme
            background: Color::from_rgba_hex(0x282828ff),
            color: Color::from_rgba_hex(0xffffffff),
            separator: Color::from_rgba_hex(0x9a8a62ff),
            tag_fg: Color::from_rgba_hex(0xd79921ff),
            tag_bg: Color::from_rgba_hex(0x282828ff),
            tag_focused_fg: Color::from_rgba_hex(0x1d2021ff),
            tag_focused_bg: Color::from_rgba_hex(0x689d68ff),
            tag_urgent_fg: Color::from_rgba_hex(0x282828ff),
            tag_urgent_bg: Color::from_rgba_hex(0xcc241dff),
            tag_inactive_fg: Color::from_rgba_hex(0xd79921ff),
            tag_inactive_bg: Color::from_rgba_hex(0x282828ff),
            font: Font::new("monospace 10"),
            height: 24,
            separator_width: 2.0,
            tags_r: 0.0,
            blocks_r: 0.0,
            blocks_overlap: 0.0,
            command: None,
            hide_inactive_tags: true,
            invert_touchpad_scrolling: true,
            show_layout_name: true,
        }
    }
}

impl Config {
    pub fn new() -> Result<Self> {
        Ok(match config_dir() {
            Some(mut config_path) => {
                config_path.push("i3bar-river");
                config_path.push("config.toml");
                if config_path.exists() {
                    let config =
                        read_to_string(config_path).context("Failed to read configuration")?;
                    toml::from_str(&config).context("Failed to deserialize configuration")?
                } else {
                    info!("Using default configuration");
                    Self::default()
                }
            }
            None => {
                warn!("Could not find the configuration path");
                info!("Using default configuration");
                Self::default()
            }
        })
    }
}

#[derive(Debug)]
pub struct Font(pub FontDescription);

impl Font {
    pub fn new(desc: &str) -> Self {
        Self(FontDescription::from_string(desc))
    }
}

impl Deref for Font {
    type Target = FontDescription;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> de::Deserialize<'de> for Font {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct FontVisitor;

        impl<'de> de::Visitor<'de> for FontVisitor {
            type Value = Font;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("font description")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Font::new(s))
            }
        }

        deserializer.deserialize_str(FontVisitor)
    }
}
