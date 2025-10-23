use anyhow::Result;
use rmcp::{
    McpServer,
    transport::stdio::StdioServerTransport,
    types::{CallToolRequest, ListToolsRequest, ListToolsResult},
};
use tracing::{info, debug, error};

use crate::ethereum::{EthereumService, BalanceResult, PriceResult, SwapSimulationResult};
use crate::tools::{get_tools, create_text_content};
use crate::error::McpError;

pub struct EthereumMCPServer {
    ethereum_service: EthereumService,
}

impl EthereumMCPServer {
    pub fn new(ethereum_service: EthereumService) -> Self {
        EthereumMCPServer { ethereum_service }
    }

    pub async fn handle_call_tool(&self, request: CallToolRequest) -> Result<Vec<rmcp::types::Content>, McpError> {
        let name = request.name;
        let arguments = request.arguments.unwrap_or_default();

        info!("Handling tool call: {}", name);
        debug!("Tool arguments: {:?}", arguments);

        match name.as_str() {
            "get_balance" => {
                let address = arguments.get("address")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::Config("Missing required parameter: address".to_string()))?;

                let token_addresses = arguments.get("token_addresses")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    });

                let balance_result = self.ethereum_service.get_all_balances(address, token_addresses).await?;
                let text = serde_json::to_string_pretty(&balance_result)
                    .map_err(McpError::Serialization)?;

                Ok(create_text_content(text))
            }
            "get_token_price" => {
                let token_identifier = arguments.get("token_identifier")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::Config("Missing required parameter: token_identifier".to_string()))?;

                let price_result = self.ethereum_service.get_token_price(token_identifier).await?;
                let text = serde_json::to_string_pretty(&price_result)
                    .map_err(McpError::Serialization)?;

                Ok(create_text_content(text))
            }
            "swap_tokens" => {
                let from_token = arguments.get("from_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::Config("Missing required parameter: from_token".to_string()))?;

                let to_token = arguments.get("to_token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::Config("Missing required parameter: to_token".to_string()))?;

                let amount = arguments.get("amount")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::Config("Missing required parameter: amount".to_string()))?;

                let slippage_tolerance = arguments.get("slippage_tolerance")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.5);

                let swap_result = self.ethereum_service.simulate_swap(from_token, to_token, amount, slippage_tolerance).await?;
                let text = serde_json::to_string_pretty(&swap_result)
                    .map_err(McpError::Serialization)?;

                Ok(create_text_content(text))
            }
            _ => Err(McpError::Config(format!("Unknown tool: {}", name))),
        }
    }

    pub async fn run_server(self) -> Result<()> {
        let mut server = McpServer::new(StdioServerTransport);

        // 设置工具列表
        server.set_tools(get_tools());

        // 设置工具调用处理器
        let server_clone = self.clone();
        server.set_tool_call_handler(move |request| {
            let server = server_clone.clone();
            async move {
                server.handle_call_tool(request).await
                    .map_err(|e| e.into())
            }
        });

        info!("Starting MCP server...");
        server.serve().await
            .map_err(|e| anyhow::anyhow!("MCP server error: {}", e))?;

        Ok(())
    }
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
