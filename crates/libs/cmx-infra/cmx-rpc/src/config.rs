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
    /// RPC 调用超时时间（毫秒），通过 volo rpc_timeout 设置
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// 连接超时时间（毫秒），通过 volo connect_timeout 设置
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    /// 重试次数（仅对可重试错误重试：UNAVAILABLE/DEADLINE_EXCEEDED/RESOURCE_EXHAUSTED/ABORTED）
    #[serde(default)]
    pub retry_count: usize,
    /// 默认服务分组（用于 query_instances 过滤，None 表示不按分组过滤）
    #[serde(default)]
    pub default_group: Option<String>,
    /// 默认集群列表（用于 query_instances 过滤，空表示不过滤）
    #[serde(default)]
    pub default_clusters: Vec<String>,
    /// 服务发现变更通知通道容量
    ///
    /// 用于 RegistryAwareDiscover 内部 broadcast 通道的容量。
    /// 值越大越能缓冲高频服务变更（如 k8s 滚动更新），但内存占用略增。
    #[serde(default = "default_discover_channel_capacity")]
    pub discover_channel_capacity: usize,
}

fn default_timeout_ms() -> u64 {
    5000
}

fn default_connect_timeout_ms() -> u64 {
    3000
}

fn default_service_sync_interval_secs() -> u64 {
    30
}

fn default_discover_channel_capacity() -> usize {
    1024
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
