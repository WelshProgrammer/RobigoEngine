//! Robigo: a modern engine for The Settlers II, written in Rust.

mod compare;
mod error;
mod game;
mod mods;
mod retail;
mod scene;
#[cfg(test)]
mod scratch;
mod settings;
mod view;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "robigo", version = s2_mods::ROBIGO_VERSION, about = "A modern engine for The Settlers II, written in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Play: a map, a saved game, or a window with no world yet.
    Play(game::Args),
    /// Open a map in the map viewer, or render one view of it to a PNG.
    View(view::Args),
    /// Compare a map's starting view with a screenshot of the original game.
    Compare(compare::Args),
    /// Bake the base mod from the game data again, which the game otherwise does when it needs to.
    Bake(mods::Args),
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = if std::env::args_os().len() > 1 {
        Cli::parse()
    } else {
        Cli::parse_from(["robigo", "play"])
    };
    let result: Result<(), Box<dyn std::error::Error>> = match cli.command {
        Command::Play(args) => game::play(&args).map_err(Into::into),
        Command::View(args) => view::run(&args).map_err(Into::into),
        Command::Compare(args) => compare::run(&args).map_err(Into::into),
        Command::Bake(args) => mods::run(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
