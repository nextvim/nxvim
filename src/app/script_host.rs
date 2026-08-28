//! Bridge between the vim-script host runtime and the app.
//!
//! Real host-function / `:call` support is future work. Today's milestone
//! only exercises the registration, expansion, and event surface of the
//! host, which does not require calling into the host.

use vim_script::host::{Host, HostFuture, HostRequest};
use vim_script::runtime::{RuntimeError, RuntimeErrorKind};

pub struct NullHost;

impl Host for NullHost {
    fn call(&self, request: HostRequest) -> HostFuture {
        Box::pin(async move {
            Err(RuntimeError::coded(
                "E_HOST",
                RuntimeErrorKind::HostError,
                format!("host does not implement function {}", request.function),
            ))
        })
    }
}
