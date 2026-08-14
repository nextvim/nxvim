use std::collections::HashMap;
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
    println!("            Vimscript Embeddable Runtime Showcase");
    println!("============================================================");
    println!("This showcase demonstrates how to compile and run Vimscript");
    println!("using an asynchronous VM runtime with full host capability checks");
    println!("and interactive mock editor state modifications.");
    println!("============================================================");
    println!();

    // 1. Define the Vimscript code to run
    let source = r#"
        " Set up some basic global variables
        let g:numbers = range(1, 5)
        let g:answer = len(g:numbers) * 2

        " Let's interact with the host editor using getline and setline
        let g:original_line_1 = await getline(1)
        let _ = await setline(1, 'Modified by Vimscript: ' .. g:original_line_1)
        let _ = await append(1, 'A new line added dynamically!')

        " Execute some editor commands
        set number filetype=rust
        highlight ErrorMsg gui=bold guifg=Red

        " Demonstrate control flow with user functions
        function CalculateFactorial(n)
            let result = 1
            let i = 1
            while i <= a:n
                let result = result * i
                let i = i + 1
            endwhile
            return result
        endfunction

        let g:factorial_5 = CalculateFactorial(5)

        " Demonstrate command definitions accepting ranges
        " 1. Log all initial buffer lines using a whole-file range (%)
        :%LogRange

        " 2. Delete the first two lines using a specific numeric range
        :1,2DeleteLines
    "#;

    println!("Source code to execute:");
    println!("------------------------------------------------------------");
    for line in source.lines() {
        println!("  {}", line);
    }
    println!("------------------------------------------------------------");
    println!();

    // 2. Set up the Mock Editor (Host)
    let editor = MockEditor::default();

    // Populate the mock editor buffer with initial text
    {
        if let Ok(mut state) = editor.state.lock() {
            let current_id = state.current_buffer;
            if let Some(buf) = state.buffers.get_mut(&current_id) {
                buf.lines = vec![
                    "Hello world! This is line 1.".to_string(),
                    "This is line 2 of the mock editor.".to_string(),
                ];
            }
        }
    }

    println!("Initial Editor Buffer State:");
    print_buffer_lines(&editor);
    println!();

    // 3. Set up the Host Runtime & register functions / commands
    let mut host = HostRuntime::new(Arc::new(editor.clone()));

    // Grant capabilities required by the script
    host.capabilities.grant(Capability::Editor);
    host.capabilities.grant(Capability::BufferRead);
    host.capabilities.grant(Capability::BufferWrite);
    host.capabilities.grant(Capability::Window);
    host.capabilities.grant(Capability::Settings);
    host.capabilities.grant(Capability::UserInterface);

    // Register host functions of MockEditor
    host.register_function("getline", Arity::Exact(1), vec![Capability::BufferRead]);
    host.register_function("setline", Arity::Exact(2), vec![Capability::BufferWrite]);
    host.register_function("append", Arity::Exact(2), vec![Capability::BufferWrite]);
    host.register_function("cursor", Arity::Exact(2), vec![Capability::Window]);

    // Register host commands of MockEditor
    host.register_command(
        CommandDefinition::new("set", 2)
            .with_capabilities(vec![Capability::Settings]),
    );
    host.register_command(
        CommandDefinition::new("highlight", 2)
            .with_capabilities(vec![Capability::UserInterface]),
    );
    host.register_command(
        CommandDefinition::new("DeleteLines", 3)
            .with_range(true)
            .with_capabilities(vec![Capability::BufferWrite]),
    );
    host.register_command(
        CommandDefinition::new("LogRange", 3)
            .with_range(true)
            .with_capabilities(vec![Capability::BufferRead]),
    );

    // 4. Compile the Vimscript
    println!("Compiling Vimscript...");
    let mut sources = SourceMap::default();
    let source_id = sources.add("showcase.vim", source);

    let lexed = Lexer::new(source_id, source).lex();
    if !lexed.diagnostics.is_empty() {
        for d in &lexed.diagnostics {
            print!("{}", sources.render(d));
        }
        return;
    }

    let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
    if !parsed.diagnostics.is_empty() {
        for d in &parsed.diagnostics {
            print!("{}", sources.render(d));
        }
        return;
    }
    let program = parsed.program.expect("parsed program");

    let mut config = ResolverConfig::default();
    for name in host.functions.names() {
        config.builtins.insert(name.to_string());
    }
    let resolved = Resolver::new(config).resolve(program);
    if !resolved.diagnostics.is_empty() {
        for d in &resolved.diagnostics {
            print!("{}", sources.render(d));
        }
        return;
    }
    let resolved_program = resolved.program.expect("resolved program");

    let compiled = Compiler::new(&resolved_program).compile();
    if !compiled.diagnostics.is_empty() {
        for d in &compiled.diagnostics {
            print!("{}", sources.render(d));
        }
        return;
    }
    let module = compiled.module.expect("compiled module");
    println!("Compilation successful!");
    println!();

    // 5. Instantiate the Vm and Scheduler, then execute
    println!("Running VM and executing bytecode...");
    let vm = Vm::new(module).expect("failed to create VM");
    let mut scheduler = Scheduler::new(10_000);
    scheduler.set_host(host);

    let task = scheduler.spawn(vm).expect("failed to spawn task");
    match scheduler.run_until_complete(task) {
        Ok(_) => {
            println!("Execution completed successfully!");
            println!();

            // Retrieve updated globals
            if let Some(finished_task) = scheduler.task(task) {
                print_globals(&finished_task.vm.globals);
            }
            println!();

            // Print modified editor state
            println!("Final Editor Buffer State:");
            print_buffer_lines(&editor);
            println!();

            if let Ok(state) = editor.snapshot() {
                if !state.messages.is_empty() {
                    println!("Logged messages during execution:");
                    for msg in &state.messages {
                        println!("  {}", msg);
                    }
                    println!();
                }
            }

            println!("Final Editor Options and Style State:");
            if let Ok(state) = editor.snapshot() {
                println!("  Options:");
                for (key, val) in &state.options {
                    println!("    &{} = {:?}", key, val);
                }
                println!("  Highlights:");
                for (group, style) in &state.highlights.groups {
                    println!("    highlight {} attributes={:?}", group, style.attributes);
                }
            }
        }
        Err(err) => {
            println!("Runtime execution failed: {}", err.message);
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
        }
    }
}

fn print_buffer_lines(editor: &MockEditor) {
    if let Ok(state) = editor.snapshot() {
        if let Some(buf) = state.buffers.get(&state.current_buffer) {
            for (i, line) in buf.lines.iter().enumerate() {
                println!("    {:3}: {}", i + 1, line);
            }
        }
    }
}

fn print_globals(globals: &HashMap<String, Value>) {
    println!("Global persistent variables left by Vimscript:");
    let mut keys: Vec<_> = globals.keys().collect();
    keys.sort();
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
        println!("  {} = {:?}", key, globals.get(key).unwrap());
    }
}
