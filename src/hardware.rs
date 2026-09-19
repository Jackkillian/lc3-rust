use byteorder::{BigEndian, ReadBytesExt};
use enum_map::Enum;
use num_enum::TryFromPrimitive;
use std::{
    fs,
    io::{self, Cursor},
};

use crate::utils::{check_key, get_char};

pub const MEM_SIZE: u32 = 1 << 16; // 2^16

pub struct Memory {
    data: Vec<u16>,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            data: vec![0; MEM_SIZE as usize],
        }
    }

    pub fn read_file(&mut self, path: &str) -> io::Result<u16> {
        let data: Vec<u8> = fs::read(path)?;
        let mut cursor = Cursor::new(data);
        let origin = cursor.read_u16::<BigEndian>().unwrap();
        let mut address = origin;
        while let Ok(word) = cursor.read_u16::<BigEndian>() {
            self.data[address as usize] = word;
            address += 1;
        }
        Ok(origin)
    }

    pub fn read(&mut self, address: u16) -> u16 {
        // TODO: refactor so that there is no mem writing in this fn?
        if address == KB_STATUS {
            if check_key() {
                self.data[KB_STATUS as usize] = 1 << 15; // set MSB
                self.data[KB_DATA as usize] = get_char() as u16;
            } else {
                self.data[KB_STATUS as usize] = 0;
            }
        } else if address == DISP_STATUS {
            // set display status so program knows it can print
            self.data[DISP_STATUS as usize] = 1 << 15; // set MSB
        }
        self.data[address as usize]
    }

    pub fn write(&mut self, address: u16, data: u16) {
        // TODO: check if trying to write to an invalid addr
        match address {
            DISP_DATA => {
                /*
                 * Also known as DDR. A character written in the low byte
                 * of this register will be displayed on the screen.
                 */
                todo!();
            }
            MACHINE_CTRL => {
                /*
                 * Also known as MCR. Bit [15] is the clock enable bit.
                 * When cleared, instruction processing stops.
                 */
                todo!();
            }
            _ => {}
        }

        self.data[address as usize] = data;
    }
}

#[derive(Debug, Enum, TryFromPrimitive, Copy, Clone)]
#[repr(u16)]
pub enum Registers {
    R0 = 0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    RProgramCounter, // program counter
    RCond,           // condition flags
    RCount,
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum OpCodes {
    OpBR = 0b0000,   // branch
    OpADD = 0b0001,  // add
    OpLD = 0b0010,   // load
    OpST = 0b0011,   // store
    OpJSR = 0b0100,  // jump register
    OpAND = 0b0101,  // bitwise and
    OpLDR = 0b0110,  // load register
    OpSTR = 0b0111,  // store register
    OpRTI,           // unused
    OpNOT = 0b1001,  // bitwise not
    OpLDI = 0b1010,  // load indirect
    OpSTI = 0b1011,  // store indirect
    OpJMP = 0b1100,  // jump
    OpRES = 0b1101,  // reserved (unused) TODO: section A.4
    OpLEA,           // load effective address
    OpTRAP = 0b1111, // execute trap
}

// used to indicate the sign of the previous calculation
#[derive(Debug, TryFromPrimitive, PartialEq)]
#[repr(u16)]
pub enum CondFlags {
    Pos = 0,
    Zero,
    Neg,
}

#[derive(Debug, Enum)]
#[repr(u16)]
pub enum TrapCall {
    GETC = 0x20,
    OUT = 0x21,
    PUTS = 0x22,
    IN = 0x23,
    PUTSP = 0x24,
    HALT = 0x25,
}
impl TryFrom<u16> for TrapCall {
    type Error = ();

    fn try_from(v: u16) -> Result<Self, Self::Error> {
        match v {
            0x20 => Ok(TrapCall::GETC),
            0x21 => Ok(TrapCall::OUT),
            0x22 => Ok(TrapCall::PUTS),
            0x23 => Ok(TrapCall::IN),
            0x24 => Ok(TrapCall::PUTSP),
            0x25 => Ok(TrapCall::HALT),
            _ => Err(()),
        }
    }
}

// hardware registers
pub const KB_STATUS: u16 = 0xFE00;
pub const KB_DATA: u16 = 0xFE02;
pub const DISP_STATUS: u16 = 0xFE04;
pub const DISP_DATA: u16 = 0xFE06;
pub const MACHINE_CTRL: u16 = 0xFFFE;
