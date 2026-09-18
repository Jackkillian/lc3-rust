#[macro_use]
extern crate enum_map;

mod hardware;
use hardware::{CondFlags, MEM_SIZE, OpCodes, Registers, TrapCall};

mod utils;
use utils::{
    MASK_IMM5, MASK_OFFSET6, MASK_REG, MASK_SE_9, MASK_SE_11, get_char, mem_read, sign_extend,
    update_flags,
};

use byteorder::{BigEndian, ReadBytesExt};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::{
    env, fs,
    io::{self, Cursor, Write},
    process::exit,
};

use crate::utils::mem_write;

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        println!();
    }
}

fn read_to_mem(path: &str, memory_data: &mut Vec<u16>) -> io::Result<u16> {
    let data: Vec<u8> = fs::read(path)?;
    let mut cursor = Cursor::new(data);
    let origin = cursor.read_u16::<BigEndian>().unwrap();
    let mut address = origin;
    while let Ok(word) = cursor.read_u16::<BigEndian>() {
        memory_data[address as usize] = word;
        address += 1;
    }
    Ok(origin)
}

// Config
struct Config {
    path: String,
}
impl Config {
    fn new(path: String) -> Self {
        Config { path }
    }
}
fn parse_config(mut args: impl Iterator<Item = String>) -> Result<Config, &'static str> {
    // ignore path to binary
    args.next();

    let path = match args.next() {
        Some(arg) => arg,
        None => return Err("path to lc3 obj not specified"),
    };

    Ok(Config::new(path))
}

fn main() {
    let config = parse_config(env::args()).unwrap_or_else(|err| {
        eprintln!("Error: {err}");
        exit(1);
    });

    let mut register_data = enum_map! {
        // default all registers to 0
        Registers::RCond => CondFlags::Zero as u16,
        _ => 0
    };

    let mut memory_data: Vec<u16> = vec![0; MEM_SIZE as usize];
    register_data[Registers::RProgramCounter] = match read_to_mem(&config.path, &mut memory_data) {
        Ok(origin) => origin,
        _ => {
            eprintln!("Error: could not read file {}", config.path);
            exit(1);
        }
    };

    // for (key, &value) in &register_data {
    //     print!("{:?} has {} as value.\r\n", key, value);
    // }

    enable_raw_mode().unwrap();
    let _guard = RawModeGuard; // when this gets dropped, raw mode is disabled (including on panic)

    loop {
        let op_data = {
            let pc = register_data[Registers::RProgramCounter];
            register_data[Registers::RProgramCounter] = pc.wrapping_add(1);
            mem_read(pc, &mut memory_data)
        };

        // TODO: maybe don't use an enum, just inline bit values
        let op = OpCodes::try_from(
            (op_data >> 12) as u8, // op code is the first 4 bits of a 16-bit word
        )
        .unwrap();

        match op {
            OpCodes::OpBR => {
                // branch
                let n = ((op_data >> 11) & 1) != 0;
                let z = ((op_data >> 10) & 1) != 0;
                let p = ((op_data >> 9) & 1) != 0;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9) as i16;

                // NOTE: this should never throw because this is always set to a flag value
                let condition_flag = CondFlags::try_from(register_data[Registers::RCond]).unwrap();
                if (n && condition_flag == CondFlags::Neg)
                    || (z && condition_flag == CondFlags::Zero)
                    || (p && condition_flag == CondFlags::Pos)
                {
                    let current_pc = register_data[Registers::RProgramCounter] as i32;
                    let new_pc = current_pc.wrapping_add(pc_offset as i32) as u16;
                    register_data[Registers::RProgramCounter] = new_pc;
                }
            }
            OpCodes::OpJMP => {
                // jump or return
                let base_r = (op_data >> 6) & MASK_REG;
                if base_r == MASK_REG {
                    // return
                    register_data[Registers::RProgramCounter] = register_data[Registers::R7];
                } else {
                    // jump
                    let reg = Registers::try_from(base_r).unwrap();
                    register_data[Registers::RProgramCounter] = register_data[reg];
                }
            }
            OpCodes::OpJSR => {
                let mode = ((op_data >> 11) & 0b1) == 1;
                register_data[Registers::R7] = register_data[Registers::RProgramCounter];
                if mode {
                    // PCOffset
                    let pc_offset = sign_extend(op_data & MASK_SE_11, 11);
                    let pc = register_data[Registers::RProgramCounter];
                    register_data[Registers::RProgramCounter] = pc.wrapping_add(pc_offset);
                } else {
                    // BaseR
                    let reg = Registers::try_from((op_data >> 6) & MASK_REG).unwrap();
                    register_data[Registers::RProgramCounter] = register_data[reg];
                }
            }
            OpCodes::OpST => {
                let sr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9);
                let reg = Registers::try_from(sr).unwrap();
                let reg_data = register_data[reg];

                mem_write(
                    register_data[Registers::RProgramCounter].wrapping_add(pc_offset),
                    &mut memory_data,
                    reg_data,
                );
            }
            OpCodes::OpLDI => {
                // load indirect
                let dr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9) as i16;
                let mem_index = mem_read(
                    register_data[Registers::RProgramCounter].wrapping_add(pc_offset as u16),
                    &mut memory_data,
                );
                let value = mem_read(mem_index, &mut memory_data);
                let reg = Registers::try_from(dr).unwrap();
                register_data[reg] = value;
                update_flags(value, &mut register_data);
            }
            OpCodes::OpLEA => {
                // load effective addr
                let dr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9);
                let value = register_data[Registers::RProgramCounter].wrapping_add(pc_offset);
                let reg = Registers::try_from(dr).unwrap();
                register_data[reg] = value;
                update_flags(value, &mut register_data);
            }
            OpCodes::OpLDR => {
                // load base + offset
                let offset = sign_extend(op_data & MASK_OFFSET6, 6);

                let baser = (op_data >> 6) & MASK_REG;
                let baser = Registers::try_from(baser).unwrap();

                let mut address = register_data[baser];
                address = address.wrapping_add(offset);

                let data = mem_read(address, &mut memory_data);

                let dr = (op_data >> 9) & MASK_REG;
                let dr = Registers::try_from(dr).unwrap();
                register_data[dr] = data;
                update_flags(data, &mut register_data);
            }
            OpCodes::OpSTI => {
                let sr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9);

                let reg = Registers::try_from(sr).unwrap();
                let reg_data = register_data[reg];

                let address = mem_read(
                    register_data[Registers::RProgramCounter] + pc_offset,
                    &mut memory_data,
                );

                mem_write(address, &mut memory_data, reg_data);
            }
            OpCodes::OpLD => {
                // load
                let dr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9) as i16;
                let load_addr =
                    register_data[Registers::RProgramCounter].wrapping_add(pc_offset as u16);
                let value = mem_read(load_addr, &mut memory_data);
                let reg = Registers::try_from(dr).unwrap();
                register_data[reg] = value;
                update_flags(value, &mut register_data);
            }
            OpCodes::OpTRAP => {
                // TODO
                register_data[Registers::R7] = register_data[Registers::RProgramCounter];
                let trapvect8 = op_data & 0xFF; // get last 8 bits of the data

                let result = TrapCall::try_from(trapvect8);
                match result {
                    Ok(call) => {
                        match call {
                            TrapCall::GETC => {
                                /*
                                 * Read a single character from the keyboard. The character is not echoed onto the
                                 * console. Its ASCII code is copied into R0. The high eight bits of R0 are cleared.
                                 */
                                register_data[Registers::R0] = get_char() as u16;
                            }
                            TrapCall::OUT => {
                                //  Write a character in R0[7:0] to the console display.
                                let c = register_data[Registers::R0] as u8;
                                // unsafe { print!("{}", char::from_u32_unchecked(c as u32)) }

                                // TODO: custom \r\n check because we're in raw mode?
                                if c as char == '\n' {
                                    print!("\r\n");
                                } else {
                                    print!("{}", c as char);
                                }
                                io::stdout().flush().unwrap();
                            }
                            TrapCall::HALT => {
                                // flush stdout before halting
                                io::stdout().flush().unwrap();
                                break;
                            }
                            TrapCall::PUTS => {
                                /*
                                 * Write a string of ASCII characters to the console display. The characters are contained
                                 * in consecutive memory locations, one character per memory location, starting with
                                 * the address specified in R0. Writing terminates with the occurrence of x0000 in a
                                 * memory location.
                                 */
                                let mut pointer = register_data[Registers::R0];
                                loop {
                                    let data = mem_read(pointer, &mut memory_data);
                                    if data == 0x0000 {
                                        break;
                                    }
                                    let c = data as u8 as char;
                                    // TODO: custom \r\n check because we're in raw mode?
                                    if c == '\n' {
                                        print!("\r\n");
                                    } else {
                                        print!("{}", c);
                                    }
                                    pointer = pointer.wrapping_add(1);
                                }
                                io::stdout().flush().unwrap();
                            }
                            TrapCall::IN => {
                                /*
                                 * Print a prompt on the screen and read a single character from the keyboard. The
                                 * character is echoed onto the console monitor, and its ASCII code is copied into R0.
                                 * The high eight bits of R0 are cleared.
                                 */
                                print!("Input a character: ");
                                io::stdout().flush().unwrap();
                                let c = get_char();
                                // TODOD: currently appends a newline to match lc3sim
                                print!("{}\r\n", c as char);
                                register_data[Registers::R0] = (c as u16) & 0xFF;
                            }
                            TrapCall::PUTSP => {
                                /*
                                 * Write a string of ASCII characters to the console. The characters are contained in
                                 * consecutive memory locations, two characters per memory location, starting with the
                                 * address specified in R0. The ASCII code contained in bits [7:0] of a memory location
                                 * is written to the console first. Then the ASCII code contained in bits [15:8] of that
                                 * memory location is written to the console. (A character string consisting of an odd
                                 * number of characters to be written will have x00 in bits [15:8] of the memory
                                 * location containing the last character to be written.) Writing terminates with the
                                 * occurrence of x0000 in a memory location.
                                 */
                                let mut pointer = register_data[Registers::R0];
                                loop {
                                    let data = mem_read(pointer, &mut memory_data);
                                    if data == 0x0000 {
                                        break;
                                    }
                                    let char1 = (data & 0xFF) as u8 as char;
                                    let char2 = (data >> 8) as u8 as char;
                                    // TODO: custom \r\n check because we're in raw mode?
                                    if char1 == '\n' {
                                        print!("\r\n");
                                    } else {
                                        print!("{}", char1);
                                    }
                                    if char2 == '\n' {
                                        print!("\r\n");
                                    } else {
                                        print!("{}", char2);
                                    }
                                    pointer = pointer.wrapping_add(1);
                                }
                                io::stdout().flush().unwrap();
                            }
                        }
                    }
                    Err(e) => {
                        panic!("UNKNOWN TRAP CALL {:#06}     ERR: {:?}\r\n", trapvect8, e);
                    }
                }

                /*
                 * NOTE: TODO: Memory locations x0000 through x00FF, 256 in all, are available to contain
                 * starting addresses for system calls specified by their corresponding trap vectors.
                 * This region of memory is called the Trap Vector Table. Table A.2 describes the
                 * functions performed by the service routines corresponding to trap vectors x20
                 * to x25.
                 */
            }
            OpCodes::OpADD => {
                let mode = ((op_data >> 5) & 0b1) == 1;
                let dr = (op_data >> 9) & MASK_REG;
                let reg1 = register_data[Registers::try_from((op_data >> 6) & MASK_REG).unwrap()];
                let value: u16;
                if mode {
                    let imm5 = sign_extend(op_data & MASK_IMM5, 5);
                    // TODO: getting overflow error here
                    // value = reg1 + imm5;
                    value = reg1.wrapping_add(imm5);
                    // panic!("REG 1: {}, IMM 5: {}, VALUE: {}", reg1, imm5, value);
                } else {
                    let reg2 = register_data[Registers::try_from(op_data & MASK_REG).unwrap()];
                    // TODO: attempt to add with overflow
                    // value = reg1 + reg2;
                    value = reg1.wrapping_add(reg2);
                }
                let final_reg = Registers::try_from(dr).unwrap();
                register_data[final_reg] = value;
                update_flags(value, &mut register_data);
            }
            OpCodes::OpAND => {
                /*
                 * If bit [5] is 0, the second source operand is obtained from SR2. If bit [5] is 1,
                 * the second source operand is obtained by sign-extending the imm5 field to 16
                 * bits. In either case, the second source operand and the contents of SR1 are bitwise ANDed,
                 * and the result stored in DR. The condition codes are set, based on
                 * whether the binary value produced, taken as a 2’s complement integer, is negative,
                 * zero, or positive.
                 */
                let mode = ((op_data >> 5) & 0b1) == 1;
                let dr = (op_data >> 9) & MASK_REG;
                let reg1 = register_data[Registers::try_from((op_data >> 6) & MASK_REG).unwrap()];
                let value: u16;
                if mode {
                    let imm5 = sign_extend(op_data & MASK_IMM5, 5);
                    value = reg1 & imm5;
                } else {
                    let reg2 = register_data[Registers::try_from(op_data & MASK_REG).unwrap()];
                    value = reg1 & reg2;
                }
                let final_reg = Registers::try_from(dr).unwrap();
                register_data[final_reg] = value;
                update_flags(value, &mut register_data);
            }
            OpCodes::OpNOT => {
                /*
                 * The bit-wise complement of the contents of SR is stored in DR.
                 * The condition codes are set, based on whether the binary value produced, taken as a 2’s
                 * complement integer, is negative, zero, or positive.
                 */
                let dr = (op_data >> 9) & MASK_REG;
                let dr = Registers::try_from(dr).unwrap();

                let sr = (op_data >> 6) & MASK_REG;
                let sr = Registers::try_from(sr).unwrap();

                let value = !register_data[sr];
                register_data[dr] = value;
                update_flags(value, &mut register_data);
            }
            OpCodes::OpSTR => {
                let sr = (op_data >> 9) & MASK_REG;
                let baser = (op_data >> 6) & MASK_REG;
                let offset6 = sign_extend(op_data & MASK_OFFSET6, 6);

                let base_reg = Registers::try_from(baser).unwrap();
                let base_reg_data = register_data[base_reg];

                let final_reg = Registers::try_from(sr).unwrap();
                let final_reg_data = register_data[final_reg];

                mem_write(
                    base_reg_data.wrapping_add(offset6),
                    &mut memory_data,
                    final_reg_data,
                );
            }
            OpCodes::OpRTI => todo!(),
            OpCodes::OpRES => todo!(),
        }
    }
}
