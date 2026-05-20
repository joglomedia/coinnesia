use anyhow::Result;
use clap::{Parser, Subcommand};
use coinnesia::{app, config::AppConfig, scanner::Scanner, storage};
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Parser)]
#[command(name = "coinnesia")]
#[command(about = "Async multi-asset trading signal scanner", version)]
struct Cli {
    #[arg(
        short,
        long,
        default_value = "config/default.toml",
        env = "COINNESIA_CONFIG"
    )]
    config: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate configuration and print a concise summary.
    CheckConfig,
    /// Run Axum API and supervised background workers.
    Serve,
    /// Run configured Postgres migrations.
    Migrate,
    /// Run one scan cycle using the configured scanner.
    ScanOnce,
    /// Placeholder entry point for future live scanner loop.
    Scan,
    /// Placeholder entry point for future paper/live trading loop.
    Trade {
        #[arg(long)]
        paper: bool,
    },
    /// Placeholder entry point for future backtest runner.
    Backtest,
}

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cli = Cli::parse();
    let config = AppConfig::from_file(&cli.config)?;

    match cli.command {
        Command::CheckConfig => {
            info!(
                symbols = config.symbols.len(),
                exchange = %config.exchange.platform,
                trading_mode = %config.trading.mode,
                server_enabled = config.server.enabled,
                database_enabled = config.database.enabled,
                cache_enabled = config.cache.enabled,
                "configuration loaded"
            );
        }
        Command::Serve => {
            app::serve(config).await?;
        }
        Command::Migrate => {
            storage::migrate_from_config(&config.database).await?;
        }
        Command::ScanOnce => {
            let scanner = Scanner::new(config);
            let report = scanner.scan_once().await?;
            info!(
                scanned = report.scanned,
                signals = report.signals.len(),
                "scan cycle completed"
            );
        }
        Command::Scan => {
            let scanner = Scanner::new(config);
            scanner.run().await?;
        }
        Command::Trade { paper } => {
            info!(
                paper,
                "trade command scaffolded; trading service wiring pending"
            );
        }
        Command::Backtest => {
            coinnesia::backtest::BacktestEngine::new(config)
                .run()
                .await?;
        }
    }

    Ok(())
}
