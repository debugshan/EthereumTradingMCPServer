use rmcp::types::{Tool, Content, TextContent};
use serde_json::json;

pub fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "get_balance".to_string(),
            description: "Get Ethereum ETH and ERC20 token balances for an address".to_string(),
            input_schema: json!({
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
            input_schema: json!({
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
            input_schema: json!({
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

pub fn create_text_content(text: String) -> Vec<Content> {
    vec![Content::Text(TextContent {
        r#type: "text".to_string(),
        text,
    })]
}
