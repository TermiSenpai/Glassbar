#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod log;
mod fsx;
mod gfx;
mod i18n;
mod icons;
mod model;
mod theme;
mod ui;
mod update;

fn main() {
    std::process::exit(0);
}
