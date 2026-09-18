use crate::hardware::{CondFlags, Registers};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use enum_map::EnumMap;
use std::time::Duration;

pub const MASK_REG: u16 = 0b111; // used for registers
pub const MASK_IMM5: u16 = 0b1_1111; // used for sign-extended 5-bit integers
pub const MASK_OFFSET6: u16 = 0b11_1111; // used for sign-extended 6-bit integers
pub const MASK_SE_9: u16 = 0b1_1111_1111; // used for sign-extended 9-bit integers
pub const MASK_SE_11: u16 = 0b111_1111_1111; // used for sign-extended 11-bit integers

pub fn sign_extend(mut x: u16, bit_count: u32) -> u16 {
    // check if the sign bit (the furthest left/most significant bit) is 1, which means the number
    // is negative
    if (x >> (bit_count - 1)) & 1 != 0 {
        x |= 0xFFFF << bit_count; // fill in 1's before the value
    }
    x
}

pub fn check_key() -> bool {
    // check for term event
    event::poll(Duration::from_secs(0)).unwrap_or(false)
}

pub fn get_char() -> u8 {
    loop {
        if let Ok(Event::Key(key_event)) = event::read() {
            if key_event.kind == KeyEventKind::Press {
                match key_event.code {
                    KeyCode::Char(c) => return c as u8,
                    KeyCode::Enter => return b'\n',
                    KeyCode::Backspace => return 0x7F,
                    KeyCode::Esc => return 0x1B,
                    _ => continue,
                }
            }
        }
    }
}

pub fn update_flags(value: u16, register_data: &mut EnumMap<Registers, u16>) {
    register_data[Registers::RCond] = match value {
        // negative is MSB is 1
        v if (v >> 15) == 1 => CondFlags::Neg,
        0 => CondFlags::Zero,
        _ => CondFlags::Pos,
    } as u16;
}
