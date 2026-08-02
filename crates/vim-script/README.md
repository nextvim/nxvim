# vim-script

A host-agnostic, embeddable Vim script interpreter and asynchronous VM runtime written in Rust.

## Try it

Run the end-to-end showcase:

```sh
cargo run
```

Start the persistent interactive interpreter:

```sh
cargo run --example interpreter
```

The REPL supports multiline functions and control-flow blocks. Use `.eval EXPR` to print an expression, `.globals` to inspect persistent runtime state, `.editor` to inspect the mock editor, and `.help` for all commands.

## Library usage

Compile and execute Vimscript, then inspect values left in the VM's global scope:

```rust
use vim_script::compiler::Compiler;
use vim_script::lexer::Lexer;
use vim_script::parser::Parser;
use vim_script::resolver::{Resolver, ResolverConfig};
use vim_script::runtime::{Scheduler, Value, Vm};
use vim_script::SourceId;

let source = r#"
    let g:numbers = range(1, 5)
    let g:answer = len(g:numbers) * 2
"#;

let lexed = Lexer::new(SourceId(0), source).lex();
assert!(lexed.diagnostics.is_empty());

let parsed = Parser::new_with_source(&lexed.tokens, source).parse();
assert!(parsed.diagnostics.is_empty());

let resolved = Resolver::new(ResolverConfig::default())
    .resolve(parsed.program.expect("parsed program"));
assert!(resolved.diagnostics.is_empty());

let compiled = Compiler::new(&resolved.program.expect("resolved program")).compile();
assert!(compiled.diagnostics.is_empty());

let vm = Vm::new(compiled.module.expect("bytecode module")).expect("valid VM");
let mut scheduler = Scheduler::new(1_000);
let task = scheduler.spawn(vm).expect("spawn VM");
scheduler.run_until_complete(task).expect("execute Vimscript");

let globals = &scheduler.task(task).expect("completed task").vm.globals;
assert_eq!(globals.get("g:answer"), Some(&Value::Integer(10)));
```

Host functions and editor commands can be exposed with `HostRuntime`; see `src/main.rs` for an end-to-end embedding example with capabilities and asynchronous host calls.

## Authoritative references

Vim's help files are the practical language specification. The most important references are:

| Help file | Area |
|---|---|
| `eval.txt` | Expressions, variables, operators, collections, lambdas, functions |
| `builtin.txt` | Builtin functions |
| `userfunc.txt` | User-defined functions |
| `cmdline.txt` | Ex command syntax |
| `map.txt` | Key mappings |
| `options.txt` | Options and `:set` |
| `autocmd.txt` | Autocommands |
| `pattern.txt` | Regular expressions |
| `repeat.txt`, `motion.txt`, `change.txt` | Editing behavior |
| `channel.txt`, `terminal.txt` | Jobs, channels, and terminals |
| `vim9.txt` | Vim9 script |

Useful user-manual chapters include `usr_41.txt` through `usr_44.txt` and `usr_50.txt` through `usr_52.txt`.

For exact compatibility, consult Vim's implementation (`src/eval.c`, `src/evalfunc.c`, `src/userfunc.c`, `src/ex_docmd.c`, `src/ex_cmds*.c`, `src/map.c`, `src/option.c`, `src/regexp.c`, `src/normal.c`, and `src/getchar.c`) and the executable specification in `src/testdir/`.

## Architecture

```text
SourceMap
    |
    v
Lexer -> Parser -> AST -> Semantic Resolver -> Bytecode Compiler -> Async VM
                                                                    |
                                   Host API / Commands / Events / Keymaps
                                                                    |
                                                   Editor or another application
```

Parsing and execution are deliberately separate. Scripts compile once and their bytecode can be cached and reused.

### Modules

- `source`: source files, spans, labels, suggestions, and structured diagnostics
- `lexer`: tokens, keywords, operators, heredocs, and lexer state
- `parser`: parser state, contexts, and parse results
- `ast`: expressions, statements, control flow, functions, Ex commands, mappings, options, and autocommands
- `resolver`: lexical scopes, symbols, closure captures, and resolved bindings
- `compiler`: bytecode emission and control-flow patching state
- `bytecode`: instructions, constants, function prototypes, and modules
- `runtime`: values, frames, exceptions, tasks, scheduler state, and resource limits
- `host`: asynchronous host calls, capability checks, and builtin/command registries
- `integration`: events, compiled keymaps, options, and module caching

## Runtime model

The VM is stack-based and uses explicit call frames. Bytecode includes local/global access, collection construction, calls and closures, jumps, exceptions, host commands, and `Await`. A suspended VM records the task it is waiting on and can be resumed by a scheduler.

Host interaction is asynchronous and capability-controlled. The language core does not directly know about buffers, windows, filesystems, networks, or UI. An embedding application implements the `Host` trait and registers native functions and commands. This allows an editor to expose APIs such as `getline()` or `editor.current_buffer()`, while another application can expose unrelated APIs without changing the interpreter.

Capabilities cover editor access, buffer reads/writes, windows, settings, filesystem access, networking, clipboard access, terminals, processes, and UI. Resource limits bound instructions, call depth, stack size, collection size, and concurrent tasks.

## Compatibility strategy

Legacy Vim script is the initial target. Vim9 is represented by a language-version boundary for later work. The implementation should retain familiar syntax and compatibility semantics while using clean internal types and a modern VM.

The initial implementation sequence is complete:

1. Source map, lexer, and diagnostics
2. Expression and statement parser
3. AST and semantic resolver
4. Bytecode compiler and synchronous VM core
5. Builtin values, functions, and errors
6. Async suspension and scheduler integration
7. Host API and capability enforcement
8. Ex commands, options, mappings, and autocommands
9. Compatibility testing against selected Vim regression tests
10. Incremental expansion toward plugin compatibility

## Next steps: plugin compatibility

Development now proceeds through compatibility-driven slices based on conventional plugin structure and real plugin failures:

1. ~~Module-owned function references~~ — complete
2. ~~Automatic autoload~~ — complete
3. ~~`exists()` and plugin load guards~~ — complete
4. ~~Source-preserving Ex parser~~ — complete
5. ~~User commands~~ — complete
6. ~~Mappings and autocommands~~ — complete
7. ~~First realistic plugin fixture (upstream Vim `desert` colorscheme)~~ — complete
8. Select and support a small external plugin

Each slice should add focused regression and plugin-level integration tests. Compatibility gaps should be reported structurally rather than causing crashes, and host-facing behavior should remain asynchronous and capability-controlled.

The difficult portion is behavioral compatibility accumulated over decades. Vim's documentation describes intended behavior; its source and regression suite settle ambiguous edge cases.
