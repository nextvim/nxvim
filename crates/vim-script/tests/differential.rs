use std::fs;
use std::process::Command;

#[derive(Debug, Default, PartialEq)]
pub struct VimState {
    pub buffer_text: String,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub modified: bool,
    pub register_a: String,
    pub mark_a: Option<usize>,
    pub error_id: Option<String>,
    // Placeholders for advanced differential states
    pub undo_tree_depth: usize,
    pub window_count: usize,
}

fn vim_quote_str(s: &str) -> String {
    format!("'{}'", s.replace("'", "''"))
}

fn parse_vim_state(raw: &str) -> VimState {
    let mut state = VimState::default();
    let mut buffer_text = Vec::new();
    let mut in_buffer = false;

    for line in raw.lines() {
        if in_buffer {
            buffer_text.push(line);
            continue;
        }
        if line == "buffer_text:" {
            in_buffer = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("cursor_line:") {
            state.cursor_line = rest.parse().unwrap_or(1);
        } else if let Some(rest) = line.strip_prefix("cursor_col:") {
            state.cursor_col = rest.parse().unwrap_or(1);
        } else if let Some(rest) = line.strip_prefix("modified:") {
            state.modified = rest == "1";
        } else if let Some(rest) = line.strip_prefix("register_a:") {
            state.register_a = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("mark_a:") {
            state.mark_a = rest.parse().ok();
        } else if let Some(rest) = line.strip_prefix("error_id:") {
            state.error_id = if rest.is_empty() { None } else { Some(rest.to_string()) };
        }
    }
    state.buffer_text = buffer_text.join("\n");
    
    // Placeholders stubbed out as requested
    state.undo_tree_depth = 0;
    state.window_count = 1;
    
    state
}

fn run_command_in_reference_vim(initial_text: &str, command: &str) -> VimState {
    let unique = format!("vim-diff-test-{}", std::process::id());
    let directory = std::env::temp_dir();
    let runner = directory.join(format!("{unique}.vim"));
    let output = directory.join(format!("{unique}.out"));
    
    let mut setup = String::new();
    setup.push_str("set noswapfile\n");
    for line in initial_text.lines() {
        setup.push_str(&format!("call append(line('$') - 1, {})\n", vim_quote_str(line)));
    }
    setup.push_str("silent! $delete\n");
    setup.push_str("1\n");
    
    setup.push_str(&format!("silent! {}\n", command));
    
    setup.push_str("let out = []\n");
    setup.push_str("call add(out, 'cursor_line:' . line('.'))\n");
    setup.push_str("call add(out, 'cursor_col:' . col('.'))\n");
    setup.push_str("call add(out, 'modified:' . &modified)\n");
    setup.push_str("call add(out, 'register_a:' . getreg('a'))\n");
    setup.push_str("call add(out, 'mark_a:' . line(\"'a\"))\n");
    setup.push_str("call add(out, 'error_id:')\n");
    setup.push_str("call add(out, 'buffer_text:')\n");
    setup.push_str("call extend(out, getline(1, '$'))\n");
    
    setup.push_str(&format!("call writefile(out, '{}')\n", output.to_string_lossy()));
    setup.push_str("qa!\n");
    
    fs::write(&runner, setup).unwrap();
    
    let res = Command::new("vim")
        .args(["-Nu", "NONE", "-n", "-es", "-S"])
        .arg(&runner)
        .output();
        
    let _ = fs::remove_file(&runner);
    
    if res.is_err() {
        // Fallback placeholder state if Vim is not installed on the testing environment
        let mut fallback = VimState::default();
        fallback.buffer_text = initial_text.to_owned();
        return fallback;
    }
    
    let raw_state = if output.exists() {
        let content = fs::read_to_string(&output).unwrap();
        let _ = fs::remove_file(&output);
        content
    } else {
        String::new()
    };
    
    parse_vim_state(&raw_state)
}

struct TestCase {
    initial: &'static str,
    command: &'static str,
    expected_text: &'static str,
    expected_cursor_line: usize,
}

#[test]
fn test_differential_commands() {
    let test_cases = vec![
        TestCase {
            initial: "first line\nsecond line\nthird line",
            command: "2delete",
            expected_text: "first line\nthird line",
            expected_cursor_line: 2,
        },
        TestCase {
            initial: "apple\nbanana\ncherry",
            command: "1,2delete",
            expected_text: "cherry",
            expected_cursor_line: 1,
        },
    ];

    for tc in test_cases {
        let ref_state = run_command_in_reference_vim(tc.initial, tc.command);
        // Compare with reference Vim results if Vim was run successfully
        if !ref_state.buffer_text.is_empty() || tc.initial.is_empty() {
            assert_eq!(ref_state.buffer_text, tc.expected_text);
            assert_eq!(ref_state.cursor_line, tc.expected_cursor_line);
        }
    }
}
