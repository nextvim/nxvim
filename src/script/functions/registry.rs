use super::{EditorState, buffer};
use vim_script::{
    host::{Arity, Capability, HostRuntime},
    runtime::{RuntimeResult, Value},
};

type SyncHandler = fn(&EditorState, &[Value]) -> RuntimeResult<Value>;

struct FunctionSpec {
    name: &'static str,
    arity: Arity,
    capability: Capability,
    sync_handler: Option<SyncHandler>,
}

const FUNCTION_SPECS: &[FunctionSpec] = &[
    FunctionSpec {
        name: "echo",
        arity: Arity::Exact(1),
        capability: Capability::Editor,
        sync_handler: None,
    },
    FunctionSpec {
        name: "message",
        arity: Arity::Exact(1),
        capability: Capability::Editor,
        sync_handler: None,
    },
    FunctionSpec {
        name: "echomsg",
        arity: Arity::Exact(1),
        capability: Capability::Editor,
        sync_handler: None,
    },
    FunctionSpec {
        name: "bufnr",
        arity: Arity::Range { min: 0, max: 1 },
        capability: Capability::BufferRead,
        sync_handler: Some(buffer::bufnr),
    },
    FunctionSpec {
        name: "bufexists",
        arity: Arity::Exact(1),
        capability: Capability::BufferRead,
        sync_handler: Some(buffer::bufexists),
    },
    FunctionSpec {
        name: "getline",
        arity: Arity::Range { min: 1, max: 2 },
        capability: Capability::BufferRead,
        sync_handler: Some(buffer::getline),
    },
    FunctionSpec {
        name: "getbufline",
        arity: Arity::Range { min: 2, max: 3 },
        capability: Capability::BufferRead,
        sync_handler: Some(buffer::getbufline),
    },
    FunctionSpec {
        name: "getbufoneline",
        arity: Arity::Exact(2),
        capability: Capability::BufferRead,
        sync_handler: Some(buffer::getbufoneline),
    },
];

pub(super) fn register(host: &mut HostRuntime) {
    for spec in FUNCTION_SPECS {
        host.register_function(spec.name, spec.arity.clone(), vec![spec.capability.clone()]);
    }
}

pub(super) fn sync_handler(name: &str) -> Option<SyncHandler> {
    FUNCTION_SPECS
        .iter()
        .find(|spec| spec.name == name)
        .and_then(|spec| spec.sync_handler)
}
