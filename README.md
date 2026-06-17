# cmx-container

> 插件化容器运行时，支持 WebAssembly 插件热插拔、服务编排、分布式存储等功能。

## 项目简介

cmx-container 是一个基于 Rust 构建的插件化容器运行时，支持运行时加载和管理 WebAssembly 插件，提供服务编排、文件存储、缓存、数据库等核心功能。

## 核心功能

| 功能     | 说明                            |
|--------|-------------------------------|
| 插件系统   | 基于 Extism 的 WebAssembly 插件热插拔 |
| 服务编排   | 可视化服务编排引擎                     |
| 文件存储   | 支持本地存储和 S3 兼容对象存储             |
| 分布式缓存  | 基于 Redis 的缓存和分布式锁             |
| 数据库    | 支持 PostgreSQL 数据库             |
| Web 框架 | 基于 Axum 的高性能 HTTP 服务器         |
| 配置中心   | 支持 Nacos 配置中心                 |

## 目录结构

```text
cmx-container/
├── crates/
│   ├── web/
│   │   └── web-server/          # Web 服务器入口
│   ├── libs/                    # 内部库
│   │   ├── cmx-api/             # HTTP API 层
│   │   ├── cmx-core/            # 核心类型定义
│   │   ├── cmx-plugin/          # 插件管理
│   │   ├── cmx-plugin-sdk/      # WASM 插件 SDK
│   │   ├── cmx-runtime/         # WASM 运行时
│   │   ├── cmx-service/         # 服务编排引擎
│   │   ├── cmx-traits/          # 跨模块 trait 接口
│   │   ├── cmx-utils/           # 工具库
│   │   ├── cmx-buffer/          # 缓存
│   │   ├── cmx-database/        # 数据库
│   │   ├── cmx-debug/           # 调试
│   │   ├── cmx-storage/         # 文件存储
│   │   ├── cmx-metadata/        # 元数据
│   │   ├── cmx-infra/           # 基础设施
│   │   │   ├── cmx-rpc/         # gRPC 通信
│   │   │   ├── cmx-nacos/       # Nacos 注册中心
│   │   │   ├── cmx-buffer/      # 缓存实现
│   │   │   ├── cmx-database/    # 数据库实现
│   │   │   └── cmx-storage/     # 存储实现
│   │   └── cmx-rpc-gen/         # volo gRPC 代码生成
│   └── tools/
├── config/                      # 配置文件
├── docs/                        # 文档
├── docker/                      # Docker 相关
└── scripts/                     # 脚本
```

## 快速开始

### 环境要求

- Rust 1.70+
- PostgreSQL 12+
- Redis 6+
- (可选) Nacos 2.x

### 1. 克隆并构建

```bash
# 克隆项目
git clone <repository-url>
cd cmx-container

# Debug 构建
cargo build

# Release 构建
cargo build --release
```

> **注意**：项目使用 volo-build 在编译时自动生成 gRPC 代码（`cmx-rpc-gen`）。
> 如果遇到 `cmx_service_orchestrator.rs not found` 错误，请先清理构建缓存：
> ```bash
> rm -rf target/debug/build/cmx-rpc-gen-* && cargo build
> ```

### 2. 配置

复制配置文件模板并修改：

```bash
cp config/config_template.toml config.toml
```

编辑 `config.toml`：

```toml
[server]
host = "0.0.0.0"
port = 8080

[[databases]]
db_id = "primary"
db_type = "postgres"
db_url = "postgresql://postgres:postgres@localhost:5432/cmx"
default = true

[databases.pool_config]
max_connections = 20

[redis]
url = "redis://localhost:6379/13"

[plugin]
install_root = "plugins/root"
backup_root = "plugins/backup"
temp_root = "plugins/temp"
upload_root = "plugins/uploads"

[[storage.instances]]
platform = "local-1"
storage_type = "local"
storage_path = "./storage"
domain = "http://localhost:8080/files/"
enable_access = true
```

### 3. 启动 web-server

#### 开发模式

```bash
# 设置环境变量
export CONFIG_FILE=config.toml
export RUST_LOG=debug

# 启动服务器
cargo run --bin web-server
```

#### 生产模式

```bash
# Release 构建
cargo build --release --bin web-server

# 后台运行
nohup ./target/release/web-server > server.log 2>&1 &

# 或直接运行
./target/release/web-server
```

#### 使用 Docker

```bash
# 构建 Docker 镜像
./docker/scripts/build-docker.sh --image-name cmx-container --push

# 使用 docker-compose 启动
docker-compose -f docker/docker-compose.local.yml up -d
```

### 4. 验证服务

```bash
# 健康检查
curl http://localhost:8080/api/health

# 响应示例
{"status":"ok","timestamp":"2024-01-15T10:30:00Z"}
```

### 5. API 文档

启动后访问 `http://localhost:8080/swagger` 查看交互式 API 文档。

## VSCode 调试配置

项目已配置 VSCode 调试任务，位于 `.vscode/` 目录。

### 调试任务

1. **Debug Rust Program** - 启动调试会话
2. **cargo build web-server** - Debug 构建任务
3. **cargo build web-server --release** - Release 构建任务

### 使用方法

1. 确保已安装 VSCode 扩展 **CodeLLDB**
2. 按 `F5` 启动调试，或使用命令面板 (`Ctrl+Shift+P`) 执行 "Debug: Start Debugging"
3. 在代码中设置断点
4. 调试控制台支持：
   - Continue/Pause
   - Step Over/Into/Out
   - 变量监视
   - 调用栈查看
   - 条件断点

### 环境变量配置

调试配置默认设置以下环境变量：

```json
{
   "RUST_LOG": "debug",
   "RUST_BACKTRACE": "1"
}
```

可在 `.vscode/launch.json` 中修改。

## 常见问题

### 端口被占用

```bash
# 查看端口占用
lsof -i :8080

# 或使用 fuser
fuser 8080/tcp
```

### 数据库连接失败

检查 `config.toml` 中的数据库 URL 是否正确，确保 PostgreSQL 服务已启动。

### Redis 连接失败

检查 `config.toml` 中的 Redis URL 是否正确，确保 Redis 服务已启动。

### 插件加载失败

```bash
# 查看插件加载日志
RUST_LOG=debug cargo run --bin web-server 2>&1 | grep plugin

# 检查插件目录权限
ls -la plugins/
```
### 插件调用失败

```text
call to route_check encountered an error: array had incorrect length, expected 6
```

`只要宿主端 SVRContext / FunctionInput / FunctionOutput 结构变更，所有依赖 cmx-plugin-sdk 的 WASM 插件都需要重新编译。`



## 构建说明

### 跨平台构建

```bash
# Linux musl 静态构建
cross build --release --target x86_64-unknown-linux-musl

# Windows
cross build --release --target x86_64-pc-windows-gnu
```

### Docker 多架构构建

```bash
# 构建并推送
./docker/scripts/build-docker.sh --image-name cmx-container --push
```

## 许可证

MIT
