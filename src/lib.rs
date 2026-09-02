mod action;
mod app;
mod cli;
mod input;
mod storage;
mod task;
mod terminal;
mod ui;

use std::{
    ffi::OsString,
    io::{IsTerminal as _, Read as _, Write as _},
};

use color_eyre::eyre::{Result, WrapErr as _, eyre};

/// Runs shtodo using process arguments and local environment state.
///
/// # Errors
///
/// Returns an error when arguments, storage, terminal setup, input, rendering,
/// persistence, or terminal restoration fails.
pub fn run() -> Result<()> {
    match cli::parse_args(std::env::args_os().skip(1))? {
        cli::Command::Help => {
            std::io::stdout()
                .lock()
                .write_all(cli::usage().as_bytes())?;
        }
        cli::Command::Version => {
            writeln!(
                std::io::stdout().lock(),
                "shtodo {}",
                env!("CARGO_PKG_VERSION")
            )?;
        }
        cli::Command::Add(choice, argument) => {
            add_task(choice, argument)?;
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

fn add_task(choice: cli::ScopeChoice, argument: Option<OsString>) -> Result<()> {
    let text = match argument {
        Some(value) => value
            .into_string()
            .map_err(|value| eyre!("task text is not valid UTF-8: {value:?}"))?,
        None => read_task_from_stdin()?,
    };
    if text.trim().is_empty() {
        return Err(missing_task_text());
    }

    let home = storage::home_from_environment()?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let mut tasks = store.load()?;
    tasks.add(&text)?;
    store.save(&tasks)?;

    writeln!(std::io::stdout().lock(), "Added: {}", text.trim())?;
    Ok(())
}

fn read_task_from_stdin() -> Result<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(missing_task_text());
    }

    let mut text = String::new();
    stdin
        .lock()
        .read_to_string(&mut text)
        .wrap_err("could not read task text from standard input")?;
    Ok(text)
}

fn missing_task_text() -> color_eyre::Report {
    eyre!("task text is required\n\n{}", cli::usage())
}
