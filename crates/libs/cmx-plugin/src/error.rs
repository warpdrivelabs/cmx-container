//! 插件错误类型模块
//!
//! 定义插件系统在生命周期管理过程中可能遇到的各种错误类型，包括但不限于：
//! - IO 错误：文件读写、网络通信等
//! - 插件错误：插件未找到、已存在、状态错误等
//! - 签名验证错误：签名不匹配、证书无效等
//! - 生命周期错误：安装、卸载、激活、停用、升级、降级等操作失败
//! - 依赖错误：缺少依赖、依赖冲突等
//! - 版本错误：版本格式错误、版本不兼容等
//! - 部署错误：部署失败、节点不可用等
//! - 权限错误：权限不足、权限被拒绝等
//! - 资源错误：资源不足、超时等
//!
//! 所有错误类型都实现了 `thiserror::Error` trait，可以方便地转换为字符串错误消息。

use thiserror::Error;

/// 插件系统错误枚举
///
/// 涵盖插件从安装到卸载整个生命周期内可能出现的各类错误。
///
#[derive(Error, Debug)]
pub enum PluginError {
    // ==================== 基础错误 ====================

    /// IO 错误：文件读写、网络通信等
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 解析错误：序列化或反序列化失败
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),

    /// YAML 解析错误
    #[error("YAML 解析错误: {0}")]
    Yaml(String),

    /// TOML 解析错误
    #[error("TOML 解析错误: {0}")]
    Toml(String),

    /// ZIP 压缩/解压错误
    #[error("ZIP 错误: {0}")]
    Zip(String),

    // ==================== 插件核心错误 ====================

    /// 插件通用错误
    #[error("插件错误: {0}")]
    Plugin(String),

    /// 未找到错误：插件、节点、服务等不存在
    #[error("未找到: {0}")]
    NotFound(String),

    /// 冲突错误：插件已存在、资源冲突等
    #[error("冲突: {0}")]
    Conflict(String),

    /// 状态错误：插件状态不正确
    #[error("状态错误: 插件 '{plugin_id}' 当前状态为 '{current}'，无法执行 '{operation}' 操作")]
    InvalidState {
        plugin_id: String,
        current: String,
        operation: String,
    },

    // ==================== 生命周期操作错误 ====================

    /// 插件获取错误（从本地/远程/注册表获取插件包时）
    #[error("获取错误: {0}")]
    Fetcher(String),

    /// 插件安装错误
    #[error("安装错误: {0}")]
    Install(String),

    /// 插件卸载错误
    #[error("卸载错误: {0}")]
    Uninstall(String),

    /// 插件激活错误
    #[error("激活错误: {0}")]
    Activate(String),

    /// 插件停用错误
    #[error("停用错误: {0}")]
    Deactivate(String),

    /// 插件升级错误
    #[error("升级错误: {0}")]
    Upgrade(String),

    /// 插件降级错误
    #[error("降级错误: {0}")]
    Downgrade(String),

    /// 插件回滚错误
    #[error("回滚错误: {0}")]
    Rollback(String),

    /// 插件部署错误（智能安装/升级）
    #[error("部署错误: {0}")]
    Deploy(String),

    // ==================== 依赖和版本错误 ====================

    /// 依赖错误：缺少依赖、依赖冲突等
    #[error("依赖错误: {0}")]
    Dependency(String),

    /// 缺少依赖
    #[error("缺少依赖: 插件 '{plugin_id}' 需要依赖 '{dependency}'")]
    MissingDependency {
        plugin_id: String,
        dependency: String,
    },

    /// 依赖冲突
    #[error("依赖冲突: 插件 '{plugin_id}' 与 '{conflicting_plugin}' 存在依赖冲突")]
    DependencyConflict {
        plugin_id: String,
        conflicting_plugin: String,
        details: String,
    },

    /// 版本错误：版本格式错误、版本不兼容等
    #[error("版本错误: {0}")]
    Version(String),

    /// 版本不兼容
    #[error("版本不兼容: 插件 '{plugin_id}' 版本 {installed} 与要求版本 {required} 不兼容")]
    VersionIncompatible {
        plugin_id: String,
        installed: String,
        required: String,
    },

    // ==================== 部署和集群错误 ====================

    /// 部署错误：多节点部署失败等
    #[error("部署错误: {0}")]
    Deployment(String),

    /// 节点不可用
    #[error("节点不可用: {node_id}")]
    NodeUnavailable { node_id: String },

    /// 节点错误
    #[error("节点错误: {0}")]
    Node(String),

    // ==================== 安全和权限错误 ====================

    /// 权限错误：权限检查失败、权限不足等
    #[error("权限错误: {0}")]
    Permission(String),

    /// 权限被拒绝
    #[error("权限被拒绝: 插件 '{plugin_id}' 缺少权限 '{permission}'")]
    PermissionDenied {
        plugin_id: String,
        permission: String,
    },

    /// 签名验证失败
    #[error("签名验证失败: {0}")]
    SignatureVerification(String),

    /// 安全错误：安全验证失败、沙箱逃逸等
    #[error("安全错误: {0}")]
    Security(String),

    // ==================== 资源错误 ====================

    /// 资源不足错误：内存、磁盘空间等
    #[error("资源不足: {0}")]
    InsufficientResource(String),

    /// 超时错误：操作超时
    #[error("超时错误: {0}")]
    Timeout(String),

    // ==================== 数据存储错误 ====================

    /// 数据库错误：数据库操作失败、连接错误等
    #[error("数据库错误: {0}")]
    Database(String),

    /// 缓存错误
    #[error("缓存错误: {0}")]
    Cache(#[from] cmx_buffer::Error),

    /// 存储错误
    #[error("存储错误: {0}")]
    Storage(String),

    // ==================== 配置和元数据错误 ====================

    /// 元数据错误：插件元数据格式错误、缺少必需字段等
    #[error("元数据错误: {0}")]
    Metadata(String),

    /// 配置错误：配置文件缺失、格式错误等
    #[error("配置错误: {0}")]
    Config(String),

    /// 初始化错误：系统初始化失败等
    #[error("初始化错误: {0}")]
    Init(String),

    // ==================== 网络错误 ====================

    /// 网络错误：网络连接失败、超时等
    #[error("网络错误: {0}")]
    Network(String),

    // ==================== 事务错误 ====================

    /// 事务错误：事务失败、回滚失败等
    #[error("事务错误: {0}")]
    Transaction(String),

    // ==================== 运行时错误 ====================

    /// WASM 运行时错误
    #[error("WASM 运行时错误: {0}")]
    WasmRuntime(String),

    /// 服务调用错误
    #[error("服务调用错误: {0}")]
    ServiceCall(String),

    /// 功能错误
    #[error("功能错误: {0}")]
    Feature(String),

    // ==================== 服务中心错误 ====================

    /// 服务中心数据分发错误
    #[error("服务中心错误: {0}")]
    CenterData(String),
}

// ==================== From 实现 ====================

impl From<serde_yaml::Error> for PluginError {
    fn from(err: serde_yaml::Error) -> Self {
        PluginError::Yaml(err.to_string())
    }
}

impl From<toml::de::Error> for PluginError {
    fn from(err: toml::de::Error) -> Self {
        PluginError::Toml(err.to_string())
    }
}

impl From<toml::ser::Error> for PluginError {
    fn from(err: toml::ser::Error) -> Self {
        PluginError::Toml(err.to_string())
    }
}

// ==================== 辅助方法 ====================

impl PluginError {
    /// 创建插件未找到错误
    pub fn plugin_not_found(plugin_id: &str) -> Self {
        PluginError::NotFound(format!("插件 '{}' 未找到", plugin_id))
    }

    /// 创建插件已存在错误
    pub fn plugin_already_exists(plugin_id: &str) -> Self {
        PluginError::Conflict(format!("插件 '{}' 已存在", plugin_id))
    }

    /// 创建无效状态错误
    pub fn invalid_state(plugin_id: &str, current: &str, operation: &str) -> Self {
        PluginError::InvalidState {
            plugin_id: plugin_id.to_string(),
            current: current.to_string(),
            operation: operation.to_string(),
        }
    }

    /// 创建缺少依赖错误
    pub fn missing_dependency(plugin_id: &str, dependency: &str) -> Self {
        PluginError::MissingDependency {
            plugin_id: plugin_id.to_string(),
            dependency: dependency.to_string(),
        }
    }

    /// 创建依赖冲突错误
    pub fn dependency_conflict(plugin_id: &str, conflicting_plugin: &str, details: &str) -> Self {
        PluginError::DependencyConflict {
            plugin_id: plugin_id.to_string(),
            conflicting_plugin: conflicting_plugin.to_string(),
            details: details.to_string(),
        }
    }

    /// 创建版本不兼容错误
    pub fn version_incompatible(plugin_id: &str, installed: &str, required: &str) -> Self {
        PluginError::VersionIncompatible {
            plugin_id: plugin_id.to_string(),
            installed: installed.to_string(),
            required: required.to_string(),
        }
    }

    /// 创建权限被拒绝错误
    pub fn permission_denied(plugin_id: &str, permission: &str) -> Self {
        PluginError::PermissionDenied {
            plugin_id: plugin_id.to_string(),
            permission: permission.to_string(),
        }
    }

    /// 创建节点不可用错误
    pub fn node_unavailable(node_id: &str) -> Self {
        PluginError::NodeUnavailable {
            node_id: node_id.to_string(),
        }
    }

    /// 检查是否为可重试错误
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            PluginError::Timeout(_)
                | PluginError::Network(_)
                | PluginError::NodeUnavailable { .. }
                | PluginError::Io(_)
        )
    }

    /// 检查是否为致命错误
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            PluginError::Security(_)
                | PluginError::SignatureVerification(_)
                | PluginError::PermissionDenied { .. }
                | PluginError::InsufficientResource(_)
        )
    }

    /// 获取错误代码
    pub fn error_code(&self) -> &'static str {
        match self {
            PluginError::Io(_) => "IO_ERROR",
            PluginError::Json(_) => "JSON_ERROR",
            PluginError::Yaml(_) => "YAML_ERROR",
            PluginError::Toml(_) => "TOML_ERROR",
            PluginError::Zip(_) => "ZIP_ERROR",
            PluginError::Plugin(_) => "PLUGIN_ERROR",
            PluginError::NotFound(_) => "NOT_FOUND",
            PluginError::Conflict(_) => "CONFLICT",
            PluginError::InvalidState { .. } => "INVALID_STATE",
            PluginError::Install(_) => "INSTALL_ERROR",
            PluginError::Uninstall(_) => "UNINSTALL_ERROR",
            PluginError::Activate(_) => "ACTIVATE_ERROR",
            PluginError::Deactivate(_) => "DEACTIVATE_ERROR",
            PluginError::Upgrade(_) => "UPGRADE_ERROR",
            PluginError::Downgrade(_) => "DOWNGRADE_ERROR",
            PluginError::Rollback(_) => "ROLLBACK_ERROR",
            PluginError::Deploy(_) => "DEPLOY_ERROR",
            PluginError::Fetcher(_) => "FETCHER_ERROR",
            PluginError::Dependency(_) => "DEPENDENCY_ERROR",
            PluginError::MissingDependency { .. } => "MISSING_DEPENDENCY",
            PluginError::DependencyConflict { .. } => "DEPENDENCY_CONFLICT",
            PluginError::Version(_) => "VERSION_ERROR",
            PluginError::VersionIncompatible { .. } => "VERSION_INCOMPATIBLE",
            PluginError::Deployment(_) => "DEPLOYMENT_ERROR",
            PluginError::NodeUnavailable { .. } => "NODE_UNAVAILABLE",
            PluginError::Node(_) => "NODE_ERROR",
            PluginError::Permission(_) => "PERMISSION_ERROR",
            PluginError::PermissionDenied { .. } => "PERMISSION_DENIED",
            PluginError::SignatureVerification(_) => "SIGNATURE_VERIFICATION_ERROR",
            PluginError::Security(_) => "SECURITY_ERROR",
            PluginError::InsufficientResource(_) => "INSUFFICIENT_RESOURCE",
            PluginError::Timeout(_) => "TIMEOUT",
            PluginError::Database(_) => "DATABASE_ERROR",
            PluginError::Cache(_) => "CACHE_ERROR",
            PluginError::Storage(_) => "STORAGE_ERROR",
            PluginError::Metadata(_) => "METADATA_ERROR",
            PluginError::Config(_) => "CONFIG_ERROR",
            PluginError::Init(_) => "INIT_ERROR",
            PluginError::Network(_) => "NETWORK_ERROR",
            PluginError::Transaction(_) => "TRANSACTION_ERROR",
            PluginError::WasmRuntime(_) => "WASM_RUNTIME_ERROR",
            PluginError::ServiceCall(_) => "SERVICE_CALL_ERROR",
            PluginError::Feature(_) => "FEATURE_ERROR",
            PluginError::CenterData(_) => "CENTER_DATA_ERROR",
        }
    }
}

/// 插件操作结果类型别名
pub type PluginResult<T> = Result<T, PluginError>;
