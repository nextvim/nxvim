mod app;
mod commandline;
mod controller;
mod display;
mod editor;
mod event;
mod script;
mod services;
mod state;
mod terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
