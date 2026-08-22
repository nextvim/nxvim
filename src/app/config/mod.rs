use std::collections::HashMap;
use vim_buffer::BufferId;
use vim_ui::WindowId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigValue {
    Bool(bool),
    Number(i64),
    String(String),
}

impl ConfigValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<i64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionScope {
    Global,
    BufferLocal,
    WindowLocal,
}

#[derive(Debug, Clone)]
pub struct OptionSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub default_value: ConfigValue,
    pub scope: OptionScope,
    pub description: &'static str,
}

pub struct ConfigRegistry {
    specs: HashMap<String, OptionSpec>,
    alias_to_name: HashMap<String, String>,
}

impl ConfigRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            specs: HashMap::new(),
            alias_to_name: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    pub fn register(&mut self, spec: OptionSpec) {
        let name = spec.name.to_string();
        for &alias in spec.aliases {
            self.alias_to_name.insert(alias.to_string(), name.clone());
        }
        self.specs.insert(name, spec);
    }

    pub fn lookup(&self, name_or_alias: &str) -> Option<&OptionSpec> {
        let canonical_name = self
            .alias_to_name
            .get(name_or_alias)
            .map(|s| s.as_str())
            .unwrap_or(name_or_alias);
        self.specs.get(canonical_name)
    }

    fn register_defaults(&mut self) {
        self.register(OptionSpec {
            name: "number",
            aliases: &["nu"],
            default_value: ConfigValue::Bool(false),
            scope: OptionScope::Global,
            description: "Show line numbers",
        });
        self.register(OptionSpec {
            name: "relativenumber",
            aliases: &["rnu"],
            default_value: ConfigValue::Bool(false),
            scope: OptionScope::WindowLocal,
            description: "Show relative line numbers",
        });
        self.register(OptionSpec {
            name: "cursorline",
            aliases: &["cul"],
            default_value: ConfigValue::Bool(false),
            scope: OptionScope::Global,
            description: "Show line cursorline",
        });
        self.register(OptionSpec {
            name: "wrap",
            aliases: &["wrap"],
            default_value: ConfigValue::Bool(false),
            scope: OptionScope::Global,
            description: "Wrap text on overflow",
        });
        self.register(OptionSpec {
            name: "tabstop",
            aliases: &["ts"],
            default_value: ConfigValue::Number(8),
            scope: OptionScope::BufferLocal,
            description: "Number of spaces that a <Tab> in the file counts for",
        });
        self.register(OptionSpec {
            name: "shiftwidth",
            aliases: &["sw"],
            default_value: ConfigValue::Number(8),
            scope: OptionScope::BufferLocal,
            description: "Number of spaces to use for each step of (auto)indent",
        });
        self.register(OptionSpec {
            name: "expandtab",
            aliases: &["et"],
            default_value: ConfigValue::Bool(false),
            scope: OptionScope::BufferLocal,
            description: "Use spaces instead of tabs",
        });
        self.register(OptionSpec {
            name: "inspect",
            aliases: &[],
            default_value: ConfigValue::String("none".to_string()),
            scope: OptionScope::Global,
            description: "Inspect style (none, treesitter, textmate, indexer)",
        });
    }
}

pub struct ConfigStore {
    registry: ConfigRegistry,
    global: HashMap<String, ConfigValue>,
    buffer_local: HashMap<BufferId, HashMap<String, ConfigValue>>,
    window_local: HashMap<WindowId, HashMap<String, ConfigValue>>,
}

impl ConfigStore {
    pub fn new() -> Self {
        Self {
            registry: ConfigRegistry::new(),
            global: HashMap::new(),
            buffer_local: HashMap::new(),
            window_local: HashMap::new(),
        }
    }

    pub fn registry(&self) -> &ConfigRegistry {
        &self.registry
    }

    pub fn get(
        &self,
        name_or_alias: &str,
        buffer_id: Option<BufferId>,
        window_id: Option<WindowId>,
    ) -> Option<ConfigValue> {
        let spec = self.registry.lookup(name_or_alias)?;
        let name = spec.name;

        match spec.scope {
            OptionScope::WindowLocal => {
                if let Some(w_id) = window_id {
                    if let Some(val) = self.window_local.get(&w_id).and_then(|m| m.get(name)) {
                        return Some(val.clone());
                    }
                }
            }
            OptionScope::BufferLocal => {
                if let Some(b_id) = buffer_id {
                    if let Some(val) = self.buffer_local.get(&b_id).and_then(|m| m.get(name)) {
                        return Some(val.clone());
                    }
                }
            }
            OptionScope::Global => {}
        }

        if let Some(val) = self.global.get(name) {
            return Some(val.clone());
        }

        Some(spec.default_value.clone())
    }

    pub fn set(
        &mut self,
        name_or_alias: &str,
        value: ConfigValue,
        buffer_id: Option<BufferId>,
        window_id: Option<WindowId>,
    ) -> Result<(), String> {
        let spec = self
            .registry
            .lookup(name_or_alias)
            .ok_or_else(|| format!("Unknown option: {name_or_alias}"))?;
        let name = spec.name.to_string();

        // Validate type
        match (&spec.default_value, &value) {
            (ConfigValue::Bool(_), ConfigValue::Bool(_)) => {}
            (ConfigValue::Number(_), ConfigValue::Number(_)) => {}
            (ConfigValue::String(_), ConfigValue::String(_)) => {}
            _ => return Err(format!("Invalid type for option: {name}")),
        }

        let mut final_value = value;
        if name == "inspect" {
            if let ConfigValue::String(ref s) = final_value {
                let s_stripped = s.trim_matches('"').trim_matches('\'');
                if s_stripped != "none" && s_stripped != "treesitter" && s_stripped != "textmate" && s_stripped != "indexer" {
                    return Err(format!("Invalid value for option inspect: {s}"));
                }
                final_value = ConfigValue::String(s_stripped.to_string());
            }
        }

        match spec.scope {
            OptionScope::WindowLocal => {
                if let Some(w_id) = window_id {
                    self.window_local
                        .entry(w_id)
                        .or_default()
                        .insert(name, final_value);
                } else {
                    self.global.insert(name.clone(), final_value.clone());
                    // Apply to all windows if it's set globally
                    for w_store in self.window_local.values_mut() {
                        w_store.insert(name.clone(), final_value.clone());
                    }
                }
            }
            OptionScope::BufferLocal => {
                if let Some(b_id) = buffer_id {
                    self.buffer_local
                        .entry(b_id)
                        .or_default()
                        .insert(name, final_value);
                } else {
                    self.global.insert(name.clone(), final_value.clone());
                    for b_store in self.buffer_local.values_mut() {
                        b_store.insert(name.clone(), final_value.clone());
                    }
                }
            }
            OptionScope::Global => {
                self.global.insert(name, final_value);
            }
        }

        Ok(())
    }

    pub fn execute_set_command(
        &mut self,
        arguments: &str,
        buffer_id: Option<BufferId>,
        window_id: Option<WindowId>,
    ) -> Result<Option<String>, String> {
        let args_str = arguments.trim();
        if args_str.is_empty() {
            return Ok(Some(
                "number relativenumber tabstop shiftwidth expandtab".to_string(),
            ));
        }

        let mut output = Vec::new();
        let parts = args_str
            .split(|c| c == ' ' || c == ',' || c == '\t')
            .filter(|s| !s.is_empty());
        for part in parts {
            if part.ends_with('?') {
                let opt_name = &part[..part.len() - 1];
                let spec = self
                    .registry
                    .lookup(opt_name)
                    .ok_or_else(|| format!("Unknown option: {opt_name}"))?;
                let canonical_name = spec.name;
                let val = self
                    .get(opt_name, buffer_id, window_id)
                    .ok_or_else(|| format!("Unknown option: {opt_name}"))?;
                match val {
                    ConfigValue::Bool(b) => output.push(format!("{}={}", canonical_name, b)),
                    ConfigValue::Number(n) => output.push(format!("{}={}", canonical_name, n)),
                    ConfigValue::String(s) => output.push(format!("{}={}", canonical_name, s)),
                }
            } else if part.contains('=') {
                let mut split = part.splitn(2, '=');
                let opt_name = split.next().unwrap().trim();
                let val_str = split.next().unwrap().trim();
                let spec = self
                    .registry
                    .lookup(opt_name)
                    .ok_or_else(|| format!("Unknown option: {opt_name}"))?;
                let val = match &spec.default_value {
                    ConfigValue::Bool(_) => {
                        let b = val_str
                            .parse::<bool>()
                            .map_err(|_| format!("Invalid bool value: {val_str}"))?;
                        ConfigValue::Bool(b)
                    }
                    ConfigValue::Number(_) => {
                        let n = val_str
                            .parse::<i64>()
                            .map_err(|_| format!("Invalid number value: {val_str}"))?;
                        ConfigValue::Number(n)
                    }
                    ConfigValue::String(_) => ConfigValue::String(val_str.to_string()),
                };
                self.set(opt_name, val, buffer_id, window_id)?;
            } else if part.starts_with("no") {
                let opt_name = &part[2..];
                if self.registry.lookup(opt_name).is_some() {
                    self.set(opt_name, ConfigValue::Bool(false), buffer_id, window_id)?;
                } else {
                    if self.registry.lookup(part).is_some() {
                        self.set(part, ConfigValue::Bool(true), buffer_id, window_id)?;
                    } else {
                        return Err(format!("Unknown option: {part}"));
                    }
                }
            } else {
                self.set(part, ConfigValue::Bool(true), buffer_id, window_id)?;
            }
        }

        if output.is_empty() {
            Ok(None)
        } else {
            Ok(Some(output.join(" ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_store_basic() {
        let mut store = ConfigStore::new();
        let w_id = WindowId::new(1);
        let b_id = BufferId::new(1).unwrap();

        // Default values
        assert_eq!(
            store.get("number", Some(b_id), Some(w_id)),
            Some(ConfigValue::Bool(false))
        );
        assert_eq!(
            store.get("tabstop", Some(b_id), Some(w_id)),
            Some(ConfigValue::Number(8))
        );

        // Set local values
        store
            .set("number", ConfigValue::Bool(true), Some(b_id), Some(w_id))
            .unwrap();
        assert_eq!(
            store.get("number", Some(b_id), Some(w_id)),
            Some(ConfigValue::Bool(true))
        );

        // Set command parsing
        store
            .execute_set_command("ts=4 nonumber", Some(b_id), Some(w_id))
            .unwrap();
        assert_eq!(
            store.get("tabstop", Some(b_id), Some(w_id)),
            Some(ConfigValue::Number(4))
        );
        assert_eq!(
            store.get("number", Some(b_id), Some(w_id)),
            Some(ConfigValue::Bool(false))
        );

        // Querying
        let query_res = store
            .execute_set_command("ts?", Some(b_id), Some(w_id))
            .unwrap();
        assert_eq!(query_res, Some("tabstop=4".to_string()));

        // Commas separation
        store
            .execute_set_command("number, rnu", Some(b_id), Some(w_id))
            .unwrap();
        assert_eq!(
            store.get("number", Some(b_id), Some(w_id)),
            Some(ConfigValue::Bool(true))
        );
        assert_eq!(
            store.get("relativenumber", Some(b_id), Some(w_id)),
            Some(ConfigValue::Bool(true))
        );
    }
}
