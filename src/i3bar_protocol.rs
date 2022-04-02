use crate::color::Color;
use crate::pointer_btn::PointerBtn;
use crate::text::Align;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Deserializer, Result as SerdeResult};
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
    Json(JsonHeader, Option<Vec<Block>>, Option<String>),
}

impl Protocol {
    pub fn process_line(&mut self, line: String) -> Result<()> {
        macro_rules! invalid {
            ($fmt:expr) => {
                return Err(Error::new(ErrorKind::InvalidData, format!($fmt)))
            };
        }

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
                let line = line.trim_start();
                if !line.is_empty() {
                    if !line.starts_with('[') {
                        invalid!("Expected '['");
                    }
                    *self = Self::Json(*header, None, None);
                    self.process_line(line[1..].to_owned())?;
                }
            }
            Self::Json(_, blocks, start_orig) => match start_orig {
                Some(start) => {
                    start.push_str(&line);
                    match de_last::<Vec<Block>>(start) {
                        Err(e) => invalid!("Invalid JSON: {e}"),
                        Ok((rem, new_blocks)) => {
                            *start_orig = (!rem.is_empty()).then(|| rem.to_owned());
                            if let Some(new_blocks) = new_blocks {
                                *blocks = Some(new_blocks);
                            }
                        }
                    }
                }
                None => match de_last::<Vec<Block>>(&line) {
                    Err(e) => invalid!("Invalid JSON: {e}"),
                    Ok((rem, new_blocks)) => {
                        *start_orig = (!rem.is_empty()).then(|| rem.to_owned());
                        if let Some(new_blocks) = new_blocks {
                            *blocks = Some(new_blocks);
                        }
                    }
                },
            },
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
            Self::Json(_, blocks, _) => blocks.take(),
        }
    }

    pub fn supports_clicks(&self) -> bool {
        match self {
            Self::JsonNotStarted(h) => h.click_events,
            Self::Json(h, _, _) => h.click_events,
            _ => false,
        }
    }
}

/// Deserialize the last complete object. Returns (`remaining`, `object`). See tests for examples.
fn de_last<'a, T: Deserialize<'a>>(mut s: &'a str) -> SerdeResult<(&'a str, Option<T>)> {
    let mut last = None;
    let mut tmp;
    loop {
        (s, tmp) = de_once(s)?;
        last = match tmp {
            Some(obj) => Some(obj),
            None => return Ok((s, last)),
        };
    }
}

/// Deserialize the first complete object. Returns (`remaining`, `object`).
fn de_once<'a, T: Deserialize<'a>>(s: &'a str) -> SerdeResult<(&'a str, Option<T>)> {
    let s = s.trim_start_matches(|x: char| x.is_ascii_whitespace() || x == ',');
    let mut de = Deserializer::from_str(s).into_iter();
    match de.next() {
        Some(Ok(obj)) => Ok((&s[de.byte_offset()..], Some(obj))),
        Some(Err(e)) if e.is_eof() => Ok((&s[de.byte_offset()..], None)),
        Some(Err(e)) => Err(e),
        None => Ok((&s[de.byte_offset()..], None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_json() {
        let s = ",[2], [3], [4, 3],[32][3] ";
        assert_eq!(de_last::<Vec<u8>>(s).unwrap(), ("", Some(vec![3])));

        let s = ",[2], [3], [4, 3],[32][3] [2, 4";
        assert_eq!(de_last::<Vec<u8>>(s).unwrap(), ("[2, 4", Some(vec![3])));

        let s = ",[2], [3], [4, 3],[32] invalid";
        assert!(de_last::<Vec<u8>>(s).is_err());
    }
}
