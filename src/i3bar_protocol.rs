use crate::color::Color;
use crate::pointer_btn::PointerBtn;
use crate::text::Align;
use serde::Deserialize;
use serde::Serialize;
use std::io::{Error, ErrorKind, Result};

#[derive(Deserialize, Default)]
pub struct Block {
    pub full_text: String,
    #[serde(default)]
    pub short_text: Option<String>,
    #[serde(default)]
    pub color: Option<Color>,
    #[serde(default)]
    pub background: Option<Color>,
    #[serde(default)]
    pub min_width: Option<MinWidth>,
    #[serde(default)]
    pub align: Option<Align>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default = "def_sep")]
    pub separator: bool,
    #[serde(default = "def_sep_width")]
    pub separator_block_width: u8,
    #[serde(default)]
    pub markup: Option<String>,
}

fn def_sep() -> bool {
    true
}

fn def_sep_width() -> u8 {
    9
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum MinWidth {
    Text(String),
    Pixels(u32),
}

#[derive(Serialize, Default)]
pub struct Event<'a> {
    pub name: Option<&'a str>,
    pub instance: Option<&'a str>,
    pub button: PointerBtn,
    // Not available on wayland
    pub modifiers: Vec<()>,
    // I see no reason to have these in the protocol, as a lot depends on font & pango markup
    pub x: u8,
    pub y: u8,
    pub relative_x: u8,
    pub relative_y: u8,
    pub output_x: u8,
    pub output_y: u8,
    pub width: u8,
    pub height: u8,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct JsonHeader {
    version: u8,
    #[serde(default)]
    #[allow(dead_code)]
    stop_signal: i32,
    #[serde(default)]
    #[allow(dead_code)]
    cont_signal: i32,
    #[serde(default)]
    click_events: bool,
}

pub enum Protocol {
    Unknown,
    PlainText(Option<String>),
    JsonNotStarted(JsonHeader),
    Json(JsonHeader, Option<Vec<Block>>),
}

impl Protocol {
    pub fn process_line(&mut self, line: String) -> Result<()> {
        match self {
            Self::Unknown => {
                if let Ok(header) = serde_json::from_str::<JsonHeader>(&line) {
                    if header.version == 1 {
                        *self = Protocol::JsonNotStarted(header);
                        return Ok(());
                    }
                }
                *self = Self::PlainText(Some(line))
            }
            Self::PlainText(s) => *s = Some(line),
            Self::JsonNotStarted(header) => {
                let line = line.trim();
                if !line.is_empty() {
                    if !line.starts_with('[') && line.is_empty() {
                        return Err(Error::new(ErrorKind::InvalidData, "Expected '['"));
                    }
                    *self = Self::Json(*header, None);
                    self.process_line(line[1..].to_owned())?;
                }
            }
            Self::Json(_, blocks) => {
                let line = line.trim();
                if !line.is_empty() {
                    if !line.ends_with(',') {
                        return Err(Error::new(ErrorKind::InvalidData, "Expected ','"));
                    }
                    *blocks = match serde_json::from_str::<Vec<Block>>(&line[..(line.len() - 1)]) {
                        Ok(b) => Some(b),
                        Err(e) => {
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                format!("Invalid JSON: {e}"),
                            ))
                        }
                    };
                }
            }
        }
        Ok(())
    }

    pub fn get_blocks(&mut self) -> Option<Vec<Block>> {
        match self {
            Self::Unknown => None,
            Self::PlainText(text) => Some(vec![Block {
                full_text: text.take()?,
                ..Default::default()
            }]),
            Self::JsonNotStarted(_) => None,
            Self::Json(_, blocks) => blocks.take(),
        }
    }

    pub fn supports_clicks(&self) -> bool {
        match self {
            Self::JsonNotStarted(h) => h.click_events,
            Self::Json(h, _) => h.click_events,
            _ => false,
        }
    }
}
