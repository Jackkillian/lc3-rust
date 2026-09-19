#[macro_use]
extern crate enum_map;

mod hardware;
mod utils;
mod vm;
use vm::{State, VM};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::{env, process::exit};

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        println!();
    }
}

struct Config {
    path: String,
}

impl Config {
    fn new(mut args: impl Iterator<Item = String>) -> Result<Self, &'static str> {
        // ignore path to binary
        args.next();

        let path = match args.next() {
            Some(arg) => arg,
            None => return Err("path to lc3 obj not specified"),
        };

        Ok(Config { path })
    }
}

fn main() {
    let config = Config::new(env::args()).unwrap_or_else(|err| {
        eprintln!("Error: {}", err);
        exit(1);
    });

    let mut vm = VM::new();
    match vm.read_file(&config.path) {
        Err(err) => {
            eprintln!("Error: could not read file: {}", err);
            exit(1);
        }
        _ => {}
    }

    enable_raw_mode().unwrap();
    let _guard = RawModeGuard; // when this gets dropped, raw mode is disabled (including on panic)

    loop {
        if let State::HALTED = vm.step() {
            break;
        }
    }
}
