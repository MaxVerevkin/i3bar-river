use crate::color::Color;
use crate::protocol::zwlr_layer_surface_v1;
use anyhow::{Context, Result};
use pangocairo::pango::FontDescription;
use serde::{de, Deserialize};
use std::fs::read_to_string;
use std::ops::Deref;
use std::path::PathBuf;
use std::{env, fmt};

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
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub margin_right: i32,
    pub separator_width: f64,
    pub tags_r: f64,
    pub tags_padding: f64,
    pub blocks_r: f64,
    pub blocks_overlap: f64,
    // command
    pub command: Option<String>,
    // misc
    pub position: Position,
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
            margin_top: 0,
            margin_bottom: 0,
            margin_left: 0,
            margin_right: 0,
            separator_width: 2.0,
            tags_r: 0.0,
            tags_padding: 25.0,
            blocks_r: 0.0,
            blocks_overlap: 0.0,
            command: None,
            position: Position::Top,
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
                    eprintln!("Using default configuration");
                    Self::default()
                }
            }
            None => {
                eprintln!("Could not find the configuration path");
                eprintln!("Using default configuration");
                Self::default()
            }
        })
    }
}

fn config_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(env::var_os("HOME")?).join(".config")))
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Bottom,
}

impl From<Position> for zwlr_layer_surface_v1::Anchor {
    fn from(position: Position) -> Self {
        match position {
            Position::Top => Self::Top | Self::Left | Self::Right,
            Position::Bottom => Self::Bottom | Self::Left | Self::Right,
        }
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
