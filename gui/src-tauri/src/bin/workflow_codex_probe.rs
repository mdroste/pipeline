fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("probe runtime");
    match runtime.block_on(pipeline_gui_lib::pipeline::codex_server::probe::run()) {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("probe JSON")
        ),
        Err(error) => {
            eprintln!("Workflow Codex qualification failed: {error}");
            std::process::exit(1);
        }
    }
}
