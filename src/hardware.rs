use enum_map::Enum;
use num_enum::TryFromPrimitive;

pub const MEM_SIZE: u32 = 1 << 16; // 2^16

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
