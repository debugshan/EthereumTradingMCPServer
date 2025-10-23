use anyhow::{anyhow, Result};
use clap::Parser;
use ethers::{
    prelude::*,
    types::{Address, U256},
};
use rmcp::{
    McpServer,
    transport::stdio::StdioServerTransport,
    types::{
        CallToolRequest, Content, ImageContent, ListToolsRequest,
        ListToolsResult, Tool, TextContent
    }
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{info, error, warn, debug};
use tracing_subscriber;

// Uniswap V2 Router ABI
abigen!(
    IUniswapV2Router,
    r#"[
        function getAmountsOut(uint amountIn, address[] memory path) public view returns (uint[] memory amounts)
        function swapExactTokensForTokens(uint amountIn, uint amountOutMin, address[] calldata path, address to, uint deadline) external returns (uint[] memory amounts)
    ]"#,
);

// ERC20 ABI
abigen!(
    IERC20,
    r#"[
        function balanceOf(address account) external view returns (uint256)
        function decimals() external view returns (uint8)
        function symbol() external view returns (string memory)
        function name() external view returns (string memory)
    ]"#,
);

// 命令行参数
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

// 余额查询结果
#[derive(Debug, Serialize)]
struct BalanceResult {
    address: String,
    eth_balance: String,
    token_balances: Vec<TokenBalance>,
}

#[derive(Debug, Serialize)]
struct TokenBalance {
    token_address: String,
    name: String,
    symbol: String,
    balance: String,
    decimals: u8,
}

// 价格查询结果
#[derive(Debug, Serialize)]
struct PriceResult {
    token: String,
    usd_price: Option<f64>,
    eth_price: Option<f64>,
    market_cap: Option<f64>,
    last_updated: String,
}

// 交易模拟结果
#[derive(Debug, Serialize)]
struct SwapSimulationResult {
    from_token: String,
    to_token: String,
    amount_in: String,
    expected_amount_out: String,
    min_amount_out: String,
    price_impact: f64,
    gas_estimate: u64,
    gas_cost_eth: String,
    gas_cost_usd: Option<String>,
    route: Vec<String>,
    success: bool,
}

// CoinGecko API 响应
#[derive(Debug, Serialize, Deserialize)]
struct CoinGeckoPriceResponse {
    #[serde(flatten)]
    data: HashMap<String, CoinGeckoTokenData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoinGeckoTokenData {
    usd: Option<f64>,
    eth: Option<f64>,
    market_cap: Option<f64>,
    last_updated_at: Option<u64>,
}

// 以太坊服务
struct EthereumService {
    provider: Provider<Http>,
    uniswap_router: Address,
    coingecko_api_key: Option<String>,
    http_client: reqwest::Client,
}

impl EthereumService {
    fn new(
        rpc_url: String,
        uniswap_router: String,
        coingecko_api_key: Option<String>
    ) -> Result<Self> {
        let provider = Provider::<Http>::try_from(rpc_url)
            .map_err(|e| anyhow!("Failed to create Ethereum provider: {}", e))?;

        let uniswap_router = uniswap_router.parse::<Address>()
            .map_err(|e| anyhow!("Invalid Uniswap router address: {}", e))?;

        let http_client = reqwest::Client::new();

        info!("Ethereum service initialized with router: {}", uniswap_router);

        Ok(EthereumService {
            provider,
            uniswap_router,
            coingecko_api_key,
            http_client,
        })
    }

    async fn get_eth_balance(&self, address: &str) -> Result<String> {
        let address = address.parse::<Address>()
            .map_err(|e| anyhow!("Invalid Ethereum address: {}", e))?;

        debug!("Querying ETH balance for address: {}", address);

        let balance = self.provider.get_balance(address, None).await
            .map_err(|e| anyhow!("Failed to get ETH balance: {}", e))?;

        let balance_eth = ethers::utils::format_ether(balance);
        info!("ETH balance for {}: {}", address, balance_eth);

        Ok(balance_eth)
    }

    async fn get_token_balance(&self, token_address: &str, wallet_address: &str) -> Result<TokenBalance> {
        let token_addr = token_address.parse::<Address>()
            .map_err(|e| anyhow!("Invalid token address: {}", e))?;

        let wallet_addr = wallet_address.parse::<Address>()
            .map_err(|e| anyhow!("Invalid wallet address: {}", e))?;

        debug!("Querying token balance: token={}, wallet={}", token_addr, wallet_addr);

        let erc20 = IERC20::new(token_addr, self.provider.clone());

        let balance = erc20.balance_of(wallet_addr).call().await
            .map_err(|e| anyhow!("Failed to get token balance: {}", e))?;

        let decimals = erc20.decimals().call().await
            .map_err(|e| anyhow!("Failed to get token decimals: {}", e))?;

        let symbol = erc20.symbol().call().await
            .unwrap_or_else(|_| "UNKNOWN".to_string());

        let name = erc20.name().call().await
            .unwrap_or_else(|_| "Unknown Token".to_string());

        let balance_formatted = format_token_amount(balance, decimals);

        info!("Token balance: {} {} for {}", balance_formatted, symbol, wallet_addr);

        Ok(TokenBalance {
            token_address: token_address.to_string(),
            name,
            symbol,
            balance: balance_formatted,
            decimals,
        })
    }

    async fn get_all_balances(&self, wallet_address: &str, token_addresses: Option<Vec<String>>) -> Result<BalanceResult> {
        info!("Getting all balances for: {}", wallet_address);

        let eth_balance = self.get_eth_balance(wallet_address).await?;

        let mut token_balances = Vec::new();

        if let Some(tokens) = token_addresses {
            for token_addr in tokens {
                match self.get_token_balance(&token_addr, wallet_address).await {
                    Ok(balance) => token_balances.push(balance),
                    Err(e) => {
                        warn!("Failed to get balance for token {}: {}", token_addr, e);
                    }
                }
            }
        }

        let result = BalanceResult {
            address: wallet_address.to_string(),
            eth_balance,
            token_balances,
        };

        info!("Balance query completed for {}", wallet_address);
        Ok(result)
    }

    async fn get_token_price(&self, token_identifier: &str) -> Result<PriceResult> {
        info!("Getting price for token: {}", token_identifier);

        let mut url = format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd,eth&include_market_cap=true&include_last_updated_at=true",
            token_identifier
        );

        if let Some(api_key) = &self.coingecko_api_key {
            url = format!("{}&x_cg_demo_api_key={}", url, api_key);
        }

        debug!("Making CoinGecko API request: {}", url);

        let response: CoinGeckoPriceResponse = self.http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch price from CoinGecko: {}", e))?
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse CoinGecko response: {}", e))?;

        let token_data = response.data.get(token_identifier)
            .ok_or_else(|| anyhow!("Token not found: {}", token_identifier))?;

        let last_updated = if let Some(timestamp) = token_data.last_updated_at {
            chrono::DateTime::from_timestamp(timestamp as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        };

        let result = PriceResult {
            token: token_identifier.to_string(),
            usd_price: token_data.usd,
            eth_price: token_data.eth,
            market_cap: token_data.market_cap,
            last_updated,
        };

        info!("Price query completed for {}", token_identifier);
        Ok(result)
    }

    async fn simulate_swap(
        &self,
        from_token: &str,
        to_token: &str,
        amount: &str,
        slippage_tolerance: f64,
    ) -> Result<SwapSimulationResult> {
        info!("Simulating swap: {} -> {} (amount: {}, slippage: {}%)",
              from_token, to_token, amount, slippage_tolerance);

        let from_token_addr = from_token.parse::<Address>()
            .map_err(|e| anyhow!("Invalid from_token address: {}", e))?;

        let to_token_addr = to_token.parse::<Address>()
            .map_err(|e| anyhow!("Invalid to_token address: {}", e))?;

        let amount_in = parse_token_amount(amount, 18)?;

        // 创建合约实例
        let router = IUniswapV2Router::new(self.uniswap_router, self.provider.clone());
        let from_token_contract = IERC20::new(from_token_addr, self.provider.clone());
        let to_token_contract = IERC20::new(to_token_addr, self.provider.clone());

        // 获取代币信息
        let from_decimals = from_token_contract.decimals().call().await
            .map_err(|e| anyhow!("Failed to get from_token decimals: {}", e))?;

        let to_decimals = to_token_contract.decimals().call().await
            .map_err(|e| anyhow!("Failed to get to_token decimals: {}", e))?;

        let from_symbol = from_token_contract.symbol().call().await
            .unwrap_or_else(|_| "UNKNOWN".to_string());

        let to_symbol = to_token_contract.symbol().call().await
            .unwrap_or_else(|_| "UNKNOWN".to_string());

        // 构建交易路径
        let path = vec![from_token_addr, to_token_addr];

        debug!("Calling getAmountsOut with amount_in: {}, path: {:?}", amount_in, path);

        // 模拟获取输出数量 - 这里使用 eth_call 进行模拟
        let amounts = router.get_amounts_out(amount_in, path.clone()).call().await
            .map_err(|e| anyhow!("Failed to simulate swap: {}", e))?;

        if amounts.len() < 2 {
            return Err(anyhow!("Invalid amounts returned from router"));
        }

        let expected_amount_out = amounts[1];
        let min_amount_out = expected_amount_out * U256::from(10000 - (slippage_tolerance * 100.0) as u64) / U256::from(10000);

        // 估算 Gas
        let gas_estimate = 200000u64; // 保守估计
        let gas_price = self.provider.get_gas_price().await
            .map_err(|e| anyhow!("Failed to get gas price: {}", e))?;

        let gas_cost = gas_estimate * gas_price;
        let gas_cost_eth = ethers::utils::format_ether(gas_cost);

        // 计算价格影响（简化版）
        let price_impact = calculate_price_impact(&amounts, from_decimals, to_decimals);

        let result = SwapSimulationResult {
            from_token: format!("{} ({})", from_symbol, from_token),
            to_token: format!("{} ({})", to_symbol, to_token),
            amount_in: format_token_amount(amount_in, from_decimals),
            expected_amount_out: format_token_amount(expected_amount_out, to_decimals),
            min_amount_out: format_token_amount(min_amount_out, to_decimals),
            price_impact,
            gas_estimate,
            gas_cost_eth,
            gas_cost_usd: None, // 需要ETH价格来计算
            route: vec![from_symbol, to_symbol],
            success: true,
        };

        info!("Swap simulation completed successfully");
        debug!("Swap result: {:?}", result);

        Ok(result)
    }
}

// 工具函数
fn format_token_amount(amount: U256, decimals: u8) -> String {
    let divisor = U256::from(10).pow(U256::from(decimals));
    let whole = amount / divisor;
    let fractional = amount % divisor;

    if fractional.is_zero() {
        format!("{}", whole)
    } else {
        // 格式化小数部分，去除尾随零
        let fractional_str = fractional.to_string();
        let padded = format!("{:0>width$}", fractional_str, width = decimals as usize);
        let trimmed = padded.trim_end_matches('0');

        if trimmed.is_empty() {
            format!("{}", whole)
        } else {
            format!("{}.{}", whole, trimmed)
        }
    }
}

fn parse_token_amount(amount: &str, decimals: u8) -> Result<U256> {
    let parts: Vec<&str> = amount.split('.').collect();
    let whole = parts[0].parse::<U256>()
        .map_err(|e| anyhow!("Invalid amount format: {}", e))?;

    let fractional = if parts.len() > 1 { parts[1] } else { "0" };

    let multiplier = U256::from(10).pow(U256::from(decimals));
    let whole_amount = whole * multiplier;

    // 处理小数部分
    let fractional_len = fractional.len() as u32;
    if fractional_len > decimals as u32 {
        return Err(anyhow!("Too many decimal places: {} > {}", fractional_len, decimals));
    }

    let fractional_amount = if !fractional.is_empty() {
        let fractional = fractional.parse::<U256>()
            .map_err(|e| anyhow!("Invalid fractional part: {}", e))?;

        let fractional_multiplier = U256::from(10).pow(U256::from(decimals as u32 - fractional_len));
        fractional * fractional_multiplier
    } else {
        U256::zero()
    };

    Ok(whole_amount + fractional_amount)
}

fn calculate_price_impact(amounts: &[U256], from_decimals: u8, to_decimals: u8) -> f64 {
    // 简化的价格影响计算
    // 在实际实现中，这需要从池子 reserves 计算
    0.5
}

// MCP 服务器实现
struct EthereumMCPServer {
    ethereum_service: EthereumService,
}

impl EthereumMCPServer {
    fn new(ethereum_service: EthereumService) -> Self {
        EthereumMCPServer { ethereum_service }
    }

    fn get_tools(&self) -> Vec<Tool> {
        vec![
            Tool {
                name: "get_balance".to_string(),
                description: "Get Ethereum ETH and ERC20 token balances for an address".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Ethereum address to check balance for"
                        },
                        "token_addresses": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "Optional list of ERC20 token addresses to check"
                        }
                    },
                    "required": ["address"]
                }),
            },
            Tool {
                name: "get_token_price".to_string(),
                description: "Get current token price in USD and ETH".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "token_identifier": {
                            "type": "string",
                            "description": "Token identifier (e.g., 'ethereum', 'uniswap', or contract address)"
                        }
                    },
                    "required": ["token_identifier"]
                }),
            },
            Tool {
                name: "swap_tokens".to_string(),
                description: "Simulate token swap on Uniswap V2".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "from_token": {
                            "type": "string",
                            "description": "Token address to swap from"
                        },
                        "to_token": {
                            "type": "string",
                            "description": "Token address to swap to"
                        },
                        "amount": {
                            "type": "string",
                            "description": "Amount to swap"
                        },
                        "slippage_tolerance": {
                            "type": "number",
                            "description": "Slippage tolerance percentage (e.g., 0.5 for 0.5%)",
                            "default": 0.5
                        }
                    },
                    "required": ["from_token", "to_token", "amount"]
                }),
            },
        ]
    }

    async fn handle_call_tool(&self, request: CallToolRequest) -> Result<Vec<Content>> {
        let name = request.name;
        let arguments = request.arguments.unwrap_or_default();

        info!("Handling tool call: {}", name);
        debug!("Tool arguments: {:?}", arguments);

        match name.as_str() {
            "get_balance" => {
                let address = arguments.get("address")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing required parameter: address"))?;

                let token_addresses = arguments.get("token_addresses")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    });

                let balance_result = self.ethereum_service.get_all_balances(address, token_addresses).await?;
                let text = serde_json::to_string_pretty(&balance_result)?;

                Ok(vec![Content::Text(TextContent {
                    r#type: "text".to_string(),
                    text,
                })])
            }
            "get_token_price" => {
                let token_identifier = arguments.get("token_identifier")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing required parameter: token_identifier"))?;

                let price_result = self.ethereum_service.get_token_price(token_identifier).await?;
                let text = serde_json::to_string_pretty(&price_result)?;

                Ok(vec![Content::Text(TextContent {
                    r#type: "text".to_string(),
                    text,
                })])
            }
            "swap_tokens" => {
                let from_token = arguments.get("from_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing required parameter: from_token"))?;

                let to_token = arguments.get("to_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing required parameter: to_token"))?;

                let amount = arguments.get("amount")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing required parameter: amount"))?;

                let slippage_tolerance = arguments.get("slippage_tolerance")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.5);

                let swap_result = self.ethereum_service.simulate_swap(from_token, to_token, amount, slippage_tolerance).await?;
                let text = serde_json::to_string_pretty(&swap_result)?;

                Ok(vec![Content::Text(TextContent {
                    r#type: "text".to_string(),
                    text,
                })])
            }
            _ => Err(anyhow!("Unknown tool: {}", name)),
        }
    }
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

    // 创建 MCP 服务器
    let mut server = McpServer::new(StdioServerTransport);

    // 设置工具列表
    server.set_tools(mcp_server.get_tools());

    // 设置工具调用处理器
    server.set_tool_call_handler(|request| {
        let server = mcp_server.clone();
        async move {
            server.handle_call_tool(request).await
        }
    });

    info!("Ethereum MCP Server started successfully");
    info!("Ready to handle requests via stdio");

    // 启动服务器
    server.serve().await
        .map_err(|e| anyhow!("MCP server error: {}", e))?;

    Ok(())
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

// 为 EthereumMCPServer 实现 Clone
impl Clone for EthereumMCPServer {
    fn clone(&self) -> Self {


        EthereumMCPServer {
            ethereum_service: EthereumService {
                provider: self.ethereum_service.provider.clone(),
                uniswap_router: self.ethereum_service.uniswap_router,
                coingecko_api_key: self.ethereum_service.coingecko_api_key.clone(),
                http_client: self.ethereum_service.http_client.clone(),
            },
        }
    }
}
