# MCP Ethereum Server

一个基于 Rust 的模型上下文协议 (MCP) 服务器，使 AI 代理能够查询以太坊余额并执行代币兑换模拟。

## 功能特性

- **余额查询**: 查询 ETH 和 ERC20 代币余额
- **价格查询**: 通过 CoinGecko API 获取代币价格
- **交易模拟**: 在 Uniswap V2 上模拟代币兑换（仅模拟，不实际执行）
- **MCP 协议兼容**: 遵循模型上下文协议标准
- **结构化日志**: 使用 `tracing` 框架进行可配置的日志记录

## 快速开始

### 前提条件

- Rust 1.70+ 和 Cargo
- 以太坊 RPC 端点（Infura、Alchemy 或本地节点）
- （可选）CoinGecko API 密钥用于价格查询

### 安装

```bash
git clone https://github.com/your-username/mcp-ethereum-server
cd mcp-ethereum-server
cargo build --release


### 配置
# 使用环境变量
export ETH_RPC_URL="https://mainnet.infura.io/v3/YOUR_PROJECT_ID"
export COINGECKO_API_KEY="YOUR_API_KEY"

# 或使用命令行参数
cargo run -- \
  --rpc-url "https://mainnet.infura.io/v3/YOUR_PROJECT_ID" \
  --coingecko-api-key "YOUR_API_KEY" \
  --log-level info

### 运行
### # 开发模式
cargo run

# 发布模式
cargo build --release
./target/release/mcp-ethereum-server
