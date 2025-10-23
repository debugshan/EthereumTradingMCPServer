use thiserror::Error;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("Ethereum error: {0}")]
    Ethereum(#[from] ethers::providers::ProviderError),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    #[error("Token not found: {0}")]
    TokenNotFound(String),

    #[error("Swap simulation failed: {0}")]
    SwapSimulation(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<McpError> for rmcp::types::McpError {
    fn from(error: McpError) -> Self {
        rmcp::types::McpError {
            code: -32000,
            message: error.to_string(),
        }
    }
}
