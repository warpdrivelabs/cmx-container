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
#[derive(Error, Debug)]
pub enum PluginError {
    /// IO 错误：文件读写、网络通信等
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    /// JSON 解析错误：序列化或反序列化失败
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    /// ZIP 压缩/解压错误
    #[error("ZIP 错误: {0}")]
    Zip(String),
    /// 插件通用错误
    #[error("插件错误: {0}")]
    Plugin(String),
    /// 签名验证失败
    #[error("签名验证失败: {0}")]
    SignatureVerification(String),
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
    /// 依赖错误：缺少依赖、依赖冲突等
    #[error("依赖错误: {0}")]
    Dependency(String),
    /// 版本错误：版本格式错误、版本不兼容等
    #[error("版本错误: {0}")]
    Version(String),
    /// 部署错误：多节点部署失败等
    #[error("部署错误: {0}")]
    Deployment(String),
    /// 权限错误：权限检查失败、权限不足等
    #[error("权限错误: {0}")]
    Permission(String),
    /// 资源不足错误：内存、磁盘空间等
    #[error("资源不足: {0}")]
    InsufficientResource(String),
    /// 未找到错误：插件、节点、服务等不存在
    #[error("未找到: {0}")]
    NotFound(String),
    /// 冲突错误：插件已存在、资源冲突等
    #[error("冲突: {0}")]
    Conflict(String),
    /// 数据库错误：数据库操作失败、连接错误等
    #[error("数据库错误: {0}")]
    Database(String),
    /// 元数据错误：插件元数据格式错误、缺少必需字段等
    #[error("元数据错误: {0}")]
    Metadata(String),
    /// 配置错误：配置文件缺失、格式错误等
    #[error("配置错误: {0}")]
    Config(String),
    /// 初始化错误：系统初始化失败等
    #[error("初始化错误: {0}")]
    Init(String),
    /// 安全错误：安全验证失败、沙箱逃逸等
    #[error("安全错误: {0}")]
    Security(String),
    /// 网络错误：网络连接失败、超时等
    #[error("网络错误: {0}")]
    Network(String),
    /// 超时错误：操作超时
    #[error("超时错误: {0}")]
    Timeout(String),
    /// 事务错误：事务失败、回滚失败等
    #[error("事务错误: {0}")]
    Transaction(String),
}
