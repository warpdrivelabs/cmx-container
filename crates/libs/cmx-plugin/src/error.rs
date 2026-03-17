//! 插件错误类型

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP 错误: {0}")]
    Zip(String),
    #[error("插件错误: {0}")]
    Plugin(String),
    #[error("签名验证失败: {0}")]
    SignatureVerification(String),
    #[error("安装错误: {0}")]
    Install(String),
    #[error("卸载错误: {0}")]
    Uninstall(String),
    #[error("激活错误: {0}")]
    Activate(String),
    #[error("停用错误: {0}")]
    Deactivate(String),
    #[error("升级错误: {0}")]
    Upgrade(String),
    #[error("降级错误: {0}")]
    Downgrade(String),
    #[error("回滚错误: {0}")]
    Rollback(String),
    #[error("依赖错误: {0}")]
    Dependency(String),
    #[error("版本错误: {0}")]
    Version(String),
    #[error("部署错误: {0}")]
    Deployment(String),
    #[error("权限错误: {0}")]
    Permission(String),
    #[error("资源不足: {0}")]
    InsufficientResource(String),
    #[error("未找到: {0}")]
    NotFound(String),
    #[error("冲突: {0}")]
    Conflict(String),
    #[error("数据库错误: {0}")]
    Database(String),
    #[error("元数据错误: {0}")]
    Metadata(String),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("初始化错误: {0}")]
    Init(String),
    #[error("安全错误: {0}")]
    Security(String),
    #[error("网络错误: {0}")]
    Network(String),
    #[error("超时错误: {0}")]
    Timeout(String),
    #[error("事务错误: {0}")]
    Transaction(String),
}
