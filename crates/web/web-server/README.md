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
- 全局配置（从 `CONFIG_FILE` 环境变量指定路径读取）
- 数据库连接池
- Redis 缓存
- WASM 运行时
- 插件管理器

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| API 路由管理 | 基于 Axum 的现代 Web 框架实现路由，支持 RESTful 接口 |
| 插件系统 | 支持运行时插件加载与管理，具备热插拔能力 |
| 中间件支持 | 提供 CORS、Cookie 管理、请求追踪、访问日志等中间件 |
| 服务管理器 | 延迟加载服务组件，提升系统启动效率 |
| 分布式缓存 | 基于 Redis 实现缓存和分布式锁 |
| 异步运行时 | 基于 Tokio，支持高并发处理 |
| 调试支持 | 集成调试功能，支持运行时调试信息获取 |

## 模块结构

```
web-server
├── src/
│   ├── main.rs              # 程序入口
│   ├── config.rs            # 配置加载和初始化
│   ├── datasource_init.rs   # 数据源初始化
│   ├── error.rs             # 错误类型定义
│   ├── plugins.rs           # 插件系统
│   └── routes.rs            # API 路由定义
└── Cargo.toml
```

## 主要模块说明

### `main.rs`

主要职责：
1. 初始化环境变量和日志系统
2. 加载并初始化数据库、缓存、插件和各种服务
3. 配置 Web 服务器的路由和中间件
4. 启动服务器监听端口

### `routes.rs`

负责路由的统一管理，注册 `/api` 下的所有路由以及 Swagger 路由。

### `config.rs`

负责初始化：
- 全局配置加载
- Redis 缓存
- WASM 运行时
- 插件和全局事件总线
- 服务管理器

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

#### 1.2 可选的环境变量

```bash
# 数据库配置
database.url=postgresql://user:pass@localhost:5432/cmx
database.max_connections=20

# 日志配置
RUST_LOG=info
RUST_LOG_FORMAT=json

# 服务器配置
SERVER_HOST=0.0.0.0
SERVER_PORT=8080

# CORS 配置
CORS_ALLOWED_ORIGINS=http://localhost:3000
CORS_ALLOWED_METHODS=GET,POST,PUT,DELETE
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

#### 2.3 Docker 部署

```dockerfile
FROM rust:latest as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin web-server

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/web-server /usr/local/bin/
COPY config/ /etc/cmx/
EXPOSE 8080
CMD ["web-server"]
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

服务器默认配置了 CORS 中间件，支持跨域请求：

```toml
[cors]
allowed_origins = ["http://localhost:3000"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
allowed_headers = ["Content-Type", "Authorization"]
expose_headers = ["X-Request-Id"]
max_age = 3600
```

#### 4.2 请求追踪中间件

所有请求自动添加 `X-Request-Id` 响应头：

```bash
curl -I http://localhost:8080/api/health

# 响应头
X-Request-Id: req-abc123
X-Trace-Id: trace-xyz789
```

#### 4.3 限流中间件

```toml
[rate_limit]
enabled = true
requests_per_minute = 100
burst = 20
```

### 五、Swagger API 文档

访问 `http://localhost:8080/swagger` 查看交互式 API 文档。

```bash
# 导出 OpenAPI 规范
curl http://localhost:8080/swagger/openapi.json -o openapi.json
```

### 六、日志配置

#### 6.1 日志格式

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

# 响应示例
{
  "total_connections": 20,
  "idle_connections": 15,
  "waiting_requests": 0
}
```

### 八、缓存配置

#### 8.1 Redis 配置

```toml
[redis]
url = "redis://127.0.0.1:6379"
pool_size = 10
timeout = 5
```

#### 8.2 缓存操作 API

```bash
# 设置缓存
curl -X POST http://localhost:8080/api/cache \
  -H "Content-Type: application/json" \
  -d '{"key": "user:001", "value": "{\"name\":\"test\"}", "ttl": 3600}'

# 获取缓存
curl http://localhost:8080/api/cache/user:001

# 删除缓存
curl -X DELETE http://localhost:8080/api/cache/user:001
```

### 九、安全配置

#### 9.1 安全响应头

服务器自动添加以下安全响应头：
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `X-XSS-Protection: 1; mode=block`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`

#### 9.2 认证配置

```toml
[auth]
enabled = true
jwt_secret = "your-secret-key"
token_expiry = 3600
```

### 十、监控与健康检查

#### 10.1 健康检查端点

```bash
# 基础健康检查
curl http://localhost:8080/api/health

# 详细健康检查（包含依赖）
curl http://localhost:8080/api/health/detailed

# 响应示例
{
  "status": "healthy",
  "timestamp": "2024-01-15T10:30:00Z",
  "components": {
    "database": "healthy",
    "redis": "healthy",
    "plugins": "healthy"
  }
}
```

#### 10.2 指标端点

```bash
# Prometheus 格式指标
curl http://localhost:8080/metrics
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

#### 11.3 性能问题

```bash
# 启用性能分析
RUST_LOG=debug cargo run --bin web-server

# 查看慢请求日志
grep "slow request" logs/cmx-server.log
```
