#![allow(unexpected_cfgs)]

mod app;
mod renderer;
mod session;
mod terminal_buffer;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
