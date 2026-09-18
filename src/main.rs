mod flatten;
mod parser;

use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: yaml-flatten <file.yaml|->");
            return ExitCode::from(2);
        }
    };

    let input = match read_input(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("yaml-flatten: {path}: {e}");
            return ExitCode::from(1);
        }
    };

    let value = match parser::parse(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("yaml-flatten: parse error: {e}");
            return ExitCode::from(1);
        }
    };

    for (key, val) in flatten::flatten(&value) {
        println!("{key}={val}");
    }

    ExitCode::SUCCESS
}

fn read_input(path: &str) -> io::Result<String> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        fs::read_to_string(path)
    }
}
