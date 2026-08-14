use std::path::{Path, PathBuf};
use std::sync::Arc;

use vim_script::host::{Capability, CommandDefinition, HostRuntime};
use vim_script::mock_editor::MockEditor;
use vim_script::plugin::{CompatibilityStage, RuntimePath, ScriptLoader};
use vim_script::runtime::Value;

fn runtime_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/runtime")
}

fn command(name: &str, minimum_abbreviation: usize, capability: Capability) -> CommandDefinition {
    CommandDefinition::new(name, minimum_abbreviation)
        .with_capabilities(vec![capability])
}

#[test]
fn loads_and_applies_upstream_desert_colorscheme() {
    let editor = MockEditor::default();
    let mut host = HostRuntime::new(Arc::new(editor.clone()));
    host.capabilities.grant(Capability::Settings);
    host.capabilities.grant(Capability::UserInterface);
    host.capabilities.grant(Capability::Editor);
    host.register_command(command("set", 2, Capability::Settings));
    host.register_command(command("highlight", 2, Capability::UserInterface));
    host.register_command(command("syntax", 3, Capability::Editor));

    let mut loader = ScriptLoader::with_host(RuntimePath::new([runtime_root()]), host);
    loader
        .globals
        .insert(":syntax_on".into(), Value::Integer(1));
    let path = loader
        .load_colorscheme("desert")
        .expect("colorscheme loads");
    assert!(path.ends_with("colors/desert.vim"));
    assert_eq!(
        loader.globals.get("g:colors_name"),
        Some(&Value::String("desert".into()))
    );

    let state = editor.snapshot().unwrap();
    assert_eq!(
        state.options.get("background"),
        Some(&Value::String("dark".into()))
    );
    assert_eq!(state.highlights.clear_count, 1);
    assert_eq!(state.syntax_reset_count, 1);
    assert_eq!(
        state.highlights.groups["Normal"]
            .attributes
            .get("guifg")
            .map(String::as_str),
        Some("White")
    );
    assert_eq!(
        state.highlights.groups["Normal"]
            .attributes
            .get("guibg")
            .map(String::as_str),
        Some("grey20")
    );
    assert_eq!(
        state.highlights.groups["Comment"]
            .attributes
            .get("guifg")
            .map(String::as_str),
        Some("SkyBlue")
    );
    assert_eq!(
        state.highlights.groups["Comment"]
            .attributes
            .get("ctermfg")
            .map(String::as_str),
        Some("darkcyan")
    );
    assert_eq!(
        state.highlights.groups["StatusLine"]
            .attributes
            .get("cterm")
            .map(String::as_str),
        Some("bold,reverse")
    );
}

#[test]
fn missing_colorschemes_report_discovery_failure() {
    let mut loader = ScriptLoader::new(RuntimePath::new([runtime_root()]));
    let failure = loader.load_colorscheme("does-not-exist").unwrap_err();
    assert_eq!(failure.stage, CompatibilityStage::Discovery);
    assert!(failure.message.contains("does-not-exist"));
}
