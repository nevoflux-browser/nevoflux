//! CLI module with clap-based argument parsing.
//!
//! Provides subcommands for configuration management and setup.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// NevoFlux Agent - AI-powered browser assistant
#[derive(Parser, Debug)]
#[command(name = "nevoflux")]
#[command(version)]
#[command(about = "NevoFlux Agent - AI-powered browser assistant")]
#[command(
    long_about = "NevoFlux is an AI-powered browser assistant that provides \
    intelligent automation and assistance through browser extensions and MCP integration."
)]
// Allow external subcommands (Firefox passes manifest path and extension ID)
#[command(allow_external_subcommands = true)]
pub struct Cli {
    /// Run as MCP server (stdio bridge for Claude Code integration)
    #[arg(long)]
    pub mcp: bool,

    /// Run as daemon (core processing server)
    #[arg(long)]
    pub daemon: bool,

    /// Check daemon status
    #[arg(long)]
    pub status: bool,

    /// Stop the running daemon
    #[arg(long)]
    pub stop: bool,

    /// Config file path (overrides default location)
    #[arg(long, short)]
    pub config: Option<PathBuf>,

    /// Enable verbose output
    #[arg(long, short)]
    pub verbose: bool,

    /// Enable trace output for debugging (writes JSONL to data dir)
    #[arg(long)]
    pub trace: bool,

    /// Run proxy in dev mode (connect to manually-started daemon on port 19500)
    #[arg(long)]
    pub dev: bool,

    /// Override daemon port range start (used when proxy spawns a daemon)
    #[arg(long, hide = true)]
    pub port_start: Option<u16>,

    /// Override daemon port range end (used when proxy spawns a daemon)
    #[arg(long, hide = true)]
    pub port_end: Option<u16>,

    /// Bind to this exact port (used by proxy in managed mode)
    #[arg(long, hide = true)]
    pub port: Option<u16>,

    /// Daemon self-terminates on idle (set by proxy when spawning)
    #[arg(long, hide = true)]
    pub managed: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
#[allow(dead_code)] // External variant's Vec<String> is used in tests but not in main binary
pub enum Commands {
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Run interactive setup wizard
    Setup,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// External subcommand (catches Firefox native messaging args)
    #[command(external_subcommand)]
    External(Vec<String>),
}

/// Configuration subcommand actions.
#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Initialize default configuration
    Init,
    /// Get a configuration value by key
    Get {
        /// The configuration key (e.g., "app.theme")
        key: String,
    },
    /// Set a configuration value
    Set {
        /// The configuration key (e.g., "app.theme")
        key: String,
        /// The value to set (JSON format for complex values)
        value: String,
    },
    /// Delete a configuration value
    Delete {
        /// The configuration key to delete
        key: String,
    },
    /// List configuration values by prefix
    List {
        /// Optional prefix to filter keys (e.g., "app.")
        #[arg(default_value = "")]
        prefix: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_no_args() {
        let cli = Cli::try_parse_from(["nevoflux"]).unwrap();
        assert!(!cli.mcp);
        assert!(!cli.daemon);
        assert!(!cli.status);
        assert!(!cli.stop);
        assert!(cli.config.is_none());
        assert!(!cli.verbose);
        assert!(!cli.dev);
        assert!(cli.port_start.is_none());
        assert!(cli.port_end.is_none());
        assert!(!cli.managed);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_parse_mcp_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "--mcp"]).unwrap();
        assert!(cli.mcp);
    }

    #[test]
    fn test_cli_parse_daemon_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "--daemon"]).unwrap();
        assert!(cli.daemon);
    }

    #[test]
    fn test_cli_parse_status_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "--status"]).unwrap();
        assert!(cli.status);
    }

    #[test]
    fn test_cli_parse_stop_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "--stop"]).unwrap();
        assert!(cli.stop);
    }

    #[test]
    fn test_cli_parse_verbose_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "-v"]).unwrap();
        assert!(cli.verbose);

        let cli = Cli::try_parse_from(["nevoflux", "--verbose"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_cli_parse_config_path() {
        let cli = Cli::try_parse_from(["nevoflux", "-c", "/path/to/config.toml"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.toml")));

        let cli = Cli::try_parse_from(["nevoflux", "--config", "/other/path.toml"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/other/path.toml")));
    }

    #[test]
    fn test_cli_parse_config_show() {
        let cli = Cli::try_parse_from(["nevoflux", "config", "show"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Config {
                action: ConfigAction::Show
            })
        ));
    }

    #[test]
    fn test_cli_parse_config_init() {
        let cli = Cli::try_parse_from(["nevoflux", "config", "init"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Config {
                action: ConfigAction::Init
            })
        ));
    }

    #[test]
    fn test_cli_parse_config_get() {
        let cli = Cli::try_parse_from(["nevoflux", "config", "get", "app.theme"]).unwrap();
        match cli.command {
            Some(Commands::Config {
                action: ConfigAction::Get { key },
            }) => {
                assert_eq!(key, "app.theme");
            }
            _ => panic!("Expected Config Get command"),
        }
    }

    #[test]
    fn test_cli_parse_config_set() {
        let cli = Cli::try_parse_from(["nevoflux", "config", "set", "app.theme", "dark"]).unwrap();
        match cli.command {
            Some(Commands::Config {
                action: ConfigAction::Set { key, value },
            }) => {
                assert_eq!(key, "app.theme");
                assert_eq!(value, "dark");
            }
            _ => panic!("Expected Config Set command"),
        }
    }

    #[test]
    fn test_cli_parse_config_set_json_value() {
        let cli = Cli::try_parse_from([
            "nevoflux",
            "config",
            "set",
            "app.settings",
            r#"{"theme":"dark","font_size":14}"#,
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Config {
                action: ConfigAction::Set { key, value },
            }) => {
                assert_eq!(key, "app.settings");
                assert_eq!(value, r#"{"theme":"dark","font_size":14}"#);
            }
            _ => panic!("Expected Config Set command"),
        }
    }

    #[test]
    fn test_cli_parse_config_delete() {
        let cli = Cli::try_parse_from(["nevoflux", "config", "delete", "app.theme"]).unwrap();
        match cli.command {
            Some(Commands::Config {
                action: ConfigAction::Delete { key },
            }) => {
                assert_eq!(key, "app.theme");
            }
            _ => panic!("Expected Config Delete command"),
        }
    }

    #[test]
    fn test_cli_parse_config_list() {
        let cli = Cli::try_parse_from(["nevoflux", "config", "list"]).unwrap();
        match cli.command {
            Some(Commands::Config {
                action: ConfigAction::List { prefix },
            }) => {
                assert_eq!(prefix, "");
            }
            _ => panic!("Expected Config List command"),
        }
    }

    #[test]
    fn test_cli_parse_config_list_with_prefix() {
        let cli = Cli::try_parse_from(["nevoflux", "config", "list", "app."]).unwrap();
        match cli.command {
            Some(Commands::Config {
                action: ConfigAction::List { prefix },
            }) => {
                assert_eq!(prefix, "app.");
            }
            _ => panic!("Expected Config List command"),
        }
    }

    #[test]
    fn test_cli_parse_setup() {
        let cli = Cli::try_parse_from(["nevoflux", "setup"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Setup)));
    }

    #[test]
    fn test_cli_parse_combined_flags() {
        let cli = Cli::try_parse_from(["nevoflux", "--verbose", "--daemon"]).unwrap();
        assert!(cli.verbose);
        assert!(cli.daemon);
    }

    #[test]
    fn test_cli_parse_trace_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "--trace", "--daemon"]).unwrap();
        assert!(cli.trace);
        assert!(cli.daemon);
    }

    #[test]
    fn test_cli_parse_dev_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "--dev"]).unwrap();
        assert!(cli.dev);
    }

    #[test]
    fn test_cli_parse_port_start_end() {
        let cli = Cli::try_parse_from(["nevoflux", "--port-start", "19501", "--port-end", "19600"])
            .unwrap();
        assert_eq!(cli.port_start, Some(19501));
        assert_eq!(cli.port_end, Some(19600));
    }

    #[test]
    fn test_cli_parse_managed_flag() {
        let cli = Cli::try_parse_from(["nevoflux", "--daemon", "--managed"]).unwrap();
        assert!(cli.daemon);
        assert!(cli.managed);
    }

    #[test]
    fn test_cli_parse_port_flag() {
        let cli =
            Cli::try_parse_from(["nevoflux", "--daemon", "--managed", "--port", "19523"]).unwrap();
        assert!(cli.daemon);
        assert!(cli.managed);
        assert_eq!(cli.port, Some(19523));
    }

    #[test]
    fn test_cli_parse_firefox_native_messaging_args() {
        // Firefox passes manifest path and extension ID as arguments
        let cli = Cli::try_parse_from([
            "nevoflux",
            "/home/user/.mozilla/native-messaging-hosts/com.nevoflux.agent.json",
            "agent@nevoflux.com",
        ])
        .unwrap();

        // These should be captured as external subcommand, not cause an error
        match cli.command {
            Some(Commands::External(args)) => {
                assert_eq!(args.len(), 2);
                assert!(args[0].contains("native-messaging-hosts"));
                assert_eq!(args[1], "agent@nevoflux.com");
            }
            _ => panic!("Expected External command for Firefox args"),
        }
    }
}
