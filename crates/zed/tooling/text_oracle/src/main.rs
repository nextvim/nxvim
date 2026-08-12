use std::collections::BTreeMap;
use std::io::{self, Read};

use clock::Global;
use text::{Anchor, Bias, Buffer, BufferId, LineEnding, Operation, ReplicaId, ToOffset};

const INITIAL_STATE: &str =
    "state version=1 text=- version-vector=- operations=0 deferred=0 history=0";

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Error {
    MalformedTrace,
    UnsupportedVersion,
    InvalidUtf8,
    InvalidNumber,
    NumberOverflow,
    InvalidHex,
    InvalidLineEnding,
    UnknownCommand,
    MissingField,
    ExtraField,
    DuplicateReplica,
    UnknownReplica,
    BufferMismatch,
    PendingOperation,
    NoPendingOperation,
    DuplicateOperation,
    UnknownOperation,
    InvalidRange,
    InvalidUtf8Boundary,
    EmptyUndo,
    EmptyRedo,
    DuplicateAnchor,
    UnknownAnchor,
    DuplicateVersion,
    UnknownVersion,
}

impl Error {
    fn name(self) -> &'static str {
        match self {
            Self::MalformedTrace => "MalformedTrace",
            Self::UnsupportedVersion => "UnsupportedVersion",
            Self::InvalidUtf8 => "InvalidUtf8",
            Self::InvalidNumber => "InvalidNumber",
            Self::NumberOverflow => "NumberOverflow",
            Self::InvalidHex => "InvalidHex",
            Self::InvalidLineEnding => "InvalidLineEnding",
            Self::UnknownCommand => "UnknownCommand",
            Self::MissingField => "MissingField",
            Self::ExtraField => "ExtraField",
            Self::DuplicateReplica => "DuplicateReplica",
            Self::UnknownReplica => "UnknownReplica",
            Self::BufferMismatch => "BufferMismatch",
            Self::PendingOperation => "PendingOperation",
            Self::NoPendingOperation => "NoPendingOperation",
            Self::DuplicateOperation => "DuplicateOperation",
            Self::UnknownOperation => "UnknownOperation",
            Self::InvalidRange => "InvalidRange",
            Self::InvalidUtf8Boundary => "InvalidUtf8Boundary",
            Self::EmptyUndo => "EmptyUndo",
            Self::EmptyRedo => "EmptyRedo",
            Self::DuplicateAnchor => "DuplicateAnchor",
            Self::UnknownAnchor => "UnknownAnchor",
            Self::DuplicateVersion => "DuplicateVersion",
            Self::UnknownVersion => "UnknownVersion",
        }
    }
}

struct Replica {
    buffer: Buffer,
    pending: Option<Operation>,
}

struct StoredAnchor {
    anchor: Anchor,
    bias: Bias,
    buffer: BufferId,
}

struct StoredVersion {
    version: Global,
    buffer: BufferId,
}

#[derive(Default)]
struct Oracle {
    replicas: BTreeMap<u16, Replica>,
    initial_inputs: BTreeMap<u64, String>,
    operations: BTreeMap<String, Operation>,
    anchors: BTreeMap<String, StoredAnchor>,
    versions: BTreeMap<String, StoredVersion>,
}

fn main() {
    let mut input = Vec::new();
    if io::stdin().read_to_end(&mut input).is_err() {
        fail(0, Error::MalformedTrace);
    }
    if let Err((line, error)) = run(&input) {
        fail(line, error);
    }
}

fn fail(line: usize, error: Error) -> ! {
    eprintln!("trace line {}: {}", line, error.name());
    std::process::exit(2)
}

fn run(input: &[u8]) -> std::result::Result<(), (usize, Error)> {
    let lines = split_lines(input).map_err(|e| (1, e))?;
    let mut commands = Vec::new();
    for (index, bytes) in lines.into_iter().enumerate() {
        let line_number = index + 1;
        let line = std::str::from_utf8(bytes).map_err(|_| (line_number, Error::InvalidUtf8))?;
        if line.as_bytes().contains(&0)
            || line
                .chars()
                .any(|c| c.is_whitespace() && c != ' ' && c != '\t')
        {
            return Err((line_number, Error::MalformedTrace));
        }
        let line = line.trim_matches([' ', '\t']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        commands.push((line_number, line));
    }

    let Some((first_line, first)) = commands.first().copied() else {
        return Ok(());
    };
    let first_fields = fields(first);
    if first_fields.first() != Some(&"trace") {
        for (line, command) in commands {
            if fields(command).as_slice() != ["emit"] {
                return Err((line, Error::MalformedTrace));
            }
            println!("{INITIAL_STATE}");
        }
        return Ok(());
    }
    exact(&first_fields, 2).map_err(|e| (first_line, e))?;
    if first_fields[1] != "2" {
        return Err((first_line, Error::UnsupportedVersion));
    }

    let mut oracle = Oracle::default();
    for (line_number, command) in commands.into_iter().skip(1) {
        oracle
            .command(&fields(command))
            .map_err(|e| (line_number, e))?;
    }
    Ok(())
}

fn split_lines(input: &[u8]) -> Result<Vec<&[u8]>> {
    if input.starts_with(&[0xef, 0xbb, 0xbf]) {
        return split_lines(&input[3..]);
    }
    if input.windows(3).any(|w| w == [0xef, 0xbb, 0xbf]) || input.contains(&0) {
        return Err(Error::MalformedTrace);
    }
    let has_crlf = input.windows(2).any(|w| w == b"\r\n");
    if input
        .iter()
        .enumerate()
        .any(|(i, b)| *b == b'\r' && input.get(i + 1) != Some(&b'\n'))
    {
        return Err(Error::InvalidLineEnding);
    }
    if has_crlf
        && input
            .iter()
            .enumerate()
            .any(|(i, b)| *b == b'\n' && (i == 0 || input[i - 1] != b'\r'))
    {
        return Err(Error::InvalidLineEnding);
    }
    let mut result = Vec::new();
    let mut start = 0;
    for (i, b) in input.iter().enumerate() {
        if *b == b'\n' {
            let end = if i > start && input[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            result.push(&input[start..end]);
            start = i + 1;
        }
    }
    if start < input.len() {
        result.push(&input[start..]);
    }
    Ok(result)
}

fn fields(line: &str) -> Vec<&str> {
    line.split([' ', '\t']).filter(|s| !s.is_empty()).collect()
}

fn exact(fields: &[&str], count: usize) -> Result<()> {
    if fields.len() < count {
        Err(Error::MissingField)
    } else if fields.len() > count {
        Err(Error::ExtraField)
    } else {
        Ok(())
    }
}

fn number<T: std::str::FromStr>(token: &str) -> Result<T> {
    if token.is_empty()
        || (token.len() > 1 && token.starts_with('0'))
        || !token.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Error::InvalidNumber);
    }
    token.parse().map_err(|_| Error::NumberOverflow)
}

fn name(token: &str) -> Result<&str> {
    let mut chars = token.bytes();
    if !chars
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        || !chars.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(Error::MalformedTrace);
    }
    Ok(token)
}

fn payload(token: &str) -> Result<String> {
    if token == "-" {
        return Ok(String::new());
    }
    if token.is_empty()
        || token.len() % 2 != 0
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::InvalidHex);
    }
    let bytes: Vec<u8> = token
        .as_bytes()
        .chunks_exact(2)
        .map(|p| (hex_digit(p[0]) << 4) | hex_digit(p[1]))
        .collect();
    String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
}

fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        _ => b - b'a' + 10,
    }
}

fn parse_replica(token: &str) -> Result<u16> {
    let id: u16 = number(token)?;
    if id < ReplicaId::FIRST_COLLAB_ID.as_u16() || id == u16::MAX {
        return Err(Error::InvalidNumber);
    }
    Ok(id)
}

impl Oracle {
    fn command(&mut self, f: &[&str]) -> Result<()> {
        let Some(command) = f.first() else {
            return Err(Error::MalformedTrace);
        };
        match *command {
            "trace" => Err(Error::MalformedTrace),
            "replica" => {
                exact(f, 4)?;
                let id = parse_replica(f[1])?;
                let buffer: u64 = number(f[2])?;
                if buffer == 0 {
                    return Err(Error::InvalidNumber);
                }
                let input = payload(f[3])?;
                if self.replicas.contains_key(&id) {
                    return Err(Error::DuplicateReplica);
                }
                if self
                    .initial_inputs
                    .get(&buffer)
                    .is_some_and(|old| old != &input)
                {
                    return Err(Error::BufferMismatch);
                }
                let buffer_id = BufferId::new(buffer).map_err(|_| Error::InvalidNumber)?;
                let native = Buffer::new(ReplicaId::new(id), buffer_id, input.clone());
                self.initial_inputs.entry(buffer).or_insert(input);
                self.replicas.insert(
                    id,
                    Replica {
                        buffer: native,
                        pending: None,
                    },
                );
                Ok(())
            }
            "edit" => {
                exact(f, 5)?;
                let id = parse_replica(f[1])?;
                let start: usize = number(f[2])?;
                let end: usize = number(f[3])?;
                let text = payload(f[4])?;
                let replica = self.replica_mut(id)?;
                if replica.pending.is_some() {
                    return Err(Error::PendingOperation);
                }
                validate_range(replica.buffer.snapshot(), start, end)?;
                let op = replica.buffer.edit([(start..end, text)]);
                replica.pending = Some(op);
                Ok(())
            }
            "capture" => {
                exact(f, 3)?;
                let id = parse_replica(f[1])?;
                let operation_name = name(f[2])?;
                if self.operations.contains_key(operation_name) {
                    return Err(Error::DuplicateOperation);
                }
                let replica = self.replica_mut(id)?;
                if replica.pending.is_none() {
                    return Err(Error::NoPendingOperation);
                }
                let op = replica.pending.take().ok_or(Error::NoPendingOperation)?;
                self.operations.insert(operation_name.to_owned(), op);
                Ok(())
            }
            "deliver" => {
                exact(f, 3)?;
                let operation_name = name(f[1])?;
                let id = parse_replica(f[2])?;
                let op = self
                    .operations
                    .get(operation_name)
                    .cloned()
                    .ok_or(Error::UnknownOperation)?;
                self.replica_mut(id)?.buffer.apply_ops([op]);
                Ok(())
            }
            "undo" | "redo" => {
                exact(f, 2)?;
                let id = parse_replica(f[1])?;
                let replica = self.replica_mut(id)?;
                if replica.pending.is_some() {
                    return Err(Error::PendingOperation);
                }
                let result = if *command == "undo" {
                    replica.buffer.undo()
                } else {
                    replica.buffer.redo()
                };
                let op = result.map(|(_, op)| op).ok_or(if *command == "undo" {
                    Error::EmptyUndo
                } else {
                    Error::EmptyRedo
                })?;
                replica.pending = Some(op);
                Ok(())
            }
            "anchor" => {
                exact(f, 5)?;
                let id = parse_replica(f[1])?;
                let anchor_name = name(f[2])?;
                let offset: usize = number(f[3])?;
                let bias = parse_bias(f[4])?;
                if self.anchors.contains_key(anchor_name) {
                    return Err(Error::DuplicateAnchor);
                }
                let replica = self.replica(id)?;
                validate_offset(replica.buffer.snapshot(), offset)?;
                let native = replica.buffer.snapshot().anchor_at(offset, bias);
                let buffer = replica.buffer.remote_id();
                self.anchors.insert(
                    anchor_name.to_owned(),
                    StoredAnchor {
                        anchor: native,
                        bias,
                        buffer,
                    },
                );
                Ok(())
            }
            "resolve" => {
                exact(f, 3)?;
                let id = parse_replica(f[1])?;
                let anchor_name = name(f[2])?;
                let stored = self.anchors.get(anchor_name).ok_or(Error::UnknownAnchor)?;
                let replica = self.replica(id)?;
                if replica.buffer.remote_id() != stored.buffer {
                    return Err(Error::BufferMismatch);
                }
                let snapshot = replica.buffer.snapshot();
                let resolvable = snapshot.can_resolve(&stored.anchor);
                let valid = resolvable && stored.anchor.is_valid(snapshot);
                let offset = if valid {
                    stored.anchor.to_offset(snapshot).to_string()
                } else {
                    "-".to_owned()
                };
                println!(
                    "anchor version=2 replica={id} name={anchor_name} valid={} offset={offset} bias={} buffer={}",
                    if valid { 1 } else { 0 },
                    bias_name(stored.bias),
                    stored.buffer
                );
                Ok(())
            }
            "mark" => {
                exact(f, 3)?;
                let id = parse_replica(f[1])?;
                let version_name = name(f[2])?;
                if self.versions.contains_key(version_name) {
                    return Err(Error::DuplicateVersion);
                }
                let replica = self.replica(id)?;
                self.versions.insert(
                    version_name.to_owned(),
                    StoredVersion {
                        version: replica.buffer.version(),
                        buffer: replica.buffer.remote_id(),
                    },
                );
                Ok(())
            }
            "patch" => {
                exact(f, 3)?;
                let id = parse_replica(f[1])?;
                let version_name = name(f[2])?;
                let stored = self
                    .versions
                    .get(version_name)
                    .ok_or(Error::UnknownVersion)?;
                let replica = self.replica(id)?;
                if replica.buffer.remote_id() != stored.buffer {
                    return Err(Error::BufferMismatch);
                }
                let edits: Vec<String> = replica
                    .buffer
                    .snapshot()
                    .edits_since::<usize>(&stored.version)
                    .map(|e| {
                        format!(
                            "{}:{}:{}:{}",
                            e.old.start, e.old.end, e.new.start, e.new.end
                        )
                    })
                    .collect();
                println!(
                    "patch version=2 replica={id} since={version_name} edits={}",
                    if edits.is_empty() {
                        "-".to_owned()
                    } else {
                        edits.join(",")
                    }
                );
                Ok(())
            }
            "line-ending" => {
                exact(f, 3)?;
                let id = parse_replica(f[1])?;
                let ending = match f[2] {
                    "lf" => LineEnding::Unix,
                    "crlf" => LineEnding::Windows,
                    _ => return Err(Error::InvalidLineEnding),
                };
                self.replica_mut(id)?.buffer.set_line_ending(ending);
                Ok(())
            }
            "emit" => {
                exact(f, 2)?;
                if f[1] == "all" {
                    for (&id, replica) in &self.replicas {
                        emit(id, replica);
                    }
                } else {
                    let id = parse_replica(f[1])?;
                    emit(id, self.replica(id)?);
                }
                Ok(())
            }
            _ => Err(Error::UnknownCommand),
        }
    }

    fn replica(&self, id: u16) -> Result<&Replica> {
        self.replicas.get(&id).ok_or(Error::UnknownReplica)
    }
    fn replica_mut(&mut self, id: u16) -> Result<&mut Replica> {
        self.replicas.get_mut(&id).ok_or(Error::UnknownReplica)
    }
}

fn validate_range(snapshot: &text::BufferSnapshot, start: usize, end: usize) -> Result<()> {
    if start > end || end > snapshot.len() {
        return Err(Error::InvalidRange);
    }
    validate_offset(snapshot, start)?;
    validate_offset(snapshot, end)
}

fn validate_offset(snapshot: &text::BufferSnapshot, offset: usize) -> Result<()> {
    if offset > snapshot.len() {
        return Err(Error::InvalidRange);
    }
    if !snapshot.as_rope().is_char_boundary(offset) {
        return Err(Error::InvalidUtf8Boundary);
    }
    Ok(())
}

fn parse_bias(value: &str) -> Result<Bias> {
    match value {
        "left" => Ok(Bias::Left),
        "right" => Ok(Bias::Right),
        _ => Err(Error::MalformedTrace),
    }
}
fn bias_name(value: Bias) -> &'static str {
    match value {
        Bias::Left => "left",
        Bias::Right => "right",
    }
}
fn ending_name(value: LineEnding) -> &'static str {
    match value {
        LineEnding::Unix => "lf",
        LineEnding::Windows => "crlf",
    }
}

fn emit(id: u16, replica: &Replica) {
    let buffer = &replica.buffer;
    let snapshot = buffer.snapshot();
    let bytes = snapshot
        .as_rope()
        .to_string()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let vv = snapshot
        .version()
        .iter()
        .filter(|v| v.value != 0)
        .map(|v| format!("{}:{}", v.replica_id.as_u16(), v.value))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "state version=2 replica={id} buffer={} text={} line-ending={} vv={} operations={} deferred={}",
        buffer.remote_id(),
        if bytes.is_empty() { "-" } else { &bytes },
        ending_name(snapshot.line_ending()),
        if vv.is_empty() { "-" } else { &vv },
        buffer.operations().iter().count(),
        buffer.deferred_ops_len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_mixed_endings() {
        assert_eq!(
            split_lines(b"trace 2\r\nemit all\n"),
            Err(Error::InvalidLineEnding)
        );
    }

    #[test]
    fn payload_is_canonical_and_utf8() {
        assert_eq!(payload("6869").unwrap(), "hi");
        assert_eq!(payload("AB"), Err(Error::InvalidHex));
        assert_eq!(payload("ff"), Err(Error::InvalidUtf8));
    }

    #[test]
    fn reserved_replicas_are_rejected() {
        assert_eq!(parse_replica("3"), Err(Error::InvalidNumber));
        assert_eq!(parse_replica("8"), Ok(8));
        assert_eq!(parse_replica("65535"), Err(Error::InvalidNumber));
    }
}
