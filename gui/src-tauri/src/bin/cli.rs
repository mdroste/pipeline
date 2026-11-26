// Source builds retain the standalone CLI; releases share the desktop engine.
#[cfg(feature = "cli-launcher")]
#[path = "../cli/launcher.rs"]
mod launcher;

fn main() {
    #[cfg(feature = "cli-launcher")]
    launcher::main();
    #[cfg(not(feature = "cli-launcher"))]
    pipeline_gui_lib::cli::run(std::env::args().skip(1).collect());
}
