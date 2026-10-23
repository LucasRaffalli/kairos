use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "kairos",
    about = "Kairos, chronomètre de barre de menus pour Monday"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Affiche la version.
    Version,
}

fn main() {
    match Cli::parse().command {
        Command::Version => println!("kairos {}", kairos_core::version()),
    }
}
