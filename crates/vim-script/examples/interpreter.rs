use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;

use vim_script::compiler::Compiler;
use vim_script::host::{Arity, Capability, CommandDefinition, HostRuntime};
use vim_script::lexer::Lexer;
use vim_script::mock_editor::MockEditor;
use vim_script::parser::Parser;
use vim_script::resolver::{Resolver, ResolverConfig};
use vim_script::runtime::{Scheduler, Value, Vm};
use vim_script::source::{Diagnostic, Severity, SourceMap};

fn main() {
    println!("============================================================");
    println!("      Vimscript Interactive Playground & Interpreter");
    println!("============================================================");
    println!("Type Vimscript statements here. Press Enter to execute.");
    println!("Type `.help` for a list of REPL commands.");
    println!("Supports multiline functions and control flow blocks (if, for, while, try).");
    println!("============================================================");
    println!();

    // Set up the mock editor as host
    let editor = MockEditor::default();
    let mut host = HostRuntime::new(Arc::new(editor.clone()));

    // Grant standard capabilities
    host.capabilities.grant(Capability::Editor);
    host.capabilities.grant(Capability::BufferRead);
    host.capabilities.grant(Capability::BufferWrite);
    host.capabilities.grant(Capability::Window);
    host.capabilities.grant(Capability::Settings);
    host.capabilities.grant(Capability::FileSystemRead);
    host.capabilities.grant(Capability::FileSystemWrite);
    host.capabilities.grant(Capability::Network);
    host.capabilities.grant(Capability::ClipboardRead);
    host.capabilities.grant(Capability::ClipboardWrite);
    host.capabilities.grant(Capability::Terminal);
    host.capabilities.grant(Capability::Process);

    host.capabilities.grant(Capability::UserInterface);

    // Register host functions of MockEditor
    host.register_function("getline", Arity::Exact(1), vec![Capability::BufferRead]);
    host.register_function("setline", Arity::Exact(2), vec![Capability::BufferWrite]);
    host.register_function("append", Arity::Exact(2), vec![Capability::BufferWrite]);
    host.register_function("cursor", Arity::Exact(2), vec![Capability::Window]);
    host.register_function("message", Arity::Exact(1), vec![Capability::Editor]);
    host.register_function("echomsg", Arity::Exact(1), vec![Capability::Editor]);

    // Register host commands
    host.register_command(
        CommandDefinition::new("set", 2)
            .with_capabilities(vec![Capability::Settings]),
    );
    host.register_command(
        CommandDefinition::new("highlight", 2)
            .with_capabilities(vec![Capability::UserInterface]),
    );
    host.register_command(
        CommandDefinition::new("syntax", 3)
            .with_capabilities(vec![Capability::Editor]),
    );
    host.register_command(
        CommandDefinition::new("write", 1)
            .with_bang(true)
            .with_capabilities(vec![Capability::FileSystemWrite]),
    );

    let mut globals = HashMap::new();
    let mut sources = SourceMap::default();

    // Pre-populate mock editor buffer with some lines for testing getline/setline
    {
        if let Ok(mut state) = editor.state.lock() {
            let current_id = state.current_buffer;
            if let Some(buf) = state.buffers.get_mut(&current_id) {
                buf.lines = vec![
                    "Welcome to NextVim!".to_string(),
                    "This is line 2 of the active buffer.".to_string(),
                    "Try running `echo getline(1)` or `call setline(1, 'New text!')`".to_string(),
                ];
            }
        }
    }

    let mut current_input = String::new();

    loop {
        if current_input.is_empty() {
            print!("vim> ");
        } else {
            print!("..   ");
        }
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).unwrap() == 0 {
            // EOF reached
            println!();
            break;
        }

        let trimmed_line = line.trim();
        if current_input.is_empty() {
            if trimmed_line == ".help" {
                print_help();
                continue;
            } else if trimmed_line == ".globals" {
                print_globals(&globals);
                continue;
            } else if trimmed_line == ".editor" {
                print_editor_state(&editor);
                continue;
            } else if trimmed_line == ".exit" || trimmed_line == ".quit" {
                break;
            } else if trimmed_line.starts_with(".eval ") {
                let expr = trimmed_line.strip_prefix(".eval ").unwrap().trim();
                if expr.is_empty() {
                    println!("Error: .eval requires an expression");
                } else {
                    let stmt = format!("let g:__eval_res = {}", expr);
                    let mut temp_globals = globals.clone();
                    if execute_source(&stmt, &mut temp_globals, &host, &mut sources).is_some() {
                        if let Some(val) = temp_globals.get("g:__eval_res") {
                            println!("{}", format_value(val));
                        } else {
                            println!("v:null");
                        }
                    }
                }
                continue;
            }
        }

        current_input.push_str(&line);

        if is_complete(&current_input) {
            let code = current_input.trim().to_string();
            current_input.clear();
            if !code.is_empty() {
                execute_source(&code, &mut globals, &host, &mut sources);
            }
        }
    }
}

fn print_help() {
    println!("Available commands:");
    println!("  .eval EXPR   Evaluate a Vimscript expression and print its value");
    println!("  .globals     List all persistent global variables");
    println!(
        "  .editor      Inspect the state of the mock editor (buffers, cursor, options, etc.)"
    );
    println!("  .help        Show this help message");
    println!("  .exit / .quit Exit the interpreter");
    println!();
    println!("Multiline blocks like functions, if/endif, for/endfor, while/endwhile,");
    println!("and try/endtry are supported. Keep entering lines, and the block will");
    println!("execute once fully closed.");
}

fn print_globals(globals: &HashMap<String, Value>) {
    println!("Persistent Global State:");
    let mut keys: Vec<_> = globals.keys().collect();
    keys.sort();
    let mut count = 0;
    for key in keys {
        if key.starts_with(':') {
            continue;
        }
        if let Value::HostFunction(_) = globals.get(key).unwrap() {
            continue;
        }
        if [
            "g:getline",
            "g:setline",
            "g:append",
            "g:cursor",
            "g:message",
            "g:echomsg",
        ]
        .contains(&key.as_str())
        {
            continue;
        }
        println!("  {} = {}", key, format_value(globals.get(key).unwrap()));
        count += 1;
    }
    if count == 0 {
        println!("  (no custom variables defined)");
    }
}

fn print_editor_state(editor: &MockEditor) {
    match editor.snapshot() {
        Ok(state) => {
            println!("Mock Editor State:");
            println!("  Current Buffer ID: {}", state.current_buffer);
            println!(
                "  Cursor Position: (line {}, col {})",
                state.cursor.0, state.cursor.1
            );
            println!("  Buffers:");
            for buffer in state.buffers.values() {
                println!("    Buffer #{} '{}':", buffer.id, buffer.name);
                for (i, line) in buffer.lines.iter().enumerate() {
                    println!("      {:3}: {}", i + 1, line);
                }
            }
            if !state.options.is_empty() {
                println!("  Options:");
                let mut opt_keys: Vec<_> = state.options.keys().collect();
                opt_keys.sort();
                for key in opt_keys {
                    println!(
                        "    &{} = {}",
                        key,
                        format_value(state.options.get(key).unwrap())
                    );
                }
            }

            if !state.highlights.groups.is_empty() {
                println!("  Highlights:");
                for (group, style) in &state.highlights.groups {
                    println!("    highlight {} attributes={:?}", group, style.attributes);
                }
            }

            if !state.messages.is_empty() {
                println!("  Messages/Logs:");
                for msg in &state.messages {
                    println!("    {}", msg);
                }
            }
            if !state.command_log.is_empty() {
                println!("  Command Log (last 5):");
                let start = state.command_log.len().saturating_sub(5);
                for req in &state.command_log[start..] {
                    println!("    :{} {}", req.command.name, req.command.arguments);
                }
            }
            println!("  Write Count: {}", state.write_count);
            println!("  Syntax Reset Count: {}", state.syntax_reset_count);
        }
        Err(err) => {
            println!("Error getting editor snapshot: {:?}", err);
        }
    }
}

fn is_complete(input: &str) -> bool {
    let mut depth = 0;
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('"') {
            continue;
        }

        // Check for block openers
        if trimmed.starts_with("function")
            || trimmed.starts_with("if ")
            || trimmed.starts_with("if(")
            || trimmed.starts_with("for ")
            || trimmed.starts_with("for(")
            || trimmed.starts_with("while ")
            || trimmed.starts_with("while(")
            || trimmed == "try"
            || trimmed.starts_with("try ")
        {
            depth += 1;
        }

        // Check for block closers
        if trimmed.starts_with("endfunction")
            || trimmed == "endf"
            || trimmed.starts_with("endf ")
            || trimmed.starts_with("endif")
            || trimmed == "endi"
            || trimmed.starts_with("endi ")
            || trimmed.starts_with("endfor")
            || trimmed == "endfo"
            || trimmed.starts_with("endfo ")
            || trimmed.starts_with("endwhile")
            || trimmed == "endw"
            || trimmed.starts_with("endw ")
            || trimmed.starts_with("endtry")
            || trimmed == "endt"
            || trimmed.starts_with("endt ")
        {
            if depth > 0 {
                depth -= 1;
            }
        }
    }
    depth == 0
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "v:null".into(),
        Value::Bool(value) => if *value { "v:true" } else { "v:false" }.into(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::String(value) => format!("\"{}\"", value.replace('"', "\\\"")),
        Value::Blob(value) => format!(
            "0z{}",
            value
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        ),
        Value::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(format_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Dictionary(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!(
                    "\"{}\": {}",
                    key.replace('"', "\\\""),
                    format_value(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Closure(_) => "function('<lambda>')".into(),
        Value::Builtin(name) | Value::HostFunction(name) => format!("function('{name}')"),
        Value::Future(operation) => format!("future({})", operation.0),
        Value::HostObject(object) => format!("object({})", object.0),
    }
}

fn execute_source(
    source: &str,
    globals: &mut HashMap<String, Value>,
    host: &HostRuntime,
    sources: &mut SourceMap,
) -> Option<Value> {
    let source_id = sources.add("repl_input", source);

    let lexed = Lexer::new(source_id, source).lex();
    if !lexed.diagnostics.is_empty() {
        for diagnostic in &lexed.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }

    let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
    if !parsed.diagnostics.is_empty() {
        for diagnostic in &parsed.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }
    let Some(program) = parsed.program else {
        return None;
    };

    let mut config = ResolverConfig::default();
    for name in host.functions.names() {
        config.builtins.insert(name.to_string());
    }
    let resolved = Resolver::new(config).resolve(program);
    if !resolved.diagnostics.is_empty() {
        for diagnostic in &resolved.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }
    let Some(resolved_program) = resolved.program else {
        return None;
    };

    let compiled = Compiler::new(&resolved_program).compile();
    if !compiled.diagnostics.is_empty() {
        for diagnostic in &compiled.diagnostics {
            print!("{}", sources.render(diagnostic));
        }
        return None;
    }
    let Some(module) = compiled.module else {
        return None;
    };

    let vm = match Vm::with_globals(module, globals.clone()) {
        Ok(vm) => vm,
        Err(err) => {
            println!("VM error: {}", err.message);
            return None;
        }
    };

    let mut scheduler = Scheduler::new(10_000);
    scheduler.set_host(host.clone());

    let task = match scheduler.spawn(vm) {
        Ok(task) => task,
        Err(err) => {
            println!("Scheduler error: {}", err.message);
            return None;
        }
    };

    match scheduler.run_until_complete(task) {
        Ok(val) => {
            if let Some(finished_task) = scheduler.task(task) {
                *globals = finished_task.vm.globals.clone();
            }
            Some(val)
        }
        Err(err) => {
            println!("Runtime error: {}", err.message);
            if let Some(span) = err.span {
                let diag = Diagnostic {
                    code: err.code.clone(),
                    severity: Severity::Error,
                    message: err.message.clone(),
                    primary: span,
                    labels: Vec::new(),
                    notes: err.notes.to_vec(),
                    suggestions: Vec::new(),
                };
                print!("{}", sources.render(&diag));
            }
            None
        }
    }
}
