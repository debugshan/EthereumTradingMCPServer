#[cfg(test)]
mod tests {
    use super::*;
    use crate::ethereum::EthereumService;

    #[tokio::test]
    async fn test_ethereum_service_creation() {
        // 这个测试需要实际的 RPC URL，所以在 CI 中可能会跳过
        if std::env::var("TEST_RPC_URL").is_ok() {
            let rpc_url = std::env::var("TEST_RPC_URL").unwrap();
            let service = EthereumService::new(rpc_url, "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_string(), None);
            assert!(service.is_ok());
        }
    }

    #[test]
    fn test_tools_definition() {
        let tools = crate::tools::get_tools();
        assert_eq!(tools.len(), 3);

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"get_balance".to_string()));
        assert!(tool_names.contains(&"get_token_price".to_string()));
        assert!(tool_names.contains(&"swap_tokens".to_string()));
    }

    #[test]
    fn test_format_token_amount() {
        use ethers::types::U256;

        // 测试整数金额
        let amount = U256::from(1000000000000000000u64); // 1 ETH
        let formatted = crate::ethereum::format_token_amount(amount, 18);
        assert_eq!(formatted, "1");

        // 测试小数金额
        let amount = U256::from(100000000000000000u64); // 0.1 ETH
        let formatted = crate::ethereum::format_token_amount(amount, 18);
        assert_eq!(formatted, "0.1");
    }
}
