//! RPC 配置定义

use serde::Deserialize;

/// RPC 总配置
#[derive(Debug, Clone, Deserialize)]
pub struct RpcConfig {
    /// 是否启用 RPC
    pub enabled: bool,
    /// 通信协议（目前仅支持 "grpc"）
    pub protocol: String,
    /// gRPC 配置
    pub grpc: GrpcConfig,
    /// HTTP REST 相关配置
    #[serde(default)]
    pub http_rest: HttpRestConfig,
    /// 预热服务列表（启动时预先发现的服务名）
    #[serde(default)]
    pub warmup_services: Vec<String>,
    /// 服务列表同步间隔（秒），0 表示禁用定时同步
    #[serde(default = "default_service_sync_interval_secs")]
    pub service_sync_interval_secs: u64,
}

/// gRPC 配置
#[derive(Debug, Clone, Deserialize)]
pub struct GrpcConfig {
    /// 监听端口
    pub port: u16,
    /// 调用超时时间（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// 重试次数
    #[serde(default)]
    pub retry_count: usize,
    /// 连接池大小
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
}

fn default_timeout_ms() -> u64 {
    5000
}

fn default_pool_size() -> usize {
    4
}

fn default_service_sync_interval_secs() -> u64 {
    30
}

/// HTTP REST 相关配置（预留，本期不实现）
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HttpRestConfig {
    /// HTTP REST 服务端口
    #[serde(default = "default_http_port")]
    pub port: u16,
    /// 调用超时（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_http_port() -> u16 {
    8080
}
