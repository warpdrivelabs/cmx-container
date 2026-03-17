# cmx-plugin 插件生命周期管理

cmx-plugin 是 CMX 框架的核心插件管理模块，提供完整的插件生命周期管理功能，包括插件的安装、卸载、激活、停用、升级、降级、回滚等操作。

## 功能特性

- **插件生命周期管理** - 完整的安装、卸载、激活、停用、升级、降级、回滚支持
- **多源获取** - 支持从 Zip 文件、URL、注册表、本地目录获取插件
- **语义版本** - 完整的语义版本解析和依赖约束处理
- **依赖解析** - 自动依赖解析和循环依赖检测
- **多节点部署** - 支持串行、并行、滚动、蓝绿四种部署策略
- **安全验证** - 插件签名验证、完整性校验
- **审计日志** - 完整的操作审计日志记录
- **Redis 缓存** - 集成 cmx-buffer 提供 Redis 缓存支持
- **分布式锁** - 支持分布式环境下的部署锁
- **WASM 运行时** - 插件激活和运行管理

## 模块结构

```
cmx-plugin/src/
├── lib.rs           # 模块入口和导出
├── error.rs         # 错误类型定义
├── types.rs         # 类型定义（请求/响应/配置）
├── version.rs       # 语义版本和依赖解析
├── registry.rs      # 插件注册表（ZIP加载/签名验证）
├── fetcher.rs       # 插件源获取器
├── manager.rs       # 插件管理器（核心逻辑）
├── deployment.rs    # 部署协调器
├── activation.rs   # 激活管理和WASM运行时
├── audit.rs        # 审计日志
├── security.rs     # 安全验证器
├── repository.rs   # 数据库仓库结构
├── transaction.rs  # 事务管理
├── db.rs           # 数据库抽象层
└── cache.rs        # Redis缓存管理
```

## 核心组件

### PluginManager

插件管理器是核心组件，负责插件的完整生命周期管理。

```rust
use cmx_plugin::{PluginManager, PluginManagerConfig};

// 创建配置
let config = PluginManagerConfig {
    install_root: PathBuf::from("/path/to/plugins"),
    temp_root: PathBuf::from("/tmp/cmx-plugin"),
    backup_root: PathBuf::from("/path/to/backups"),
    default_db_id: "default".to_string(),
    enable_backup: true,
    max_backup_count: 5,
    require_signature: false,
    registry_url: None,
};

// 创建管理器
let manager = PluginManager::new(config)?;
```

### 安装插件

```rust
use cmx_plugin::{InstallRequest, PluginSource};

let request = InstallRequest {
    plugin_id: Some("my-plugin".to_string()),
    source: PluginSource::Zip {
        path: "/path/to/plugin.zip".to_string(),
    },
    target_db_id: Some("plugin_db".to_string()),
    target_db_type: None,
    target_nodes: None,
    config: None,
    force: false,
    skip_validation: false,
    operator: "admin".to_string(),
};

let response = manager.install(request).await?;
```

### 激活插件

```rust
use cmx_plugin::{ActivateRequest};

let request = ActivateRequest {
    plugin_id: "my-plugin".to_string(),
    config: None,
    operator: "admin".to_string(),
};

let response = manager.activate(request).await?;
```

### 升级插件

```rust
use cmx_plugin::{UpgradeRequest, PluginSource};

let request = UpgradeRequest {
    plugin_id: "my-plugin".to_string(),
    source: PluginSource::Zip {
        path: "/path/to/plugin-v2.zip".to_string(),
    },
    strategy: None,
    force: false,
    operator: "admin".to_string(),
};

let response = manager.upgrade(request).await?;
```

### 回滚插件

```rust
use cmx_plugin::{RollbackRequest};

let request = RollbackRequest {
    plugin_id: "my-plugin".to_string(),
    target_version: "1.0.0".to_string(),
    force: false,
    operator: "admin".to_string(),
};

let response = manager.rollback(request).await?;
```

## 部署策略

### 串行部署

逐个节点部署，遇到错误可选择继续或停止。

```rust
use cmx_plugin::types::DeploymentStrategy;

let strategy = DeploymentStrategy::Serial {
    continue_on_error: true,
};
```

### 并行部署

同时部署到多个节点，支持并发数限制。

```rust
let strategy = DeploymentStrategy::Parallel {
    max_concurrent: 5,
};
```

### 滚动部署

分批次部署，每批完成后等待一段时间。

```rust
let strategy = DeploymentStrategy::Rolling {
    batch_size: 3,
    wait_seconds: 10,
};
```

### 蓝绿部署

同时部署到所有节点，验证通过后切换流量。

```rust
let strategy = DeploymentStrategy::BlueGreen {
    switch_at: Some("now".to_string()),
};
```

## 安全验证

### 使用安全验证器

```rust
use cmx_plugin::{SecurityValidator, SecurityValidatorConfig};

let config = SecurityValidatorConfig {
    require_signature: true,
    trusted_public_keys: vec![],  // 添加受信任的公钥
    verify_file_hash: true,
    max_plugin_size: 100 * 1024 * 1024,  // 100MB
    enable_sandbox: true,
    allowed_imports: vec!["env".to_string(), "wasmtime".to_string()],
};

let validator = SecurityValidator::new(config);
let result = validator.validate_plugin(Path::new("/path/to/plugin.zip")).await?;
```

## 审计日志

### 使用审计日志

```rust
use cmx_plugin::{AuditLogger, OperationType};

let logger = AuditLogger::new();

// 记录安装操作
log_install(&logger, "my-plugin", "admin", "1.0.0", true, None).await;

// 查询审计日志
let filter = AuditLogFilter::new()
    .with_plugin_id("my-plugin")
    .with_operation_type(OperationType::Install);

let result = logger.query(filter).await?;
```

## Redis 缓存集成

### 使用缓存管理器

```rust
use cmx_plugin::{PluginCacheManager, PluginCacheValue};

// 创建缓存管理器（需要先初始化 cmx-buffer）
let cache_manager = PluginCacheManager::new(
    cmx_buffer::GlobalCacheManager::get(),
    cmx_buffer::GlobalLockManager::get(),
);

// 缓存插件信息
let value = PluginCacheValue {
    plugin_id: "my-plugin".to_string(),
    name: "My Plugin".to_string(),
    version: "1.0.0".to_string(),
    status: "active".to_string(),
    install_path: "/path/to/plugins/my-plugin".to_string(),
    activated: true,
    updated_at: Utc::now().timestamp(),
};

cache_manager.cache_plugin(&value).await?;
```

## 数据库抽象

### 实现自定义数据库

```rust
use cmx_plugin::{PluginDatabase, PluginDbRecord, PluginDbError};

struct MyPluginDatabase {
    // 数据库连接
}

impl PluginDatabase for MyPluginDatabase {
    async fn insert_plugin(&self, record: &PluginDbRecord) -> Result<(), PluginDbError> {
        // 实现插入逻辑
        Ok(())
    }

    // ... 实现其他方法
}
```

## 依赖

```toml
[dependencies]
cmx-plugin = { path = "..." }

cmx-core = "0.1"           # 核心数据模型
cmx-metadata = "0.1"      # 元数据管理
cmx-buffer = "0.1"         # Redis缓存和分布式锁
```

## 错误处理

模块定义了丰富的错误类型：

```rust
use cmx_plugin::PluginError;

match result {
    Ok(response) => { /* 成功 */ }
    Err(PluginError::NotFound(msg)) => { /* 资源不存在 */ }
    Err(PluginError::Dependency(msg)) => { /* 依赖冲突 */ }
    Err(PluginError::Security(msg)) => { /* 安全验证失败 */ }
    Err(PluginError::Install(msg)) => { /* 安装失败 */ }
    Err(PluginError::Activate(msg)) => { /* 激活失败 */ }
    Err(e) => { /* 其他错误 */ }
}
```

## 最佳实践

1. **Always 验证插件** - 安装前使用 SecurityValidator 验证插件
2. **启用审计日志** - 记录所有关键操作
3. **使用事务** - 复杂操作使用 TransactionManager
4. **配置回滚** - 设置备份策略以便出现问题时回滚
5. **使用缓存** - 高频访问的插件信息使用 Redis 缓存

## 许可证

MIT
