use crate::hardware::{CondFlags, DISP_STATUS, KB_DATA, KB_STATUS, Registers};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use enum_map::EnumMap;
use std::time::Duration;

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

pub fn mem_read(address: u16, memory_data: &mut [u16]) -> u16 {
    // print!("MEM READ {:#06X}\r\n", address);
    if address == KB_STATUS {
        // print!("Reading from keyboard\r\n");
        if check_key() {
            memory_data[KB_STATUS as usize] = 1 << 15; // set MSB
            memory_data[KB_DATA as usize] = get_char() as u16;
            // print!("Got key {}\r\n", memory_data[KB_DATA as usize]);
        } else {
            // print!("No key\r\n");
            memory_data[KB_STATUS as usize] = 0;
        }
    } else if address == DISP_STATUS {
        // set display status so program knows it can print
        memory_data[DISP_STATUS as usize] = 1 << 15; // set MSB
    }
    memory_data[address as usize]
}

pub fn update_flags(value: u16, register_data: &mut EnumMap<Registers, u16>) {
    register_data[Registers::RCond] = match value {
        // negative is MSB is 1
        v if (v >> 15) == 1 => CondFlags::Neg,
        0 => CondFlags::Zero,
        _ => CondFlags::Pos,
    } as u16;
}
