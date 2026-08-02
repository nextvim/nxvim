//! Runnable showcase for the `vim-regex` public API.
//!
//! Run it with:
//!
//! ```text
//! cargo run
//! ```

use std::{error::Error, ops::Range};

use vim_regex::{BufferContext, CompileOptions, Match, Regex};

fn main() -> Result<(), Box<dyn Error>> {
    println!("vim-regex public API showcase\n");

    ordinary_captures()?;
    vim_match_boundaries()?;
    editor_aware_assertions()?;
    syntax_region_captures()?;
    structured_diagnostics();

    Ok(())
}

fn ordinary_captures() -> Result<(), Box<dyn Error>> {
    let text = "λ abxyzc";
    let regex = Regex::compile(r"\v%(ab(xyz)c)", CompileOptions::default())?;
    let found = regex.find(text)?.expect("showcase pattern should match");

    println!("1. Ordinary matching and Vim-numbered captures");
    println!("   Vim pattern: {:?}", r"\v%(ab(xyz)c)");
    println!("   backend:     {:?}", regex.backend_pattern());
    print_match(text, &found);
    println!();
    Ok(())
}

fn vim_match_boundaries() -> Result<(), Box<dyn Error>> {
    let text = "prefix: abcbodyend";
    let regex = Regex::compile(r"abc\zsbody\zeend", CompileOptions::default())?;
    let found = regex.find(text)?.expect("boundary pattern should match");

    println!("2. Vim match boundaries (`\\zs` and `\\ze`)");
    println!(
        "   full pattern consumes context, reported match is {:?}",
        &text[found.range.clone()]
    );
    print_match(text, &found);
    println!();
    Ok(())
}

fn editor_aware_assertions() -> Result<(), Box<dyn Error>> {
    let text = "word\nword";
    let context = BufferContext::new(text).with_cursor(5);
    let regex = Regex::compile(r"\%2l\%#word", CompileOptions::default())?;
    let found = regex
        .find_in_context(&context)?
        .expect("line and cursor assertions should match");

    println!("3. Match-time editor context (`\\%l`, `\\%#`, `\\%V`, columns)");
    println!("   cursor byte offset: 5; required line: 2");
    print_match(text, &found);
    println!();
    Ok(())
}

fn syntax_region_captures() -> Result<(), Box<dyn Error>> {
    let start_text = "BEGIN tag";
    let start = Regex::compile(r"BEGIN \z(tag\)", CompileOptions::default())?;
    let start_match = start
        .find(start_text)?
        .expect("syntax-region start should match");
    let captured = capture_text(start_text, &start_match.external_captures, 1);

    let end_text = "END tag";
    let end = Regex::compile_with_external_captures(
        r"END \z1",
        CompileOptions::default(),
        [captured.clone()],
    )?;
    let end_match = end
        .find(end_text)?
        .expect("syntax-region end should match captured text");

    println!("4. Two-stage syntax-region external captures (`\\z(...)` → `\\z1`)");
    println!("   captured by start pattern: {captured:?}");
    print_match(end_text, &end_match);
    println!();
    Ok(())
}

fn structured_diagnostics() {
    println!("5. Structured diagnostics for deliberately unsupported syntax");
    let error = Regex::compile(r"[[=a=]]\+", CompileOptions::default())
        .err()
        .expect("equivalence classes are intentionally unsupported");
    for diagnostic in error.diagnostics {
        println!(
            "   {:?} during {:?} at {:?}: {}",
            diagnostic.kind, diagnostic.phase, diagnostic.span, diagnostic.message
        );
    }
}

fn capture_text(text: &str, captures: &[Option<Range<usize>>], index: usize) -> Option<String> {
    captures
        .get(index)
        .and_then(Option::as_ref)
        .map(|range| text[range.clone()].to_owned())
}

fn print_match(text: &str, found: &Match) {
    println!(
        "   match:       {:?} at bytes {:?}",
        &text[found.range.clone()],
        found.range
    );
    for (index, range) in found.captures.iter().enumerate().skip(1) {
        match range {
            Some(range) => println!(
                "   capture {index}:   {:?} at bytes {range:?}",
                &text[range.clone()]
            ),
            None => println!("   capture {index}:   <unmatched>"),
        }
    }
}
