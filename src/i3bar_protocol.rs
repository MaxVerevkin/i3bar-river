use crate::color::Color;
use crate::pointer_btn::PointerBtn;
use crate::text::Align;
use crate::utils::{de_first_json, de_last_json, last_line};
use serde::Deserialize;
use serde::Serialize;
use std::io::{Error, ErrorKind, Result};

#[derive(Clone, Deserialize, Default, Debug)]
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
    pub align: Align,
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

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
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

#[derive(Deserialize, Clone, Copy, Debug)]
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

#[derive(Debug)]
pub enum Protocol {
    Unknown,
    PlainText {
        line: Option<String>,
    },
    JsonNotStarted(JsonHeader),
    Json {
        header: JsonHeader,
        blocks: Option<Vec<Block>>,
    },
}

impl Protocol {
    /// Extract new data from `bytes`, return unused bytes.
    pub fn process_new_bytes<'a>(&mut self, bytes: &'a [u8]) -> Result<&'a [u8]> {
        match self {
            Self::Unknown => match de_first_json::<JsonHeader>(bytes) {
                Ok((Some(header), rem)) if header.version == 1 => {
                    *self = Self::JsonNotStarted(header);
                    self.process_new_bytes(rem)
                }
                Ok((Some(header), _)) => Err(Error::new(
                    ErrorKind::Other,
                    format!("Protocol version {} is not supported", header.version),
                )),
                _ => {
                    *self = Self::PlainText { line: None };
                    self.process_new_bytes(bytes)
                }
            },
            Self::PlainText { line } => {
                if let Some((new_line, rem)) = last_line(bytes) {
                    *line = Some(String::from_utf8_lossy(new_line).into());
                    Ok(rem)
                } else {
                    Ok(bytes)
                }
            }
            Self::JsonNotStarted(header) => {
                let mut bytes = bytes;
                while bytes.first().map_or(false, |&x| x == b' ' || x == b'\n') {
                    bytes = &bytes[1..];
                }
                match bytes.first() {
                    Some(b'[') => {
                        *self = Self::Json {
                            header: *header,
                            blocks: None,
                        };
                        self.process_new_bytes(&bytes[1..])
                    }
                    Some(other) => Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("invalid json: expected '[', got '{}'", *other as char),
                    )),
                    _ => Ok(bytes),
                }
            }
            Self::Json { header: _, blocks } => match de_last_json(bytes) {
                Err(e) => Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("invalid json: {e}"),
                )),
                Ok((new_blocks, rem)) => {
                    if let Some(new_blocks) = new_blocks {
                        *blocks = Some(new_blocks);
                    }
                    Ok(rem)
                }
            },
        }
    }

    pub fn get_blocks(&mut self) -> Option<Vec<Block>> {
        match self {
            Self::Unknown => None,
            Self::PlainText { line, .. } => Some(vec![Block {
                full_text: line.take()?,
                ..Default::default()
            }]),
            Self::JsonNotStarted(_) => None,
            Self::Json { blocks, .. } => blocks.take(),
        }
    }

    pub fn supports_clicks(&self) -> bool {
        match self {
            Self::JsonNotStarted(h) => h.click_events,
            Self::Json { header, .. } => header.click_events,
            _ => false,
        }
    }
}
