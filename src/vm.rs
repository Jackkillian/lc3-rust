use crate::hardware::{CondFlags, Memory, OpCodes, Registers, TrapCall};
use crate::utils::{
    MASK_IMM5, MASK_OFFSET6, MASK_REG, MASK_SE_9, MASK_SE_11, get_char, sign_extend, update_flags,
};
use enum_map::EnumMap;
use std::io::{self, Write};

pub enum State {
    INITIALIZED,
    READY,
    RUNNING,
    HALTED,
}

pub struct VM {
    state: State,
    mem: Memory,
    reg: EnumMap<Registers, u16>,
}

impl VM {
    pub fn new() -> Self {
        Self {
            state: State::INITIALIZED,
            mem: Memory::new(),
            reg: enum_map! {
                Registers::RCond => CondFlags::Zero as u16,
                // default all registers to 0
                _ => 0
            },
        }
    }

    pub fn read_file(&mut self, path: &str) -> io::Result<()> {
        let origin = self.mem.read_file(path)?;
        self.reg[Registers::RProgramCounter] = origin;
        self.state = State::READY;
        Ok(())
    }

    pub fn print_registers(&self) {
        for (key, &value) in &self.reg {
            print!("REG {:?}: {}\r\n", key, value);
        }
    }

    pub fn get_state(&self) -> &State {
        &self.state
    }

    pub fn step(&mut self) -> &State {
        match self.state {
            State::READY => {
                self.state = State::RUNNING;
            }
            State::HALTED => {
                // stop execution on halt
                return &self.state;
            }
            _ => {}
        }

        let op_data = {
            let pc = self.reg[Registers::RProgramCounter];
            self.reg[Registers::RProgramCounter] = pc.wrapping_add(1);
            self.mem.read(pc)
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
                let condition_flag = CondFlags::try_from(self.reg[Registers::RCond]).unwrap();
                if (n && condition_flag == CondFlags::Neg)
                    || (z && condition_flag == CondFlags::Zero)
                    || (p && condition_flag == CondFlags::Pos)
                {
                    let current_pc = self.reg[Registers::RProgramCounter] as i32;
                    let new_pc = current_pc.wrapping_add(pc_offset as i32) as u16;
                    self.reg[Registers::RProgramCounter] = new_pc;
                }
            }
            OpCodes::OpJMP => {
                // jump or return
                let base_r = (op_data >> 6) & MASK_REG;
                if base_r == MASK_REG {
                    // return
                    self.reg[Registers::RProgramCounter] = self.reg[Registers::R7];
                } else {
                    // jump
                    let reg = Registers::try_from(base_r).unwrap();
                    self.reg[Registers::RProgramCounter] = self.reg[reg];
                }
            }
            OpCodes::OpJSR => {
                let mode = ((op_data >> 11) & 0b1) == 1;
                self.reg[Registers::R7] = self.reg[Registers::RProgramCounter];
                if mode {
                    // PCOffset
                    let pc_offset = sign_extend(op_data & MASK_SE_11, 11);
                    let pc = self.reg[Registers::RProgramCounter];
                    self.reg[Registers::RProgramCounter] = pc.wrapping_add(pc_offset);
                } else {
                    // BaseR
                    let reg = Registers::try_from((op_data >> 6) & MASK_REG).unwrap();
                    self.reg[Registers::RProgramCounter] = self.reg[reg];
                }
            }
            OpCodes::OpST => {
                let sr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9);
                let reg = Registers::try_from(sr).unwrap();
                let reg_data = self.reg[reg];

                self.mem.write(
                    self.reg[Registers::RProgramCounter].wrapping_add(pc_offset),
                    reg_data,
                );
            }
            OpCodes::OpLDI => {
                // load indirect
                let dr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9) as i16;
                let mem_index = self
                    .mem
                    .read(self.reg[Registers::RProgramCounter].wrapping_add(pc_offset as u16));
                let value = self.mem.read(mem_index);
                let reg = Registers::try_from(dr).unwrap();
                self.reg[reg] = value;
                update_flags(value, &mut self.reg);
            }
            OpCodes::OpLEA => {
                // load effective addr
                let dr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9);
                let value = self.reg[Registers::RProgramCounter].wrapping_add(pc_offset);
                let reg = Registers::try_from(dr).unwrap();
                self.reg[reg] = value;
                update_flags(value, &mut self.reg);
            }
            OpCodes::OpLDR => {
                // load base + offset
                let offset = sign_extend(op_data & MASK_OFFSET6, 6);

                let baser = (op_data >> 6) & MASK_REG;
                let baser = Registers::try_from(baser).unwrap();

                let mut address = self.reg[baser];
                address = address.wrapping_add(offset);

                let data = self.mem.read(address);

                let dr = (op_data >> 9) & MASK_REG;
                let dr = Registers::try_from(dr).unwrap();
                self.reg[dr] = data;
                update_flags(data, &mut self.reg);
            }
            OpCodes::OpSTI => {
                let sr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9);

                let reg = Registers::try_from(sr).unwrap();
                let reg_data = self.reg[reg];

                let address = self
                    .mem
                    .read(self.reg[Registers::RProgramCounter] + pc_offset);

                self.mem.write(address, reg_data);
            }
            OpCodes::OpLD => {
                // load
                let dr = (op_data >> 9) & MASK_REG;
                let pc_offset = sign_extend(op_data & MASK_SE_9, 9) as i16;
                let load_addr = self.reg[Registers::RProgramCounter].wrapping_add(pc_offset as u16);
                let value = self.mem.read(load_addr);
                let reg = Registers::try_from(dr).unwrap();
                self.reg[reg] = value;
                update_flags(value, &mut self.reg);
            }
            OpCodes::OpTRAP => {
                self.reg[Registers::R7] = self.reg[Registers::RProgramCounter];
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
                                self.reg[Registers::R0] = get_char() as u16;
                            }
                            TrapCall::OUT => {
                                /*
                                 * Write a character in R0[7:0] to the console display.
                                 */
                                let c = self.reg[Registers::R0] as u8;
                                // TODO: custom \r\n check because we're in raw mode?
                                if c as char == '\n' {
                                    print!("\r\n");
                                } else {
                                    print!("{}", c as char);
                                }
                                io::stdout().flush().unwrap();
                            }
                            TrapCall::HALT => {
                                /*
                                 * Halt execution and print a message on the console.
                                 */
                                print!("\r\n\nVM exited.\r\n");
                                io::stdout().flush().unwrap();
                                self.state = State::HALTED;
                                return &self.state;
                            }
                            TrapCall::PUTS => {
                                /*
                                 * Write a string of ASCII characters to the console display. The characters are contained
                                 * in consecutive self.mem.locations, one character per memory location, starting with
                                 * the address specified in R0. Writing terminates with the occurrence of x0000 in a
                                 * self.mem.location.
                                 */
                                let mut pointer = self.reg[Registers::R0];
                                loop {
                                    let data = self.mem.read(pointer);
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
                                self.reg[Registers::R0] = (c as u16) & 0xFF;
                            }
                            TrapCall::PUTSP => {
                                /*
                                 * Write a string of ASCII characters to the console. The characters are contained in
                                 * consecutive self.mem.locations, two characters per memory location, starting with the
                                 * address specified in R0. The ASCII code contained in bits [7:0] of a self.mem.location
                                 * is written to the console first. Then the ASCII code contained in bits [15:8] of that
                                 * self.mem.location is written to the console. (A character string consisting of an odd
                                 * number of characters to be written will have x00 in bits [15:8] of the memory
                                 * location containing the last character to be written.) Writing terminates with the
                                 * occurrence of x0000 in a self.mem.location.
                                 */
                                let mut pointer = self.reg[Registers::R0];
                                loop {
                                    let data = self.mem.read(pointer);
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
                 * NOTE: TODO: self.mem.locations x0000 through x00FF, 256 in all, are available to contain
                 * starting addresses for system calls specified by their corresponding trap vectors.
                 * This region of self.mem.is called the Trap Vector Table. Table A.2 describes the
                 * functions performed by the service routines corresponding to trap vectors x20
                 * to x25.
                 */
            }
            OpCodes::OpADD => {
                let mode = ((op_data >> 5) & 0b1) == 1;
                let dr = (op_data >> 9) & MASK_REG;
                let reg1 = self.reg[Registers::try_from((op_data >> 6) & MASK_REG).unwrap()];
                let value: u16;
                if mode {
                    let imm5 = sign_extend(op_data & MASK_IMM5, 5);
                    value = reg1.wrapping_add(imm5);
                } else {
                    let reg2 = self.reg[Registers::try_from(op_data & MASK_REG).unwrap()];
                    value = reg1.wrapping_add(reg2);
                }
                let final_reg = Registers::try_from(dr).unwrap();
                self.reg[final_reg] = value;
                update_flags(value, &mut self.reg);
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
                let reg1 = self.reg[Registers::try_from((op_data >> 6) & MASK_REG).unwrap()];
                let value: u16;
                if mode {
                    let imm5 = sign_extend(op_data & MASK_IMM5, 5);
                    value = reg1 & imm5;
                } else {
                    let reg2 = self.reg[Registers::try_from(op_data & MASK_REG).unwrap()];
                    value = reg1 & reg2;
                }
                let final_reg = Registers::try_from(dr).unwrap();
                self.reg[final_reg] = value;
                update_flags(value, &mut self.reg);
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

                let value = !self.reg[sr];
                self.reg[dr] = value;
                update_flags(value, &mut self.reg);
            }
            OpCodes::OpSTR => {
                let sr = (op_data >> 9) & MASK_REG;
                let baser = (op_data >> 6) & MASK_REG;
                let offset6 = sign_extend(op_data & MASK_OFFSET6, 6);

                let base_reg = Registers::try_from(baser).unwrap();
                let base_reg_data = self.reg[base_reg];

                let final_reg = Registers::try_from(sr).unwrap();
                let final_reg_data = self.reg[final_reg];

                self.mem
                    .write(base_reg_data.wrapping_add(offset6), final_reg_data);
            }
            OpCodes::OpRTI => todo!(),
            OpCodes::OpRES => todo!(),
        }

        &self.state
    }
}
