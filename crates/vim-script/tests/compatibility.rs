use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use vim_script::compiler::Compiler;
use vim_script::host::{
    Capability, CommandDefinition, CommandRequest, Host, HostFuture, HostRequest, HostRuntime,
};
use vim_script::lexer::Lexer;
use vim_script::parser::Parser;
use vim_script::resolver::{Resolver, ResolverConfig};
use vim_script::runtime::{RuntimeResult, Scheduler, Value, Vm};
use vim_script::source::SourceId;

const COMPATIBILITY_FIXTURES: &[(&str, &str)] = &[
    ("eval.vim", "[14, 20, 2, 1, 1, 1, 20]"),
    ("functions.vim", "[42, 3]"),
    ("listdict.vim", "[3, 10, 30, 99]"),
    ("control_flow.vim", "[26, 3]"),
    ("trycatch.vim", "11"),
    ("builtins.vim", "[4, 2, 5, '2,3,4,5', 'VIM']"),
    ("exists.vim", "[1, 0, 1, 1, 1, 7]"),
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn compile_vm(source: &str) -> Vm {
    let lexed = Lexer::new(SourceId(0), source).lex();
    assert!(
        lexed.diagnostics.is_empty(),
        "lexer diagnostics: {:?}",
        lexed.diagnostics
    );
    let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics: {:?}",
        parsed.diagnostics
    );
    let resolved = Resolver::new(ResolverConfig::default()).resolve(parsed.program.unwrap());
    assert!(
        resolved.diagnostics.is_empty(),
        "resolver diagnostics: {:?}",
        resolved.diagnostics
    );
    let compiled = Compiler::new(&resolved.program.unwrap()).compile();
    assert!(
        compiled.diagnostics.is_empty(),
        "compiler diagnostics: {:?}",
        compiled.diagnostics
    );
    Vm::new(compiled.module.unwrap()).unwrap()
}

fn run_runtime(path: &Path) -> String {
    let source = fs::read_to_string(path).unwrap();
    let mut vm = compile_vm(&source);
    vm.run().unwrap();
    vim_string(
        vm.globals
            .get("g:compat_result")
            .expect("fixture must set g:compat_result"),
    )
}

fn run_reference_vim(path: &Path) -> String {
    let unique = format!(
        "vim-script-compat-{}-{}",
        std::process::id(),
        path.file_stem().unwrap().to_string_lossy()
    );
    let directory = std::env::temp_dir();
    let runner = directory.join(format!("{unique}.vim"));
    let output = directory.join(format!("{unique}.out"));
    let escaped_fixture = vim_quote(&path.canonicalize().unwrap().to_string_lossy());
    let escaped_output = vim_quote(&output.to_string_lossy());
    fs::write(&runner, format!("execute 'source ' . fnameescape('{escaped_fixture}')\ncall writefile([string(g:compat_result)], '{escaped_output}')\nqa!\n")).unwrap();
    let result = Command::new("vim")
        .args(["-Nu", "NONE", "-n", "-es", "-S"])
        .arg(&runner)
        .output()
        .expect("Vim must be installed for differential compatibility tests");
    let _ = fs::remove_file(&runner);
    assert!(
        result.status.success(),
        "reference Vim failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    let value = fs::read_to_string(&output).unwrap().trim_end().to_owned();
    let _ = fs::remove_file(output);
    value
}

fn vim_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn vim_string(value: &Value) -> String {
    match value {
        Value::Null => "v:null".into(),
        Value::Bool(value) => i32::from(*value).to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::String(value) => format!("'{}'", value.replace('\'', "''")),
        Value::Blob(value) => format!(
            "0z{}",
            value
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        ),
        Value::List(values) => format!(
            "[{}]",
            values.iter().map(vim_string).collect::<Vec<_>>().join(", ")
        ),
        Value::Dictionary(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("'{}': {}", key.replace('\'', "''"), vim_string(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Closure(_) => "function('<lambda>')".into(),
        Value::Builtin(name) | Value::HostFunction(name) => format!("function('{name}')"),
        Value::Future(operation) => format!("future({})", operation.0),
        Value::HostObject(object) => format!("object({})", object.0),
    }
}

#[test]
fn selected_fixtures_match_snapshots_and_reference_vim() {
    let has_vim = Command::new("vim")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    for (name, expected) in COMPATIBILITY_FIXTURES {
        let path = fixture(name);
        let actual = run_runtime(&path);
        assert_eq!(&actual, expected, "snapshot mismatch in {name}");
        if has_vim {
            assert_eq!(
                actual,
                run_reference_vim(&path),
                "reference Vim mismatch in {name}"
            );
        }
    }
}

#[derive(Default)]
struct MockEditor {
    commands: Arc<Mutex<Vec<CommandRequest>>>,
}

impl Host for MockEditor {
    fn call(&self, _request: HostRequest) -> HostFuture {
        Box::pin(async { Ok(Value::Null) })
    }
    fn execute_command(&self, request: CommandRequest) -> HostFuture {
        self.commands.lock().unwrap().push(request);
        Box::pin(async { Ok(Value::Null) })
    }
}

#[test]
fn editor_commands_are_sequential_and_capability_checked() -> RuntimeResult<()> {
    let commands = Arc::new(Mutex::new(Vec::new()));
    let mut host = HostRuntime::new(Arc::new(MockEditor {
        commands: commands.clone(),
    }));
    host.capabilities.grant(Capability::FileSystemWrite);
    host.register_command(CommandDefinition {
        name: "write".into(),
        minimum_abbreviation: 1,
        accepts_bang: true,
        accepts_range: false,
        accepts_count: false,
        accepts_register: false,
        required_capabilities: vec![Capability::FileSystemWrite],
    });
    let mut scheduler = Scheduler::new(8);
    scheduler.set_host(host);
    let task = scheduler.spawn(compile_vm(":w\nlet g:after_write = 1\n"))?;
    scheduler.run_until_complete(task)?;
    assert_eq!(
        scheduler
            .task(task)
            .unwrap()
            .vm
            .globals
            .get("g:after_write"),
        Some(&Value::Integer(1))
    );
    assert_eq!(commands.lock().unwrap().len(), 1);
    Ok(())
}

#[test]
fn malformed_scripts_report_diagnostics_without_panicking() {
    let cases = ["let = 1\n", "if 1\nlet x = 2\n", "let x = {'missing': }\n"];
    for source in cases {
        let lexed = Lexer::new(SourceId(0), source).lex();
        let parsed = Parser::new(&lexed.tokens).parse();
        assert!(!lexed.diagnostics.is_empty() || !parsed.diagnostics.is_empty());
    }
}

#[test]
fn runtime_errors_keep_vim_codes_and_source_spans() {
    let mut vm = compile_vm("let g:result = len(1)\n");
    let error = vm.run().unwrap_err();
    assert_eq!(error.code.as_deref(), Some("E745"));
    assert!(error.span.is_some());
    assert!(!error.stack_trace.is_empty());
}
