use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::bytecode::{BytecodeModule, Constant, Instruction};
use crate::compiler::Compiler;
use crate::host::{HostContext, HostRuntime};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::resolver::{Resolver, ResolverConfig};
use crate::runtime::{RuntimeError, RuntimeErrorKind, Scheduler, Value, Vm};
use crate::source::{Diagnostic, SourceId, SourceMap};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScriptId(pub u32);

#[derive(Clone, Debug, Default)]
pub struct RuntimePath {
    pub roots: Vec<PathBuf>,
}

impl RuntimePath {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut runtime_path = Self { roots: Vec::new() };
        for root in roots {
            runtime_path.push(root);
        }
        runtime_path
    }

    pub fn push(&mut self, root: impl Into<PathBuf>) {
        let root = root.into();
        let canonical = root.canonicalize().unwrap_or(root);
        if !self.roots.iter().any(|existing| existing == &canonical) {
            self.roots.push(canonical);
        }
    }

    pub fn startup_plugins(&self) -> Vec<PathBuf> {
        let mut regular = Vec::new();
        let mut after = Vec::new();
        for root in &self.roots {
            let mut root_paths = Vec::new();
            collect_vim_files(&root.join("plugin"), false, &mut root_paths);
            root_paths.sort();
            regular.extend(root_paths);
        }
        for root in &self.roots {
            let mut root_paths = Vec::new();
            collect_vim_files(&root.join("after/plugin"), false, &mut root_paths);
            root_paths.sort();
            after.extend(root_paths);
        }
        regular.extend(after);
        let mut seen = std::collections::HashSet::new();
        regular
            .into_iter()
            .filter(|path| seen.insert(path.clone()))
            .collect()
    }

    pub fn colorscheme(&self, name: &str) -> Option<PathBuf> {
        self.roots
            .iter()
            .map(|root| root.join("colors").join(format!("{name}.vim")))
            .find(|path| path.is_file())
    }

    pub fn autoload_files(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for root in &self.roots {
            collect_vim_files(&root.join("autoload"), true, &mut paths);
        }
        paths.sort();
        paths
    }

    pub fn find_autoload(&self, function: &str) -> Option<PathBuf> {
        let mut components: Vec<_> = function.split('#').collect();
        if components.len() < 2 {
            return None;
        }
        components.pop();
        let relative = components.join("/") + ".vim";
        self.roots
            .iter()
            .map(|root| root.join("autoload").join(&relative))
            .find(|path| path.is_file())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityStage {
    Discovery,
    Io,
    Lex,
    Parse,
    Resolve,
    Compile,
    Runtime,
}

#[derive(Clone, Debug)]
pub struct CompatibilityFailure {
    pub path: Option<PathBuf>,
    pub stage: CompatibilityStage,
    pub message: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default)]
pub struct CompatibilityReport {
    pub discovered: Vec<PathBuf>,
    pub loaded: Vec<PathBuf>,
    pub failures: Vec<CompatibilityFailure>,
    pub unsupported_features: HashSet<String>,
}

impl CompatibilityReport {
    pub fn is_compatible(&self) -> bool {
        self.failures.is_empty()
    }
    pub fn record_unsupported(&mut self, feature: impl Into<String>) {
        self.unsupported_features.insert(feature.into());
    }
}

#[derive(Clone, Debug)]
pub struct LoadedScript {
    pub id: ScriptId,
    pub source: SourceId,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ScriptLoader {
    pub runtime_path: RuntimePath,
    pub sources: SourceMap,
    pub loaded_scripts: HashMap<PathBuf, LoadedScript>,
    pub autoload_index: HashMap<String, PathBuf>,
    pub globals: HashMap<String, Value>,
    pub host: Option<HostRuntime>,
    pub instruction_quantum: usize,
}

impl ScriptLoader {
    pub fn new(runtime_path: RuntimePath) -> Self {
        let mut loader = Self {
            runtime_path,
            sources: SourceMap::default(),
            loaded_scripts: HashMap::new(),
            autoload_index: HashMap::new(),
            globals: HashMap::from([("v:version".into(), Value::Integer(900))]),
            host: None,
            instruction_quantum: 10_000,
        };
        loader.rebuild_autoload_index();
        loader
    }

    pub fn with_host(runtime_path: RuntimePath, host: HostRuntime) -> Self {
        let mut loader = Self::new(runtime_path);
        loader.host = Some(host);
        loader
    }

    pub fn rebuild_autoload_index(&mut self) {
        self.autoload_index.clear();
        for path in self.runtime_path.autoload_files() {
            if let Some(prefix) = autoload_prefix(&self.runtime_path, &path) {
                self.autoload_index.entry(prefix).or_insert(path);
            }
        }
    }

    pub fn autoload_for(&self, function: &str) -> Option<PathBuf> {
        self.autoload_index
            .iter()
            .filter(|(prefix, _)| function.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, path)| path.clone())
            .or_else(|| self.runtime_path.find_autoload(function))
    }

    pub fn load_colorscheme(&mut self, name: &str) -> Result<PathBuf, CompatibilityFailure> {
        let path = self
            .runtime_path
            .colorscheme(name)
            .ok_or_else(|| CompatibilityFailure {
                path: None,
                stage: CompatibilityStage::Discovery,
                message: format!("colorscheme not found: {name}"),
                diagnostics: Vec::new(),
            })?;
        self.load_script(&path)?;
        Ok(path)
    }

    pub fn load_startup_plugins(&mut self) -> CompatibilityReport {
        let paths = self.runtime_path.startup_plugins();
        let mut report = CompatibilityReport {
            discovered: paths.clone(),
            ..CompatibilityReport::default()
        };
        for path in paths {
            if self.loaded_scripts.contains_key(&path) {
                continue;
            }
            match self.load_script(&path) {
                Ok(()) => report.loaded.push(path),
                Err(failure) => report.failures.push(failure),
            }
        }
        report
    }

    pub fn load_filetype_scripts(
        &mut self,
        filetype: &str,
        context: HostContext,
    ) -> CompatibilityReport {
        let mut paths = Vec::new();
        for directory in ["ftplugin", "indent"] {
            for root in &self.runtime_path.roots {
                let path = root.join(directory).join(format!("{filetype}.vim"));
                if path.is_file() {
                    paths.push(path);
                }
            }
            for root in &self.runtime_path.roots {
                let path = root
                    .join("after")
                    .join(directory)
                    .join(format!("{filetype}.vim"));
                if path.is_file() {
                    paths.push(path);
                }
            }
        }
        let mut report = CompatibilityReport {
            discovered: paths.clone(),
            ..CompatibilityReport::default()
        };
        for path in paths {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if self.loaded_scripts.contains_key(&canonical) {
                continue;
            }
            match self.load_script_with_context(&path, context.clone()) {
                Ok(()) => report.loaded.push(canonical),
                Err(failure) => report.failures.push(failure),
            }
        }
        report
    }

    pub fn load_script(&mut self, path: &Path) -> Result<(), CompatibilityFailure> {
        self.load_script_with_context(path, HostContext::default())
    }

    pub fn load_script_with_context(
        &mut self,
        path: &Path,
        context: HostContext,
    ) -> Result<(), CompatibilityFailure> {
        let canonical = path.canonicalize().map_err(|error| {
            failure(path, CompatibilityStage::Io, error.to_string(), Vec::new())
        })?;
        if self.loaded_scripts.contains_key(&canonical) {
            return Ok(());
        }
        let text = fs::read_to_string(&canonical).map_err(|error| {
            failure(
                &canonical,
                CompatibilityStage::Io,
                error.to_string(),
                Vec::new(),
            )
        })?;
        let source = self.sources.add_path(canonical.clone(), text.clone());
        let script = ScriptId(source.0);
        let lexed = Lexer::new(source, &text).lex();
        if !lexed.diagnostics.is_empty() {
            return Err(failure(
                &canonical,
                CompatibilityStage::Lex,
                "lexing failed",
                lexed.diagnostics,
            ));
        }
        let parsed = Parser::new_with_source(&lexed.tokens, &text).parse();
        if !parsed.diagnostics.is_empty() {
            return Err(failure(
                &canonical,
                CompatibilityStage::Parse,
                "parsing failed",
                parsed.diagnostics,
            ));
        }
        let mut config = ResolverConfig::default();
        if let Some(host) = &self.host {
            config
                .builtins
                .extend(host.functions.names().map(str::to_owned));
        }
        let resolved =
            Resolver::new(config).resolve(parsed.program.expect("parser always returns a program"));
        if !resolved.diagnostics.is_empty() {
            return Err(failure(
                &canonical,
                CompatibilityStage::Resolve,
                "semantic resolution failed",
                resolved.diagnostics,
            ));
        }
        let compiled =
            Compiler::new(&resolved.program.expect("resolver always returns a program")).compile();
        if !compiled.diagnostics.is_empty() {
            return Err(failure(
                &canonical,
                CompatibilityStage::Compile,
                "compilation failed",
                compiled.diagnostics,
            ));
        }
        let module = compiled.module.expect("compiler always returns a module");
        for function in autoload_references(&module) {
            let runtime_name = format!(":{function}");
            if matches!(self.globals.get(&runtime_name), Some(Value::Closure(_))) {
                continue;
            }
            let autoload = self.autoload_for(&function).ok_or_else(|| {
                failure(
                    &canonical,
                    CompatibilityStage::Discovery,
                    format!("no autoload script found for function {function}"),
                    Vec::new(),
                )
            })?;
            self.load_script_with_context(&autoload, context.clone())?;
        }
        let mut vm = Vm::with_globals(module, self.globals.clone())
            .map_err(|error| runtime_failure(&canonical, error))?;
        vm.host_context = context;
        let mut scheduler = Scheduler::new(self.instruction_quantum);
        if let Some(host) = self.host.clone() {
            scheduler.set_host(host);
        }
        let task = scheduler
            .spawn(vm)
            .map_err(|error| runtime_failure(&canonical, error))?;
        scheduler
            .run_until_complete(task)
            .map_err(|error| runtime_failure(&canonical, error))?;
        if let Some(host) = scheduler.host().cloned() {
            self.host = Some(host);
        }
        self.globals = scheduler
            .task(task)
            .expect("completed task exists")
            .vm
            .globals
            .clone();
        self.loaded_scripts.insert(
            canonical.clone(),
            LoadedScript {
                id: script,
                source,
                path: canonical,
            },
        );
        Ok(())
    }
}

fn autoload_references(module: &BytecodeModule) -> Vec<String> {
    let mut references = HashSet::new();
    for function in &module.functions {
        for instruction in &function.code {
            let Instruction::LoadGlobal(id) = instruction else {
                continue;
            };
            let Some(Constant::String(name)) = function.constants.get(id.0 as usize) else {
                continue;
            };
            if let Some(name) = name.strip_prefix(':')
                && name.contains('#')
            {
                references.insert(name.to_owned());
            }
        }
    }
    let mut references: Vec<_> = references.into_iter().collect();
    references.sort();
    references
}

fn collect_vim_files(directory: &Path, recursive: bool, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() && recursive {
            collect_vim_files(&path, true, output);
        } else if path.extension().is_some_and(|extension| extension == "vim") {
            output.push(path);
        }
    }
}

fn autoload_prefix(runtime_path: &RuntimePath, path: &Path) -> Option<String> {
    runtime_path.roots.iter().find_map(|root| {
        let relative = path.strip_prefix(root.join("autoload")).ok()?;
        let mut components: Vec<_> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        let last = components.pop()?;
        components.push(last.strip_suffix(".vim")?.to_owned());
        Some(components.join("#") + "#")
    })
}

fn failure(
    path: &Path,
    stage: CompatibilityStage,
    message: impl Into<String>,
    diagnostics: Vec<Diagnostic>,
) -> CompatibilityFailure {
    CompatibilityFailure {
        path: Some(path.to_owned()),
        stage,
        message: message.into(),
        diagnostics,
    }
}
fn runtime_failure(path: &Path, error: RuntimeError) -> CompatibilityFailure {
    failure(
        path,
        CompatibilityStage::Runtime,
        format!(
            "{}{}",
            error
                .code
                .as_deref()
                .map_or(String::new(), |code| format!("{code}: ")),
            error.message
        ),
        Vec::new(),
    )
}

pub fn missing_feature(name: impl Into<String>) -> RuntimeError {
    RuntimeError::coded(
        "E_NOTIMPL",
        RuntimeErrorKind::HostError,
        format!("unsupported plugin feature: {}", name.into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn startup_plugins_load_regular_roots_before_after_roots() {
        let root = std::env::temp_dir().join(format!("nxvim-runtime-path-{}", std::process::id()));
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir_all(first.join("plugin")).unwrap();
        fs::create_dir_all(first.join("after/plugin")).unwrap();
        fs::create_dir_all(second.join("plugin")).unwrap();
        fs::create_dir_all(second.join("after/plugin")).unwrap();
        for path in [
            first.join("plugin/z.vim"),
            second.join("plugin/a.vim"),
            first.join("after/plugin/a.vim"),
            second.join("after/plugin/z.vim"),
        ] {
            fs::write(path, "\n").unwrap();
        }

        let runtime = RuntimePath::new([first.clone(), second.clone(), first.clone()]);
        let paths = runtime.startup_plugins();
        assert_eq!(
            paths,
            vec![
                first.join("plugin/z.vim"),
                second.join("plugin/a.vim"),
                first.join("after/plugin/a.vim"),
                second.join("after/plugin/z.vim"),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn maps_autoload_function_names_to_paths() {
        let root = PathBuf::from("/runtime");
        let runtime = RuntimePath::new([root.clone()]);
        assert_eq!(runtime.find_autoload("example#util#run"), None);
        assert!(
            autoload_prefix(&runtime, &root.join("autoload/example/util.vim"))
                .is_some_and(|prefix| prefix == "example#util#")
        );
    }
}
