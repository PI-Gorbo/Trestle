use miette::Result;
use std::{env, fs, process};

fn main() -> Result<()> {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: trestle <file.trsl>");
        process::exit(2);
    };

    let src = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        process::exit(1);
    });

    trestle::parse(&src)?;

    Ok(())
}
