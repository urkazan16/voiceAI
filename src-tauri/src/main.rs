#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if localflow_lib::cli::invoked(&args) {
        std::process::exit(localflow_lib::cli::run(&args));
    }
    localflow_lib::run();
}
