mod app;
mod controller;
mod model;
mod runtime;
pub mod script;
mod terminal;
mod view;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    runtime::run()
}
