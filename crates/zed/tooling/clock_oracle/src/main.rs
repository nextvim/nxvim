use clock::{Global, Lamport, ReplicaId};
use std::{cmp::Ordering, io::{self, BufRead}};

fn main() {
    for (line_number, line) in io::stdin().lock().lines().enumerate() {
        let line = line.unwrap_or_else(|error| fail(line_number, &format!("read error: {error}")));
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields.as_slice() {
            ["replica", id] => {
                let id = ReplicaId::new(parse(id, line_number));
                println!("replica {} {}", id.as_u16(), u8::from(id.is_remote()));
            }
            ["lamport", lr, lv, rr, rv] => {
                let left = Lamport { replica_id: ReplicaId::new(parse(lr, line_number)), value: parse(lv, line_number) };
                let right = Lamport { replica_id: ReplicaId::new(parse(rr, line_number)), value: parse(rv, line_number) };
                let ordering = match left.cmp(&right) { Ordering::Less => -1, Ordering::Equal => 0, Ordering::Greater => 1 };
                println!("lamport {} {ordering}", left.as_u64());
            }
            ["global", encoded] => write_global(&parse_global(encoded, line_number)),
            ["join", left, right] => {
                let mut left = parse_global(left, line_number);
                left.join(&parse_global(right, line_number));
                write_global(&left);
            }
            ["meet", left, right] => {
                let mut left = parse_global(left, line_number);
                left.meet(&parse_global(right, line_number));
                write_global(&left);
            }
            ["relations", left, right] => {
                let left = parse_global(left, line_number);
                let right = parse_global(right, line_number);
                print!("relations {} {} {}", u8::from(left.observed_any(&right)), u8::from(left.observed_all(&right)), u8::from(left.changed_since(&right)));
                if let Some(recent) = left.most_recent() { println!(" {}:{}", recent.replica_id.as_u16(), recent.value); } else { println!(" -"); }
            }
            _ => fail(line_number, "malformed trace"),
        }
    }
}

fn parse_global(encoded: &str, line_number: usize) -> Global {
    let mut result = Global::new();
    if encoded == "-" { return result; }
    for entry in encoded.split(',') {
        let Some((id, value)) = entry.split_once(':') else { fail(line_number, "malformed vector") };
        if value.contains(':') { fail(line_number, "malformed vector"); }
        result.observe(Lamport { replica_id: ReplicaId::new(parse(id, line_number)), value: parse(value, line_number) });
    }
    result
}

fn write_global(value: &Global) {
    let values: Vec<_> = value.iter().map(|timestamp| timestamp.value.to_string()).collect();
    println!("global {}", if values.is_empty() { "-".to_string() } else { values.join(",") });
}

fn parse<T: std::str::FromStr>(value: &str, line_number: usize) -> T {
    value.parse().unwrap_or_else(|_| fail(line_number, "invalid integer"))
}

fn fail(line_number: usize, message: &str) -> ! {
    eprintln!("trace line {}: {message}", line_number + 1);
    std::process::exit(2)
}
