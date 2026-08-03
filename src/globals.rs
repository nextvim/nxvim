use std::{collections::HashMap, error::Error, fmt};

pub const VIM_COMPATIBLE_VERSION: i64 = 902;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalValue {
    Null,
    Bool(bool),
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Global {
    value: GlobalValue,
    read_only: bool,
}

#[derive(Debug, Default)]
pub struct Globals {
    values: HashMap<String, Global>,
}

impl Globals {
    pub fn nxvim_defaults() -> Self {
        let mut globals = Self::default();
        globals.define_read_only("v:version", GlobalValue::Integer(VIM_COMPATIBLE_VERSION));
        globals.define_read_only("v:true", GlobalValue::Bool(true));
        globals.define_read_only("v:false", GlobalValue::Bool(false));
        globals.define_read_only("v:null", GlobalValue::Null);
        globals.define_read_only("v:progname", GlobalValue::String("nxvim".to_owned()));
        globals.define("g:nxvim", GlobalValue::Bool(true));
        globals
    }

    pub fn get(&self, name: &str) -> Option<&GlobalValue> {
        self.values.get(name).map(|global| &global.value)
    }

    pub fn is_read_only(&self, name: &str) -> Option<bool> {
        self.values.get(name).map(|global| global.read_only)
    }

    pub fn define(&mut self, name: impl Into<String>, value: GlobalValue) {
        self.insert(name, value, false);
    }

    pub fn define_read_only(&mut self, name: impl Into<String>, value: GlobalValue) {
        self.insert(name, value, true);
    }

    pub fn set(&mut self, name: &str, value: GlobalValue) -> Result<(), GlobalError> {
        let global = self
            .values
            .get_mut(name)
            .ok_or_else(|| GlobalError::Undefined(name.to_owned()))?;
        if global.read_only {
            return Err(GlobalError::ReadOnly(name.to_owned()));
        }
        global.value = value;
        Ok(())
    }

    fn insert(&mut self, name: impl Into<String>, value: GlobalValue, read_only: bool) {
        self.values.insert(name.into(), Global { value, read_only });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalError {
    Undefined(String),
    ReadOnly(String),
}

impl fmt::Display for GlobalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undefined(name) => write!(formatter, "global variable `{name}` is undefined"),
            Self::ReadOnly(name) => write!(formatter, "global variable `{name}` is read-only"),
        }
    }
}

impl Error for GlobalError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_expected_values_and_policy() {
        let globals = Globals::nxvim_defaults();

        assert_eq!(
            globals.get("v:version"),
            Some(&GlobalValue::Integer(VIM_COMPATIBLE_VERSION))
        );
        assert_eq!(globals.get("v:true"), Some(&GlobalValue::Bool(true)));
        assert_eq!(globals.get("v:false"), Some(&GlobalValue::Bool(false)));
        assert_eq!(globals.get("v:null"), Some(&GlobalValue::Null));
        assert_eq!(
            globals.get("v:progname"),
            Some(&GlobalValue::String("nxvim".to_owned()))
        );
        assert_eq!(globals.get("g:nxvim"), Some(&GlobalValue::Bool(true)));
        assert_eq!(globals.is_read_only("v:true"), Some(true));
        assert_eq!(globals.is_read_only("g:nxvim"), Some(false));
    }

    #[test]
    fn read_only_globals_cannot_be_changed() {
        let mut globals = Globals::nxvim_defaults();

        assert_eq!(
            globals.set("v:true", GlobalValue::Bool(false)),
            Err(GlobalError::ReadOnly("v:true".to_owned()))
        );
        assert_eq!(globals.get("v:true"), Some(&GlobalValue::Bool(true)));
    }
}
