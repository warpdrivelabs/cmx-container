# web-server

> 基于 Axum 构建的 Web 服务器，提供 API 路由管理、中间件处理、插件系统等功能。

## 项目简介

cmx-container 项目的 Web 服务器入口，基于 Axum Web 框架构建，提供 RESTful API、插件生命周期管理、数据库连接池、Redis 缓存等核心功能。

## 快速开始

### 安装

```bash
cargo build --release
cargo run --bin web-server
```

### 核心示例

服务器在 `0.0.0.0:8080` 启动，自动加载：

- 全局配置（支持 Nacos 远程配置覆盖）
- 数据库连接池
- Redis 缓存和分布式锁
- WASM 运行时（Extism）
- 插件管理器
- 文件存储服务
- 服务管理器（延迟加载）

## 核心功能与特性

| 功能       | 说明                                   |
|----------|--------------------------------------|
| API 路由管理 | 基于 Axum 的现代 Web 框架实现路由，支持 RESTful 接口 |
| 插件系统     | 支持运行时插件加载与管理，具备热插拔能力                 |
| 中间件支持    | 提供 CORS、Cookie 管理、请求追踪、访问日志等中间件      |
| 服务管理器    | 延迟加载服务组件，提升系统启动效率                    |
| 分布式缓存    | 基于 Redis 实现缓存和分布式锁                   |
| WASM 运行时 | 基于 Extism 的 WebAssembly 运行时          |
| 异步运行时    | 基于 Tokio，支持高并发处理                     |
| 调试支持     | 集成调试功能，支持运行时调试信息获取                   |
| 配置中心     | 支持 Nacos 配置中心，实现配置热更新                |

### Features

| Feature   | 默认启用 | 说明   |
|-----------|------|------|
| `default` | ✅    | 基础功能 |

## 模块结构

```text
web-server
├── src/
│   ├── main.rs              # 程序入口，初始化和启动逻辑
│   ├── error.rs             # 错误类型定义
│   ├── routes.rs            # API 路由定义
│   └── config/              # 配置模块
│       ├── mod.rs           # 模块导出和 WebConfig
│       ├── cache.rs         # Redis 缓存初始化
│       ├── datasource.rs    # 数据源初始化
│       ├── migration.rs     # 数据库迁移
│       ├── nacos.rs         # Nacos 配置中心
│       ├── plugins.rs       # 插件管理器
│       ├── runtime.rs       # WASM 运行时
│       ├── services.rs      # 服务管理器
│       └── storage.rs       # 文件存储
└── Cargo.toml
```

## 主要模块说明

### `main.rs`

应用程序主模块，负责初始化和启动服务器。

初始化顺序：

1. 日志系统（控制台 + 文件双输出）
2. 全局配置（含 Nacos 远程配置覆盖）
3. 加密服务
4. Redis 缓存
5. 数据库数据源
6. 文件存储
7. 调试会话
8. WASM 运行时
9. 全局事件总线
10. 服务管理器
11. 插件管理器

### `routes.rs`

负责路由的统一管理，注册 `/api` 下的所有路由以及 Swagger 路由。

### `config` 模块

提供应用程序初始化所需的各项配置功能：

| 子模块          | 功能                 |
|--------------|--------------------|
| `cache`      | Redis 缓存和分布式锁初始化   |
| `datasource` | 数据源配置加载、持久化和注册     |
| `migration`  | 数据库迁移执行            |
| `nacos`      | Nacos 连接、配置拉取、服务注册 |
| `plugins`    | 插件管理器初始化           |
| `runtime`    | WASM 运行时和宿主函数注册    |
| `services`   | 服务仓储和生命周期管理        |
| `storage`    | 文件存储服务初始化          |

### `error` 模块

定义应用程序级别的错误类型：

```rust
pub enum Error {
    ConfigError(String),      // 配置加载、解析或验证失败
    ServerSetup(String),      // 服务器启动设置、地址绑定失败
    DatasourceInit(String),   // 数据源连接、注册或初始化失败
    RuntimeInit(String),      // WASM 运行时引擎、宿主函数注册失败
    PluginInit(String),      // 插件管理器初始化失败
    ServiceInit(String),     // 服务管理器初始化失败
    StorageInit(String),     // 存储配置加载或服务初始化失败
    Migration(String),       // 数据库迁移执行失败
    Io(#[from] std::io::Error), // IO 操作失败
}
```

## 使用指南

### 一、环境变量配置

#### 1.1 必需的环境变量

```bash
# 配置文件路径
CONFIG_FILE=config/default.toml

# Web 静态文件目录
web_folder=static

# Redis 连接
redis.url=redis://127.0.0.1:6379

# 插件安装目录
plugin.install_root=plugins
```

#### 1.2 Nacos 配置（可选）

```bash
# Nacos 连接配置
NACOS_ENABLED=true
NACOS_HOST=127.0.0.1
NACOS_PORT=8848
NACOS_USERNAME=nacos
NACOS_PASSWORD=nacos

# Nacos 命名服务
NACOS_NAMING_ENABLED=true
NACOS_NAMING_REGISTER_IP=true

# Nacos 配置中心
NACOS_CONFIG_ENABLED=true
NACOS_CONFIG_DATA_ID=cmx-config
NACOS_CONFIG_GROUP=DEFAULT_GROUP
```

#### 1.3 配置文件示例 (config/default.toml)

```toml
[server]
host = "0.0.0.0"
port = 8080

[database]
url = "postgresql://user:pass@localhost:5432/cmx"
max_connections = 20

[redis]
url = "redis://127.0.0.1:6379"

[plugin]
install_root = "plugins"
auto_activate = true

[logging]
level = "info"
format = "json"
```

### 二、启动服务器

#### 2.1 开发模式启动

```bash
# 设置环境变量
export CONFIG_FILE=config/development.toml
export RUST_LOG=debug

# 启动服务器
cargo run --bin web-server
```

#### 2.2 生产模式启动

```bash
# 使用 release 模式构建
cargo build --release --bin web-server

# 后台运行
./target/release/web-server &

# 或使用 nohup
nohup ./target/release/web-server > server.log 2>&1 &
```

### 三、API 路由

#### 3.1 健康检查

```bash
# 服务器健康状态
curl http://localhost:8080/api/health

# 响应
{"status": "ok", "timestamp": "2024-01-15T10:30:00Z"}
```

#### 3.2 插件管理 API

```bash
# 安装插件
curl -X POST http://localhost:8080/api/plugins/install \
  -H "Content-Type: application/json" \
  -d '{"source": {"type": "local", "path": "/plugins/my-plugin.zip"}}'

# 激活插件
curl -X POST http://localhost:8080/api/plugins/{plugin_id}/activate

# 停用插件
curl -X POST http://localhost:8080/api/plugins/{plugin_id}/deactivate

# 升级插件
curl -X POST http://localhost:8080/api/plugins/{plugin_id}/upgrade \
  -H "Content-Type: application/json" \
  -d '{"version": "2.0.0", "source": {"type": "local", "path": "/plugins/my-plugin-v2.zip"}}'

# 卸载插件
curl -X DELETE http://localhost:8080/api/plugins/{plugin_id}

# 获取插件列表
curl http://localhost:8080/api/plugins

# 获取插件详情
curl http://localhost:8080/api/plugins/{plugin_id}
```

#### 3.3 服务编排 API

```bash
# 执行服务编排
curl -X POST http://localhost:8080/api/services/{service_id}/execute \
  -H "Content-Type: application/json" \
  -d '{"input": {"data": "test"}, "trace_id": "req-001"}'

# 查询服务状态
curl http://localhost:8080/api/services/{service_id}

# 获取服务定义
curl http://localhost:8080/api/services/{service_id}/definition
```

#### 3.4 调试 API

```bash
# 启动调试会话
curl -X POST http://localhost:8080/api/debug/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "plugin_id": "my-plugin",
    "function": "my_function",
    "wasm_path": "/plugins/my-plugin/1.0.0/plugin.wasm"
  }'

# 获取调试信息
curl http://localhost:8080/api/debug/sessions/{session_id}

# 删除调试会话
curl -X DELETE http://localhost:8080/api/debug/sessions/{session_id}
```

### 四、中间件配置

#### 4.1 CORS 中间件

服务器默认配置了 CORS 中间件，支持跨域请求。

#### 4.2 请求追踪中间件

所有请求自动添加 `X-Request-Id` 响应头：

```bash
curl -I http://localhost:8080/api/health

# 响应头
X-Request-Id: req-abc123
X-Trace-Id: trace-xyz789
```

#### 4.3 请求体大小限制

默认限制 100MB 请求体。

### 五、Swagger API 文档

访问 `http://localhost:8080/swagger` 查看交互式 API 文档。

```bash
# 导出 OpenAPI 规范
curl http://localhost:8080/swagger/openapi.json -o openapi.json
```

### 六、日志配置

#### 6.1 日志格式

双层输出：

- 控制台：简洁格式，带颜色，便于开发调试
- 文件：JSON 格式，便于日志收集系统解析

```json
{
  "timestamp": "2024-01-15T10:30:00.123Z",
  "level": "INFO",
  "message": "Request processed",
  "request_id": "req-abc123",
  "method": "POST",
  "path": "/api/plugins/install",
  "status": 200,
  "duration_ms": 150
}
```

#### 6.2 日志级别

```bash
# 开发环境：详细日志
RUST_LOG=debug

# 生产环境：info 级别
RUST_LOG=info

# 仅错误
RUST_LOG=error
```

### 七、数据库连接池

#### 7.1 连接池配置

```toml
[database]
url = "postgresql://user:pass@localhost:5432/cmx"
max_connections = 20
min_connections = 5
connection_timeout = 30
idle_timeout = 600
max_lifetime = 3600
```

#### 7.2 连接池监控

```bash
# 获取连接池状态
curl http://localhost:8080/api/admin/db/pool
```

### 八、缓存配置

#### 8.1 Redis 配置

```toml
[redis]
url = "redis://127.0.0.1:6379"
```

### 九、优雅关闭

服务器支持优雅关闭：

```bash
# 发送 SIGTERM 信号
kill -TERM <pid>

# 或 Ctrl+C
```

关闭流程：

1. 停止接收新请求
2. 等待现有请求处理完成
3. 关闭数据库连接池
4. 从 Nacos 注销服务实例
5. 关闭日志系统

### 十、错误处理

服务器初始化过程中可能遇到以下错误：

```rust
// 配置错误
Error::ConfigError("无法从配置管理器获取 redis.url")

// 服务器设置错误
Error::ServerSetup("绑定地址失败: Address already in use")

// 数据源初始化错误
Error::DatasourceInit("注册数据源失败: Connection refused")

// WASM 运行时错误
Error::RuntimeInit("Extism 引擎初始化失败")
```

### 十一、常见问题

#### 11.1 启动失败

检查以下配置是否正确：
- `CONFIG_FILE` 指向的配置文件是否存在
- 数据库连接是否可达
- Redis 连接是否可达
- 插件目录是否存在且有写权限

#### 11.2 插件激活失败

```bash
# 查看插件加载日志
RUST_LOG=debug cargo run --bin web-server 2>&1 | grep plugin

# 检查插件包是否完整
unzip -t /path/to/plugin.zip
```

#### 11.3 Nacos 连接失败

```bash
# 检查 Nacos 服务是否运行
curl http://127.0.0.1:8848/nacos/v1/console/health

# 如果 Nacos 不可用，可禁用它
NACOS_ENABLED=false
```

#### 11.4 性能问题

```bash
# 启用性能分析
RUST_LOG=debug cargo run --bin web-server

# 查看慢请求日志
grep "slow request" logs/cmx-server.log
```
