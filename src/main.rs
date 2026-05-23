use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{io::IsTerminal, sync::Arc};

use coinnesia::{
    app,
    cache::Cache,
    config::AppConfig,
    data::PerSymbolMarketData,
    scanner::Scanner,
    storage::{self, Db},
};
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
    /// Run the scanner continuously at the configured interval.
    Scan,
    /// Placeholder entry point for future paper/live trading loop.
    Trade {
        #[arg(long)]
        paper: bool,
    },
    /// Run the backtest scaffold.
    Backtest,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .with_ansi(std::io::stderr().is_terminal())
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
            let db = Db::connect_optional(&config.database).await?;
            let cache = Cache::connect_optional(&config.cache).await?;
            let scanner = Scanner::with_resources(
                config.clone(),
                Arc::new(PerSymbolMarketData::from_config(&config)),
                db,
                cache,
            );
            let report = scanner.scan_once().await?;
            info!(
                cycle_id = %report.cycle_id,
                scanned = report.scanned,
                signals = report.signals.len(),
                "scan cycle completed"
            );
            for signal in &report.signals {
                let ep = signal.entry_plan.as_ref().map(|ep| {
                    format!(
                        "ew1={:.2}~{:.2} tp1={:.2} tp2={:.2} tp3={:.2} sl={:.2}",
                        ep.ew1.low, ep.ew1.high,
                        ep.take_profits.tp1, ep.take_profits.tp2, ep.take_profits.tp3_optional,
                        ep.stop_loss.price
                    )
                });
                info!(
                    symbol = %signal.symbol,
                    state = ?signal.state,
                    long  = signal.confidence.long,
                    short = signal.confidence.short,
                    gap   = signal.confidence.directional_gap,
                    reason = %signal.reason,
                    entry_plan = ep.as_deref().unwrap_or("—"),
                    "signal"
                );
            }
        }
        Command::Scan => {
            let db = Db::connect_optional(&config.database).await?;
            let cache = Cache::connect_optional(&config.cache).await?;
            let scanner = Scanner::with_resources(
                config.clone(),
                Arc::new(PerSymbolMarketData::from_config(&config)),
                db,
                cache,
            );
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
