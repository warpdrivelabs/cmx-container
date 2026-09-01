//! 服务间调用统一错误类型。
//!
//! 全链路（目录定位 / 传输执行 / 信封解包 / SDK 契约）共用一个错误枚举，
//! 消费方按变体分类处理（熔断计数、重试判定、用户提示）。

use thiserror::Error;

/// 服务间调用错误。
#[derive(Debug, Error)]
pub enum ServiceRpcError {
    /// 目标服务不可用（网络不可达 / 无可用实例 / 熔断开放中 / 配置缺失）。
    #[error("服务 {key} 不可用: {cause}")]
    Unavailable {
        /// 服务键（`[service_rpc.services]` 中的键）。
        key: String,
        /// 不可用原因（人读）。
        cause: String,
    },

    /// 调用超时（总超时 = 键级 `timeout_ms` ?? 全局缺省）。
    #[error("服务 {key} 调用超时（{timeout_ms}ms）")]
    Timeout {
        /// 服务键。
        key: String,
        /// 生效的超时毫秒数。
        timeout_ms: u64,
    },

    /// 目标服务拒绝鉴权（HTTP 401/403 或 gRPC Unauthenticated/PermissionDenied）。
    #[error("服务 {key} 鉴权被拒: {cause}")]
    AuthRejected {
        /// 服务键。
        key: String,
        /// 拒绝原因。
        cause: String,
    },

    /// 目标服务返回业务失败（HTTP 2xx 但信封 code != 0，或非 2xx 响应）。
    #[error("服务 {key} 返回失败（HTTP {http_status}, code {code}）: {msg}")]
    Remote {
        /// 服务键。
        key: String,
        /// HTTP 状态码（gRPC 传输时为 0）。
        http_status: u16,
        /// 业务错误码（信封 `code`，缺失时 -1）。
        code: i64,
        /// 业务错误信息（信封 `msg`/`message`，缺失时通用文案）。
        msg: String,
    },

    /// 响应解析失败（非 JSON / 信封缺字段 / data 反序列化失败）。
    #[error("服务 {key} 响应解析失败: {cause}")]
    Decode {
        /// 服务键。
        key: String,
        /// 解析失败原因。
        cause: String,
    },

    /// 服务键未绑定所指传输：SDK 无该传输绑定，或进程未编译对应 feature。
    #[error("服务键 {key} 未绑定 {transport} 传输: {cause}")]
    NoBinding {
        /// 服务键。
        key: String,
        /// 所指传输（"grpc" 等）。
        transport: String,
        /// 说明与操作提示（如需开启的 feature 名）。
        cause: String,
    },
}

impl ServiceRpcError {
    /// 错误所属的服务键（日志 / 熔断 / 打点归类用）。
    pub fn key(&self) -> &str {
        match self {
            Self::Unavailable { key, .. }
            | Self::Timeout { key, .. }
            | Self::AuthRejected { key, .. }
            | Self::Remote { key, .. }
            | Self::Decode { key, .. }
            | Self::NoBinding { key, .. } => key,
        }
    }

    /// 是否传输级失败（熔断计数口径：Unavailable / Timeout；业务级 Remote/Auth 不计）。
    pub fn is_transport_failure(&self) -> bool {
        matches!(self, Self::Unavailable { .. } | Self::Timeout { .. })
    }
}
