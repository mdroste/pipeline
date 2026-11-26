#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if pipeline_gui_lib::cli_tools::run_headless_if_requested() {
        return;
    }
    if pipeline_gui_lib::run_command_guard_if_requested() {
        return;
    }
    if pipeline_gui_lib::commands::run_pdf_helper_if_requested() {
        return;
    }
    pipeline_gui_lib::run()
}
