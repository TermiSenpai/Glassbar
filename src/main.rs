#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod log;
mod app;
mod fsx;
mod gfx;
mod i18n;
mod icons;
mod launch;
mod model;
mod theme;
mod ui;
mod update;

fn main() {
    let code = app::run(std::env::args().collect());
    std::process::exit(code);
}
