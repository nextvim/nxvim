mod app;
mod terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
