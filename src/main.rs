use nxvim::HeadlessEditor;

fn main() {
    if let Err(error) = HeadlessEditor::new() {
        eprintln!("nxvim: {error}");
        std::process::exit(1);
    }
}
