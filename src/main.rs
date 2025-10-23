mod error;
mod ethereum;
mod server;
mod tools;

use anyhow::Result;
use clap::Parser;
use tracing::{info, error};
use tracing_subscriber;

use crate::ethereum::EthereumService;
use crate::server::EthereumMCPServer;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Ethereum RPC URL (e.g., Infura, Alchemy)
    #[arg(long, default_value = "http://localhost:8545")]
    rpc_url: String,

    /// CoinGecko API key (optional, for price data)
    #[arg(long)]
    coingecko_api_key: Option<String>,

    /// Uniswap V2 Router address
    #[arg(long, default_value = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D")]
    uniswap_router: String,

    /// Log level (debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Log file path (optional)
    #[arg(long)]
    log_file: Option<String>,
}

fn init_logging(log_level: &str, log_file: Option<&str>) -> Result<()> {
    let level = match log_level.to_lowercase().as_str() {
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    if let Some(log_file) = log_file {
        let file_appender = tracing_appender::rolling::never("", log_file);
        subscriber
            .with_writer(file_appender)
            .with_ansi(false)
            .init();
    } else {
        subscriber.init();
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 初始化日志
    init_logging(&args.log_level, args.log_file.as_deref())?;

    info!("Starting Ethereum MCP Server...");
    info!("RPC URL: {}", args.rpc_url);
    info!("Uniswap Router: {}", args.uniswap_router);

    // 初始化以太坊服务
    let ethereum_service = EthereumService::new(
        args.rpc_url,
        args.uniswap_router,
        args.coingecko_api_key,
    )?;

    let mcp_server = EthereumMCPServer::new(ethereum_service);

    // 运行服务器
    if let Err(e) = mcp_server.run_server().await {
        error!("Server error: {}", e);
        return Err(e);
    }

    Ok(())
}
