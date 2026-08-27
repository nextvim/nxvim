mod app;
mod kernel;
mod model;
mod runtime;
pub mod script;
mod terminal;
mod view;

#[macro_use]
extern crate nxvim_log;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    runtime::run()
}
