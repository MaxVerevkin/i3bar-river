use serde::{Serialize, Serializer};

// From linux/input-event-codes.h
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
// const BTN_SIDE: u32 = 0x113;
// const BTN_EXTRA: u32 = 0x114;
const BTN_FORWARD: u32 = 0x115;
const BTN_BACK: u32 = 0x116;
// const BTN_TASK: u32 = 0x117;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerBtn {
    Left,
    Middle,
    Right,
    Forward,
    WheelUp,
    WheelDown,
    Back,
    Unknown,
}

impl Default for PointerBtn {
    fn default() -> Self {
        Self::Unknown
    }
}

impl From<u32> for PointerBtn {
    fn from(code: u32) -> Self {
        use PointerBtn::*;
        match code {
            BTN_LEFT => Left,
            BTN_MIDDLE => Middle,
            BTN_RIGHT => Right,
            BTN_FORWARD => Forward,
            BTN_BACK => Back,
            _ => Unknown,
        }
    }
}

impl Serialize for PointerBtn {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let num = match *self {
            PointerBtn::Left => 1,
            PointerBtn::Middle => 2,
            PointerBtn::Right => 3,
            PointerBtn::WheelUp => 4,
            PointerBtn::WheelDown => 5,
            PointerBtn::Forward => 9,
            PointerBtn::Back => 8,
            PointerBtn::Unknown => 0,
        };
        serializer.serialize_u8(num)
    }
}
