use std::path::{Path, PathBuf};
use std::sync::Arc;

use vim_script::ast::MapMode;
use vim_script::host::{Capability, CommandDefinition, HostContext, HostRuntime};
use vim_script::integration::{Event, MappingExpansion};
use vim_script::mock_editor::MockEditor;
use vim_script::plugin::{CompatibilityStage, RuntimePath, ScriptLoader};
use vim_script::runtime::Value;

fn plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugins/basic")
}

#[test]
fn discovers_initializes_and_executes_a_tier_one_plugin() {
    let editor = MockEditor::default();
    let mut host = HostRuntime::new(Arc::new(editor.clone()));
    host.capabilities.grant(Capability::FileSystemWrite);
    host.capabilities.grant(Capability::Settings);
    host.register_command(
        CommandDefinition::new("set", 2).with_capabilities(vec![Capability::Settings]),
    );
    host.register_command(
        CommandDefinition::new("write", 1)
            .with_bang(true)
            .with_capabilities(vec![Capability::FileSystemWrite]),
    );

    let mut loader = ScriptLoader::with_host(RuntimePath::new([plugin_root()]), host);
    let report = loader.load_startup_plugins();
    assert!(report.is_compatible(), "{:#?}", report.failures);
    assert_eq!(report.discovered.len(), 3);
    assert_eq!(report.loaded.len(), 3);
    assert_eq!(
        loader.globals.get("g:basic_plugin_loaded"),
        Some(&Value::Integer(1))
    );
    assert_eq!(
        loader.globals.get("g:guard_default"),
        Some(&Value::Integer(7))
    );
    assert!(!loader.globals.contains_key("g:guard_failed"));
    assert_eq!(
        loader.globals.get("g:first_private_value"),
        Some(&Value::Integer(11))
    );
    assert_eq!(
        loader.globals.get("g:second_private_value"),
        Some(&Value::Integer(22))
    );
    assert_eq!(
        loader.globals.get("g:cross_module_value"),
        Some(&Value::Integer(11))
    );
    assert_eq!(
        loader.globals.get("g:autoload_value"),
        Some(&Value::Integer(42))
    );

    // Each sourced file gets a distinct script namespace.
    assert_eq!(
        loader.globals.get("s0:private_value"),
        Some(&Value::Integer(11))
    );
    assert_eq!(
        loader.globals.get("s1:private_value"),
        Some(&Value::Integer(22))
    );

    let host = loader.host.as_mut().expect("host runtime");
    let mapping = host
        .mapping(MapMode::Normal, "<leader>w", None)
        .expect("normal mapping");
    assert!(mapping.flags.non_recursive && mapping.flags.silent);
    assert!(
        matches!(&mapping.expansion, MappingExpansion::Keys(keys) if keys == ":BasicSet mapped<CR>")
    );
    assert_eq!(
        host.events.handlers["BufEnter"][0].group.as_deref(),
        Some("BasicPlugin")
    );
    let event = Event {
        name: "BufEnter".into(),
        pattern: Some("notes.txt".into()),
        payload: Default::default(),
    };
    let commands = host.event_commands(&event, HostContext::default());
    assert_eq!(commands.len(), 1);
    let expanded = host.prepare_command(commands[0].clone()).unwrap();
    assert_eq!(expanded.command.name, "set");
    assert_eq!(expanded.command.arguments, "plugin_value=event");
    assert!(
        host.event_commands(&event, HostContext::default())
            .is_empty()
    );

    let autoload = loader
        .autoload_for("demo#util#answer")
        .expect("autoload entry");
    assert!(autoload.ends_with("autoload/demo/util.vim"));
    assert!(
        loader
            .loaded_scripts
            .contains_key(&autoload.canonicalize().unwrap())
    );

    let state = editor.snapshot().unwrap();
    assert_eq!(state.write_count, 1);
    assert_eq!(
        state.options.get("plugin_value"),
        Some(&Value::String("enabled".into()))
    );
    assert_eq!(state.command_log.len(), 2);

    // Startup loading is idempotent.
    let second_report = loader.load_startup_plugins();
    assert!(second_report.loaded.is_empty());
    assert_eq!(editor.snapshot().unwrap().write_count, 1);
}

#[test]
fn missing_autoload_files_are_reported_during_discovery() {
    let temporary = std::env::temp_dir().join(format!(
        "vim-script-missing-autoload-{}",
        std::process::id()
    ));
    let plugin = temporary.join("plugin");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(
        plugin.join("missing.vim"),
        "let g:value = missing#feature#run()\n",
    )
    .unwrap();

    let mut loader = ScriptLoader::new(RuntimePath::new([temporary.clone()]));
    let report = loader.load_startup_plugins();
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].stage, CompatibilityStage::Discovery);
    assert!(report.failures[0].message.contains("missing#feature#run"));

    std::fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn reports_plugin_failures_without_losing_successful_scripts() {
    let temporary =
        std::env::temp_dir().join(format!("vim-script-plugin-report-{}", std::process::id()));
    let plugin = temporary.join("plugin");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("good.vim"), "let g:good_loaded = 1\n").unwrap();
    std::fs::write(plugin.join("bad.vim"), "if 1\nlet g:never = 1\n").unwrap();

    let mut loader = ScriptLoader::new(RuntimePath::new([temporary.clone()]));
    let report = loader.load_startup_plugins();
    assert_eq!(report.discovered.len(), 2);
    assert_eq!(report.loaded.len(), 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(
        loader.globals.get("g:good_loaded"),
        Some(&Value::Integer(1))
    );

    std::fs::remove_dir_all(temporary).unwrap();
}
