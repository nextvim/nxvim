mod app;
mod commandline;
mod controller;
mod editor;
mod event;
mod script;
mod state;
mod terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
