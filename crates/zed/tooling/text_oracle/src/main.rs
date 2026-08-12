use std::io::{self, BufRead};

const INITIAL_STATE: &str =
    "state version=1 text=- version-vector=- operations=0 deferred=0 history=0";

fn main() {
    for (line_number, line) in io::stdin().lock().lines().enumerate() {
        let line = line.unwrap_or_else(|error| fail(line_number, &format!("read error: {error}")));
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields: Vec<_> = line.split_whitespace().collect();
        match fields.as_slice() {
            ["emit"] => println!("{INITIAL_STATE}"),
            _ => fail(line_number, "malformed trace"),
        }
    }
}

fn fail(line_number: usize, message: &str) -> ! {
    eprintln!("trace line {}: {message}", line_number + 1);
    std::process::exit(2)
}
