#[macro_use]
extern crate enum_map;

mod hardware;
use byteorder::{BigEndian, ReadBytesExt};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use enum_map::EnumMap;
use hardware::{
    CondFlags, DISP_STATUS, KB_DATA, KB_STATUS, MEM_SIZE, OpCodes, PROGRAM_COUNTER_START,
    Registers, TrapCall,
};
use std::{
    fs,
    io::{self, Cursor, Write},
    path::Path,
    time::Duration,
};

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn sign_extend(mut x: u16, bit_count: u32) -> u16 {
    // check if the sign bit (the furthest left/most significant bit) is 1, which means the number
    // is negative
    if (x >> (bit_count - 1)) & 1 != 0 {
        x |= 0xFFFF << bit_count; // fill in 1's before the value
    }
    x
}

fn check_key() -> bool {
    // check for term event
    event::poll(Duration::from_secs(0)).unwrap_or(false)
}

fn get_char() -> u8 {
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

fn mem_read(address: u16, memory_data: &mut [u16]) -> u16 {
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

fn update_flags(reg: Registers, register_data: &mut EnumMap<Registers, u16>) {
    let value = register_data[reg];
    if (value >> 15) == 1 {
        // negative is MSB is 1
        register_data[Registers::RCond] = CondFlags::Neg as u16;
    } else if value == 0 {
        register_data[Registers::RCond] = CondFlags::Zero as u16;
    } else {
        register_data[Registers::RCond] = CondFlags::Pos as u16;
    }
}

fn read_to_mem(path: &str, memory_data: &mut Vec<u16>) -> u16 {
    let data: Vec<u8> = fs::read(path).unwrap();
    let mut cursor = Cursor::new(data);

    let origin = cursor.read_u16::<BigEndian>().unwrap();

    // read the program into memory
    let mut address = origin;
    while let Ok(word) = cursor.read_u16::<BigEndian>() {
        memory_data[address as usize] = word;
        address += 1;
    }

    origin
}

fn main() {
    enable_raw_mode().unwrap();
    let _guard = RawModeGuard; // when this gets dropped, raw mode is disabled (including on panic)

    let mut register_data = enum_map! {
        // default all registers to 0
        Registers::RCond => CondFlags::Zero as u16,
        Registers::RProgramCounter => PROGRAM_COUNTER_START,
        _ => 0
    };

    // let mut memory_data: [u16; MEM_SIZE as usize] = [];
    // let mut memory_data: Vec<u16> = Vec::with_capacity(MEM_SIZE as usize);
    let mut memory_data: Vec<u16> = vec![0; MEM_SIZE as usize];
    register_data[Registers::RProgramCounter] = read_to_mem("prog/2048.obj", &mut memory_data);

    // for (key, &value) in &register_data {
    //     print!("{:?} has {} as value.\r\n", key, value);
    // }

    loop {
        let op_data = {
            let pc = register_data[Registers::RProgramCounter];
            register_data[Registers::RProgramCounter] = pc.wrapping_add(1);
            memory_data[pc as usize]
        };
        let opcode_val = (op_data >> 12) as u8; // op code is the first 4 bits of a 16-bit word

        // both of these should get rid of the first 4 bits
        // let opcode_args = (op_data & OP_ARG_MASK) as u16;
        // let opcode_args = ((op_data << 4) >> 4) as u16;

        let op = OpCodes::try_from(opcode_val).unwrap();

        // print!(
        //     "\r\nOP DATA 0b{:b}; 0x{:x} -- {:?}\r\n",
        //     op_data, op_data, op
        // );

        // https://www.jmeiners.com/lc3-vm/supplies/lc3-isa.pdf
        match op {
            OpCodes::OpBR => {
                // branch
                // println!("BRANCH");
                let n = (op_data & 0b0000_1000_0000_0000) != 0;
                let z = (op_data & 0b0000_0100_0000_0000) != 0;
                let p = (op_data & 0b0000_0010_0000_0000) != 0;
                let pc_offset = sign_extend(op_data & 0b0000_0001_1111_1111, 9) as i16;

                // println!("N, Z, P, OFFSET: {}, {}, {}, {}", n, z, p, pc_offset);

                // NOTE: this should never throw because this is always set to a flag value
                let condition_flag = CondFlags::try_from(register_data[Registers::RCond]).unwrap();
                let do_branch = (n && condition_flag == CondFlags::Neg)
                    || (z && condition_flag == CondFlags::Zero)
                    || (p && condition_flag == CondFlags::Pos);
                if do_branch
                // || (!n && !z && !p)
                {
                    // println!("DO BRANCH");
                    let current_pc = register_data[Registers::RProgramCounter] as i32;
                    let new_pc = current_pc.wrapping_add(pc_offset as i32) as u16;
                    register_data[Registers::RProgramCounter] = new_pc;
                    // println!("NEW PC, {}", new_pc)
                    // register_data[Registers::RProgramCounter] =
                    //     register_data[Registers::RProgramCounter].wrapping_add(pc_offset);
                }
            }
            OpCodes::OpJMP => {
                // jump or return
                let base_r = (op_data >> 6) & 0b111;
                if base_r == 0b111 {
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
                    let pc_offset = sign_extend(op_data & 0b0000_0111_1111_1111, 11);
                    let pc = register_data[Registers::RProgramCounter];
                    register_data[Registers::RProgramCounter] = pc.wrapping_add(pc_offset);
                } else {
                    // BaseR
                    let reg = Registers::try_from((op_data >> 6) & 0b111).unwrap();
                    register_data[Registers::RProgramCounter] = register_data[reg];
                }
            }
            OpCodes::OpST => {
                let sr = (op_data >> 9) & 0b111;
                let pc_offset = sign_extend(op_data & 0b0000_0001_1111_1111, 9);
                let reg = Registers::try_from(sr).unwrap();
                let reg_data = register_data[reg];
                memory_data
                    [register_data[Registers::RProgramCounter].wrapping_add(pc_offset) as usize] =
                    reg_data;
            }
            OpCodes::OpLDI => {
                // load indirect
                let dr = (op_data >> 9) & 0b111;
                let pc_offset = sign_extend(op_data & 0b0000_0001_1111_1111, 9) as i16;
                let mem_index = mem_read(
                    register_data[Registers::RProgramCounter].wrapping_add(pc_offset as u16),
                    &mut memory_data,
                );
                let value = mem_read(mem_index, &mut memory_data);
                let reg = Registers::try_from(dr).unwrap();
                register_data[reg] = value;
                update_flags(reg, &mut register_data);
            }
            OpCodes::OpLEA => {
                // load effective addr
                let dr = (op_data >> 9) & 0b111;
                let pc_offset = sign_extend(op_data & 0b1_1111_1111, 9);
                let value = register_data[Registers::RProgramCounter].wrapping_add(pc_offset);
                let reg = Registers::try_from(dr).unwrap();
                register_data[reg] = value;
                update_flags(reg, &mut register_data);
            }
            OpCodes::OpLDR => {
                // load base + offset
                let offset = sign_extend(op_data & 0b11_1111, 6);

                let baser = (op_data >> 6) & 0b111;
                let baser = Registers::try_from(baser).unwrap();

                let mut address = register_data[baser];
                // print!("READ VALUE {} FROM BASE REG {:?} \r\n", address, baser);
                address = address.wrapping_add(offset);
                // print!("FINAL ADDRESS IS {} AFTER OFFSET {} \r\n", address, offset);

                let data = memory_data[address as usize];

                let dr = (op_data >> 9) & 0b111;
                let dr = Registers::try_from(dr).unwrap();
                register_data[dr] = data;
                update_flags(dr, &mut register_data);

                // print!("WROTE VALUE {} TO REG {:?} \r\n", register_data[dr], dr);
            }
            OpCodes::OpSTI => {
                // println!("STI");
                let sr = (op_data >> 9) & 0b111;
                let pc_offset = sign_extend(op_data & 0b0000_0001_1111_1111, 9);

                // println!("SR, OFFSET: {}, {}", sr, pc_offset);

                let reg = Registers::try_from(sr).unwrap();
                let reg_data = register_data[reg];
                let mem_index =
                    memory_data[(register_data[Registers::RProgramCounter] + pc_offset) as usize];
                memory_data[mem_index as usize] = reg_data;
            }
            OpCodes::OpLD => {
                // load
                let dr = (op_data >> 9) & 0b111;
                let pc_offset = sign_extend(op_data & 0b0000_0001_1111_1111, 9) as i16;
                let load_addr =
                    register_data[Registers::RProgramCounter].wrapping_add(pc_offset as u16);
                let value = mem_read(load_addr, &mut memory_data);
                let reg = Registers::try_from(dr).unwrap();
                register_data[reg] = value;
                update_flags(reg, &mut register_data);
            }
            OpCodes::OpTRAP => {
                // TODO
                register_data[Registers::R7] = register_data[Registers::RProgramCounter];
                let trapvect8 = op_data & 0b0000_0000_1111_1111;

                // print!("TRAP VECT 0x{:x}\r\n", trapvect8);

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
                                // print!("PUTS FROM ADDR {}\r\n", pointer);
                                loop {
                                    let data = mem_read(pointer, &mut memory_data);
                                    if data == 0x0000 {
                                        break;
                                    }
                                    let c = data as u8 as char;
                                    // print!("{}", data as u8 as char);
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
                            _ => {
                                panic!("UNKNOWN TRAP CALL {:#06}\r\n", trapvect8);
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
                let dr = (op_data >> 9) & 0b111;
                let reg1 = register_data[Registers::try_from((op_data >> 6) & 0b111).unwrap()];
                let value: u16;
                if mode {
                    let imm5 = sign_extend(op_data & 0b1_1111, 5);
                    // TODO: getting overflow error here
                    // value = reg1 + imm5;
                    value = reg1.wrapping_add(imm5);
                    // panic!("REG 1: {}, IMM 5: {}, VALUE: {}", reg1, imm5, value);
                } else {
                    let reg2 = register_data[Registers::try_from(op_data & 0b111).unwrap()];
                    // TODO: attempt to add with overflow
                    // value = reg1 + reg2;
                    value = reg1.wrapping_add(reg2);
                    // panic!("REG 1: {}, REG 2: {}, VALUE: {}", reg1, reg2, value);
                }
                let final_reg = Registers::try_from(dr).unwrap();
                register_data[final_reg] = value;
                update_flags(final_reg, &mut register_data);

                // print!("WROTE VALUE {} TO REG {:?} \r\n", value, final_reg);
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
                let dr = (op_data >> 9) & 0b111;
                let reg1 = register_data[Registers::try_from((op_data >> 6) & 0b111).unwrap()];
                let value: u16;
                if mode {
                    let imm5 = sign_extend(op_data & 0b1_1111, 5);
                    value = reg1 & imm5;
                } else {
                    let reg2 = register_data[Registers::try_from(op_data & 0b111).unwrap()];
                    value = reg1 & reg2;
                }
                let final_reg = Registers::try_from(dr).unwrap();
                register_data[final_reg] = value;
                update_flags(final_reg, &mut register_data);
            }
            OpCodes::OpNOT => {
                /*
                 * The bit-wise complement of the contents of SR is stored in DR.
                 * The condition codes are set, based on whether the binary value produced, taken as a 2’s
                 * complement integer, is negative, zero, or positive.
                 */
                let dr = (op_data >> 9) & 0b111;
                let dr = Registers::try_from(dr).unwrap();

                let sr = (op_data >> 6) & 0b111;
                let sr = Registers::try_from(sr).unwrap();

                register_data[dr] = !register_data[sr];
                update_flags(dr, &mut register_data);
            }
            OpCodes::OpSTR => {
                let sr = (op_data >> 9) & 0b111;
                let baser = (op_data >> 6) & 0b111;
                let offset6 = sign_extend(op_data & 0b11_1111, 6) as i16;

                let base_reg = Registers::try_from(baser).unwrap();
                let base_reg_data = register_data[base_reg];

                let final_reg = Registers::try_from(sr).unwrap();
                let final_reg_data = register_data[final_reg];
                // TODO: i dont think this should be cast again to u16, that just makes it always
                // positive
                memory_data[base_reg_data.wrapping_add(offset6 as u16) as usize] = final_reg_data;
            }
            _ => {
                panic!("UNKNOWN OP CODE {:?}\r\n", op);
            }
        }
    }

    disable_raw_mode().unwrap();
}
