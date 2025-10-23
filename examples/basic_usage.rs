//! 基本使用示例
//!
//! 这个示例展示了如何初始化和使用 MCP 服务器

use mcp_ethereum_server::{EthereumService, EthereumMCPServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化以太坊服务
    let ethereum_service = EthereumService::new(
        "https://mainnet.infura.io/v3/YOUR_PROJECT_ID".to_string(),
        "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_string(),
        Some("YOUR_COINGECKO_API_KEY".to_string()),
    )?;

    // 创建 MCP 服务器
    let server = EthereumMCPServer::new(ethereum_service);

    println!("MCP Ethereum Server initialized successfully!");
    println!("Available tools:");
    println!("- get_balance: Query ETH and ERC20 token balances");
    println!("- get_token_price: Get current token prices");
    println!("- swap_tokens: Simulate token swaps on Uniswap");

    Ok(())
}
