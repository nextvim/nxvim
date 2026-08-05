mod app;
mod controller;
mod event;
mod presentation;
mod state;
mod terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
