//! `trestle <file.trsl>` — parse a Trestle source file and print its AST.
//!
//! Phase 1 only parses; it does not yet evaluate. The path argument is
//! required so the binary stays decoupled from any particular corpus layout.

use std::{env, fs, process};

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: trestle <file.trsl>");
        process::exit(2);
    };

    let src = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        process::exit(1);
    });

    match trestle::parse(&src) {
        Ok(program) => println!("{program:#?}"),
        Err(e) => {
            eprintln!("parse error:\n{e}");
            process::exit(1);
        }
    }
}
