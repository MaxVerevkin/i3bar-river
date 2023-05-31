use crate::color::Color;
use crate::pointer_btn::PointerBtn;
use crate::text::Align;
use crate::utils::{de_first_json, de_last_json, last_line, trim_ascii_start};
use serde::{de, Deserialize, Serialize};
use std::io::{self, Error, ErrorKind};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MinWidth {
    Text(String),
    Pixels(u64),
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
        pending_line: Option<String>,
    },
    JsonNotStarted {
        header: JsonHeader,
    },
    Json {
        header: JsonHeader,
        pending_blocks: Option<Vec<Block>>,
    },
}

impl Protocol {
    /// Extract new data from `bytes`, return unused bytes.
    pub fn process_new_bytes<'a>(&mut self, bytes: &'a [u8]) -> io::Result<&'a [u8]> {
        match self {
            Self::Unknown => match de_first_json::<JsonHeader>(bytes) {
                Ok((Some(header), rem)) if header.version == 1 => {
                    *self = Self::JsonNotStarted { header };
                    self.process_new_bytes(rem)
                }
                Ok((Some(header), _)) => Err(Error::new(
                    ErrorKind::Other,
                    format!("Protocol version {} is not supported", header.version),
                )),
                _ => {
                    *self = Self::PlainText { pending_line: None };
                    self.process_new_bytes(bytes)
                }
            },
            Self::PlainText { pending_line } => match last_line(bytes) {
                Some((new_line, rem)) => {
                    *pending_line = Some(String::from_utf8_lossy(new_line).into());
                    Ok(rem)
                }
                None => Ok(bytes),
            },
            Self::JsonNotStarted { header } => match trim_ascii_start(bytes) {
                [] => Ok(&[]),
                [b'[', rem @ ..] => {
                    *self = Self::Json {
                        header: *header,
                        pending_blocks: None,
                    };
                    self.process_new_bytes(rem)
                }
                [other, ..] => Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("invalid json: expected '[', got '{}'", *other as char),
                )),
            },
            Self::Json {
                pending_blocks: blocks,
                ..
            } => match de_last_json(bytes) {
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
            Self::Unknown | Self::JsonNotStarted { .. } => None,
            Self::PlainText { pending_line, .. } => Some(vec![Block {
                full_text: pending_line.take()?,
                ..Default::default()
            }]),
            Self::Json { pending_blocks, .. } => pending_blocks.take(),
        }
    }

    pub fn supports_clicks(&self) -> bool {
        match self {
            Self::JsonNotStarted { header } | Self::Json { header, .. } => header.click_events,
            _ => false,
        }
    }
}

impl<'de> Deserialize<'de> for MinWidth {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MinWidthVisitor;

        impl<'de> de::Visitor<'de> for MinWidthVisitor {
            type Value = MinWidth;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("positive integer or string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(MinWidth::Text(v.to_owned()))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(MinWidth::Text(v))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(MinWidth::Pixels(v))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(MinWidth::Pixels(
                    v.try_into().map_err(|_| E::custom("invalid min_width"))?,
                ))
            }
        }

        deserializer.deserialize_any(MinWidthVisitor)
    }
}
