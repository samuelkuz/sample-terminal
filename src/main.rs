#![allow(unexpected_cfgs)]

mod app;
mod app_state;
mod input;
mod layout;
mod renderer;
mod session;
mod terminal_buffer;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
