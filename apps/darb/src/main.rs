//! darb binary — bootstrap only (Arquitetura §25, Contribuição §8).
//! Dispatches CLI commands; each command lives in its own module.

use darb_core::t;

mod doctor;
mod init_cmd;
mod tui_loop;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        // The TUI loads the locale from the config itself.
        None => tui_loop::run(),
        Some("version" | "--version" | "-V") => {
            println!("{}", t!("cli.version", version = env!("CARGO_PKG_VERSION")));
            0
        }
        Some("doctor") => doctor::run(),
        Some("init") => init_cmd::run(),
        Some(other) => {
            // User-visible text: pick up the configured language first.
            tui_loop::init_i18n();
            eprintln!("{}", t!("cli.unknown_command", command = other));
            eprintln!();
            eprintln!("{}", t!("cli.usage"));
            2
        }
    }
}
