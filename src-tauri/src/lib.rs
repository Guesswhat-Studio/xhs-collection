mod ai;
mod app_logs;
mod app_runtime;
mod commands;
mod constants;
mod models;
mod secrets;
mod storage;
mod utils;
mod xhs;
mod xhs_extract;
mod xhs_scripts;

pub fn run() {
    app_runtime::run();
}
