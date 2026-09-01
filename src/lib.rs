mod action;
mod app;
mod cli;
mod input;
mod storage;
mod task;
mod terminal;
mod ui;

/// Runs shtodo using process arguments and local environment state.
///
/// # Errors
///
/// Returns an error when arguments, storage, terminal setup, input, rendering,
/// persistence, or terminal restoration fails.
pub fn run() -> color_eyre::Result<()> {
    match cli::parse_args(std::env::args_os().skip(1))? {
        cli::Command::Help => {
            use std::io::Write as _;
            std::io::stdout()
                .lock()
                .write_all(cli::usage().as_bytes())?;
        }
        cli::Command::Version => {
            use std::io::Write as _;
            writeln!(
                std::io::stdout().lock(),
                "shtodo {}",
                env!("CARGO_PKG_VERSION")
            )?;
        }
        cli::Command::Run(choice) => {
            let home = storage::home_from_environment()?;
            let scope = storage::scope_from_environment(choice)?;
            let store = storage::Store::open(&home, scope)?;
            let app = app::App::new(store.load()?);
            terminal::run(app, &store)?;
        }
    }
    Ok(())
}
