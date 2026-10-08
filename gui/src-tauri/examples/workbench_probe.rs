fn main() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Failed to start Workbench probe runtime: {error}");
            std::process::exit(1);
        }
    };
    match runtime.block_on(pipeline_gui_lib::workbench::codex::run_qualification_probe()) {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("Failed to serialize Workbench probe result: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("Workbench qualification probe failed: {error}");
            std::process::exit(1);
        }
    }
}
