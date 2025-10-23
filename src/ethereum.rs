use anyhow::Result;
use ethers::{
    prelude::*,
    types::{Address, U256},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{info, debug, warn, error};

use crate::error::McpError;

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

#[derive(Debug, Serialize)]
pub struct BalanceResult {
    pub address: String,
    pub eth_balance: String,
    pub token_balances: Vec<TokenBalance>,
}

#[derive(Debug, Serialize)]
pub struct TokenBalance {
    pub token_address: String,
    pub name: String,
    pub symbol: String,
    pub balance: String,
    pub decimals: u8,
}

#[derive(Debug, Serialize)]
pub struct PriceResult {
    pub token: String,
    pub usd_price: Option<f64>,
    pub eth_price: Option<f64>,
    pub market_cap: Option<f64>,
    pub last_updated: String,
}

#[derive(Debug, Serialize)]
pub struct SwapSimulationResult {
    pub from_token: String,
    pub to_token: String,
    pub amount_in: String,
    pub expected_amount_out: String,
    pub min_amount_out: String,
    pub price_impact: f64,
    pub gas_estimate: u64,
    pub gas_cost_eth: String,
    pub gas_cost_usd: Option<String>,
    pub route: Vec<String>,
    pub success: bool,
}

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

pub struct EthereumService {
    provider: Provider<Http>,
    uniswap_router: Address,
    coingecko_api_key: Option<String>,
    http_client: reqwest::Client,
}

impl EthereumService {
    pub fn new(
        rpc_url: String,
        uniswap_router: String,
        coingecko_api_key: Option<String>,
    ) -> Result<Self, McpError> {
        let provider = Provider::<Http>::try_from(rpc_url)
            .map_err(|e| McpError::Config(format!("Failed to create Ethereum provider: {}", e)))?;

        let uniswap_router = uniswap_router.parse::<Address>()
            .map_err(|e| McpError::InvalidAddress(format!("Invalid Uniswap router address: {}", e)))?;

        let http_client = reqwest::Client::new();

        info!("Ethereum service initialized with router: {}", uniswap_router);

        Ok(EthereumService {
            provider,
            uniswap_router,
            coingecko_api_key,
            http_client,
        })
    }

    pub async fn get_eth_balance(&self, address: &str) -> Result<String, McpError> {
        let address = address.parse::<Address>()
            .map_err(|e| McpError::InvalidAddress(format!("Invalid Ethereum address: {}", e)))?;

        debug!("Querying ETH balance for address: {}", address);

        let balance = self.provider.get_balance(address, None).await
            .map_err(McpError::Ethereum)?;

        let balance_eth = ethers::utils::format_ether(balance);
        info!("ETH balance for {}: {}", address, balance_eth);

        Ok(balance_eth)
    }

    pub async fn get_token_balance(&self, token_address: &str, wallet_address: &str) -> Result<TokenBalance, McpError> {
        let token_addr = token_address.parse::<Address>()
            .map_err(|e| McpError::InvalidAddress(format!("Invalid token address: {}", e)))?;

        let wallet_addr = wallet_address.parse::<Address>()
            .map_err(|e| McpError::InvalidAddress(format!("Invalid wallet address: {}", e)))?;

        debug!("Querying token balance: token={}, wallet={}", token_addr, wallet_addr);

        let erc20 = IERC20::new(token_addr, self.provider.clone());

        let balance = erc20.balance_of(wallet_addr).call().await
            .map_err(|e| McpError::Ethereum(e))?;

        let decimals = erc20.decimals().call().await
            .map_err(|e| McpError::Ethereum(e))?;

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

    pub async fn get_all_balances(&self, wallet_address: &str, token_addresses: Option<Vec<String>>) -> Result<BalanceResult, McpError> {
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

    pub async fn get_token_price(&self, token_identifier: &str) -> Result<PriceResult, McpError> {
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
            .map_err(McpError::Http)?
            .json()
            .await
            .map_err(McpError::Http)?;

        let token_data = response.data.get(token_identifier)
            .ok_or_else(|| McpError::TokenNotFound(token_identifier.to_string()))?;

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

    pub async fn simulate_swap(
        &self,
        from_token: &str,
        to_token: &str,
        amount: &str,
        slippage_tolerance: f64,
    ) -> Result<SwapSimulationResult, McpError> {
        info!("Simulating swap: {} -> {} (amount: {}, slippage: {}%)",
              from_token, to_token, amount, slippage_tolerance);

        let from_token_addr = from_token.parse::<Address>()
            .map_err(|e| McpError::InvalidAddress(format!("Invalid from_token address: {}", e)))?;

        let to_token_addr = to_token.parse::<Address>()
            .map_err(|e| McpError::InvalidAddress(format!("Invalid to_token address: {}", e)))?;

        let amount_in = parse_token_amount(amount, 18)
            .map_err(|e| McpError::InvalidAmount(e.to_string()))?;

        // 创建合约实例
        let router = IUniswapV2Router::new(self.uniswap_router, self.provider.clone());
        let from_token_contract = IERC20::new(from_token_addr, self.provider.clone());
        let to_token_contract = IERC20::new(to_token_addr, self.provider.clone());

        // 获取代币信息
        let from_decimals = from_token_contract.decimals().call().await
            .map_err(|e| McpError::Ethereum(e))?;

        let to_decimals = to_token_contract.decimals().call().await
            .map_err(|e| McpError::Ethereum(e))?;

        let from_symbol = from_token_contract.symbol().call().await
            .unwrap_or_else(|_| "UNKNOWN".to_string());

        let to_symbol = to_token_contract.symbol().call().await
            .unwrap_or_else(|_| "UNKNOWN".to_string());

        // 构建交易路径
        let path = vec![from_token_addr, to_token_addr];

        debug!("Calling getAmountsOut with amount_in: {}, path: {:?}", amount_in, path);

        // 模拟获取输出数量 - 这里使用 eth_call 进行模拟
        let amounts = router.get_amounts_out(amount_in, path.clone()).call().await
            .map_err(|e| McpError::SwapSimulation(format!("Failed to simulate swap: {}", e)))?;

        if amounts.len() < 2 {
            return Err(McpError::SwapSimulation("Invalid amounts returned from router".to_string()));
        }

        let expected_amount_out = amounts[1];
        let min_amount_out = expected_amount_out * U256::from(10000 - (slippage_tolerance * 100.0) as u64) / U256::from(10000);

        // 估算 Gas
        let gas_estimate = 200000u64; // 保守估计
        let gas_price = self.provider.get_gas_price().await
            .map_err(McpError::Ethereum)?;

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

fn parse_token_amount(amount: &str, decimals: u8) -> anyhow::Result<U256> {
    let parts: Vec<&str> = amount.split('.').collect();
    let whole = parts[0].parse::<U256>()
        .map_err(|e| anyhow::anyhow!("Invalid amount format: {}", e))?;

    let fractional = if parts.len() > 1 { parts[1] } else { "0" };

    let multiplier = U256::from(10).pow(U256::from(decimals));
    let whole_amount = whole * multiplier;

    // 处理小数部分
    let fractional_len = fractional.len() as u32;
    if fractional_len > decimals as u32 {
        return Err(anyhow::anyhow!("Too many decimal places: {} > {}", fractional_len, decimals));
    }

    let fractional_amount = if !fractional.is_empty() {
        let fractional = fractional.parse::<U256>()
            .map_err(|e| anyhow::anyhow!("Invalid fractional part: {}", e))?;

        let fractional_multiplier = U256::from(10).pow(U256::from(decimals as u32 - fractional_len));
        fractional * fractional_multiplier
    } else {
        U256::zero()
    };

    Ok(whole_amount + fractional_amount)
}

fn calculate_price_impact(amounts: &[U256], _from_decimals: u8, _to_decimals: u8) -> f64 {
    // 简化的价格影响计算
    // 在实际实现中，这需要从池子 reserves 计算
    0.5
}
