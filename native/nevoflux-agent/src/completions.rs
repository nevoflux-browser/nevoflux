//! Shell completion generation for NevoFlux CLI.

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

use crate::cli::Cli;

/// Generate shell completions and write to stdout.
pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "nevoflux", &mut io::stdout());
}

/// Print instructions for installing completions.
#[allow(dead_code)]
pub fn print_completion_instructions(shell: Shell) {
    match shell {
        Shell::Bash => {
            println!("# Add to ~/.bashrc:");
            println!("eval \"$(nevoflux completions bash)\"");
        }
        Shell::Zsh => {
            println!("# Add to ~/.zshrc:");
            println!("eval \"$(nevoflux completions zsh)\"");
        }
        Shell::Fish => {
            println!("# Run once:");
            println!("nevoflux completions fish > ~/.config/fish/completions/nevoflux.fish");
        }
        Shell::PowerShell => {
            println!("# Add to PowerShell profile:");
            println!("Invoke-Expression (nevoflux completions powershell | Out-String)");
        }
        _ => {
            println!("# Generate completions:");
            println!("nevoflux completions {:?}", shell);
        }
    }
}
