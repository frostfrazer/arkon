mod commands;
mod json_output;
mod print;

use clap::{Parser, Subcommand};
use tracing_subscriber::{fmt, EnvFilter};

/// ARKON — Automated Runtime & Kernel Orchestration Node
///
/// Deploy any project to infrastructure you control.
/// Zero platform fees. One command.
#[derive(Parser)]
#[command(
    name    = "arkon",
    version = env!("CARGO_PKG_VERSION"),
    author  = "ARKON Contributors",
    about   = "Universal local-first deployment agent",
    long_about = None,
    arg_required_else_help = false,
)]
struct Cli {
    /// Increase verbosity (-v debug, -vv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Path to project root (default: current directory)
    #[arg(long, global = true, default_value = ".")]
    root: std::path::PathBuf,

    /// Output results as machine-readable JSON (suppresses interactive prompts)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Detect project type and deploy interactively (default when no subcommand given)
    Ship {
        /// Target name from arkon.toml (skips interactive prompt)
        #[arg(short, long)]
        target: Option<String>,
        /// Skip confirmation prompts
        #[arg(short = 'y', long)]
        yes: bool,
        /// Dry run — build but do not push
        #[arg(long)]
        dry_run: bool,
    },

    /// Detect and print project type without deploying
    Detect {
        /// Show all candidates and their confidence scores
        #[arg(short, long)]
        verbose: bool,
    },

    /// Spin up a P2P preview tunnel and return a shareable link
    Preview {
        /// How long the tunnel stays live (e.g. 24h, 2h, 30m)
        #[arg(long, default_value = "24h")]
        ttl: String,
    },

    /// Roll back to a previous deploy snapshot
    Rollback {
        /// Snapshot ID or timestamp prefix (e.g. "2024-11-03" or "abc12345")
        #[arg(long)]
        to: Option<String>,
        /// Target to roll back (default: last deployed target)
        #[arg(short, long)]
        target: Option<String>,
    },

    /// Promote a validated artifact from one environment to another without rebuilding
    Promote {
        /// Source environment name (e.g. staging)
        from: String,
        /// Destination environment name (e.g. production)
        to: String,
    },

    /// Manage encrypted secrets in the ARKON vault
    Secrets {
        #[command(subcommand)]
        action: SecretsCommands,
    },

    /// Show health and uptime status across all configured targets
    Status {
        /// Show status for a specific target only
        #[arg(short, long)]
        target: Option<String>,
    },

    /// Print the immutable audit log
    Log {
        /// Number of entries to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Filter by target name
        #[arg(short, long)]
        target: Option<String>,
        /// Output raw JSON
        #[arg(long)]
        json: bool,
    },

    /// Initialise arkon.toml in the current project
    Init {
        /// Project name (default: directory name)
        #[arg(long)]
        name: Option<String>,
    },

    /// Manage community adapters
    Adapter {
        #[command(subcommand)]
        action: AdapterCommands,
    },

    /// Check all ARKON dependencies and environment readiness
    Doctor,

    /// Estimate deployment cost for all targets without deploying
    Cost {
        /// Check cost for a specific target only
        #[arg(short, long)]
        target: Option<String>,
    },

    /// Start ARKON in daemon mode (system tray + background health monitor)
    Serve,
}

#[derive(Subcommand)]
enum SecretsCommands {
    /// Store a secret (prompts securely for value)
    Set {
        /// Secret key name, e.g. DATABASE_URL
        key: String,
        /// Value (if omitted, read from stdin securely)
        value: Option<String>,
    },
    /// Print the value of a secret to stdout
    Get { key: String },
    /// Delete a secret from the vault
    Delete { key: String },
    /// List all secret keys (not values)
    List,
    /// Export all secrets as a .env file (plaintext — handle with care)
    Export {
        /// Write to this file instead of stdout
        #[arg(short, long)]
        path: Option<std::path::PathBuf>,
    },
    /// Re-encrypt all secrets under a newly derived machine key
    Rotate,
    /// Recover vault on new hardware using 24-word BIP39 mnemonic
    Recover,
    /// Clear the cached session passphrase from the OS keychain
    Lock,
}

#[derive(Subcommand)]
enum AdapterCommands {
    /// Install a community adapter from a Git URL
    Add { url: String },
    /// Remove a community adapter by name
    Remove { name: String },
    /// List all installed adapters
    List,
    /// Reload all community adapters without restarting
    Reload,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Enable JSON output mode before anything else so print:: calls are suppressed
    if cli.json {
        json_output::enable();
    }

    // Initialise logging (suppressed in JSON mode for clean output)
    let level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    if !cli.quiet && !cli.json {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new(format!("arkon={level}"))),
            )
            .without_time()
            .with_target(false)
            .init();
    }

    if !cli.json { print::banner(); }

    let result = match cli.command {
        // Default: no subcommand → interactive ship
        None => commands::ship::run(&cli.root, None, false, false).await,

        Some(Commands::Ship { target, yes, dry_run }) => {
            commands::ship::run(&cli.root, target.as_deref(), yes, dry_run).await
        }
        Some(Commands::Detect { verbose }) => {
            commands::detect::run(&cli.root, verbose)
        }
        Some(Commands::Preview { ttl }) => {
            commands::preview::run(&cli.root, &ttl).await
        }
        Some(Commands::Rollback { to, target }) => {
            commands::rollback::run(&cli.root, to.as_deref(), target.as_deref()).await
        }
        Some(Commands::Promote { from, to }) => {
            commands::promote::run(&cli.root, &from, &to).await
        }
        Some(Commands::Secrets { action }) => {
            commands::secrets::run(action, &cli.root).await
        }
        Some(Commands::Status { target }) => {
            commands::status::run(&cli.root, target.as_deref()).await
        }
        Some(Commands::Log { limit, target, json }) => {
            commands::log::run(&cli.root, limit, target.as_deref(), json)
        }
        Some(Commands::Init { name }) => {
            commands::init::run(&cli.root, name.as_deref())
        }
        Some(Commands::Adapter { action }) => {
            commands::adapter::run(action, &cli.root).await
        }
        Some(Commands::Doctor) => {
            commands::doctor::run(&cli.root)
        }
        Some(Commands::Cost { target }) => {
            commands::cost::run(&cli.root, target.as_deref()).await
        }
        Some(Commands::Serve) => {
            commands::serve::run(&cli.root).await
        }
    };

    if let Err(e) = result {
        if json_output::is_enabled() {
            let cmd = std::env::args().nth(1).unwrap_or_else(|| "unknown".into());
            json_output::output_error(&cmd, &e.to_string());
        } else {
            eprintln!("\n  \x1b[31m✗\x1b[0m  {e}\n");
        }
        std::process::exit(1);
    }
}
