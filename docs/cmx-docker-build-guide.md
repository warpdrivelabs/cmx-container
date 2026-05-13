# CMX-Web Docker 镜像构建与部署指南

## 1. 概述

cmx-web 支持通过 Docker 容器化部署，提供一致的运行环境、便捷的扩缩容和版本管理。本文档涵盖镜像构建、本地开发、生产部署的完整流程。

### 相关文件

| 文件 | 说明 |
|------|------|
| `docker/Dockerfile` | 多阶段构建定义（cargo-chef 缓存优化） |
| `docker/.dockerignore` | 构建上下文排除规则 |
| `docker/docker-compose.yml` | 开发环境编排（cmx-web + PostgreSQL + Redis + r-nacos） |
| `docker/docker-compose.prod.yml` | 生产环境编排（多副本 + 资源限制） |
| `config/docker.toml` | 容器内配置模板 |
| `docker/scripts/build-docker.sh` | Linux/macOS 构建脚本 |
| `docker/scripts/build-docker.ps1` | Windows PowerShell 构建脚本 |

---

## 2. 前置条件

- Docker 20.10+（支持多阶段构建）
- Docker Compose V2（`docker compose` 命令）
- 至少 4GB 可用内存（Rust 编译需要）
- 至少 10GB 可用磁盘空间

---

## 3. 镜像构建

### 3.1 快速构建

**Windows (PowerShell)：**

```powershell
# 构建最新版本
.\docker\scripts\build-docker.ps1

# 构建指定版本
.\docker\scripts\build-docker.ps1 -Version "1.0.0"

# 构建并推送到镜像仓库
.\docker\scripts\build-docker.ps1 -Version "1.0.0" -Registry "registry.example.com"
```

**Linux/macOS：**

```bash
# 构建最新版本
chmod +x docker/scripts/build-docker.sh
./docker/scripts/build-docker.sh

# 构建指定版本
./docker/scripts/build-docker.sh 1.0.0

# 构建并推送到镜像仓库
./docker/scripts/build-docker.sh 1.0.0 registry.example.com
```

### 3.2 手动构建

```bash
docker build -t cmx-web:latest .
```

### 3.3 多平台构建

如需构建 `linux/amd64` + `linux/arm64` 双平台镜像：

```bash
# 创建 buildx 构建器（首次使用）
docker buildx create --name cmx-builder --use

# 构建并推送多平台镜像
docker buildx build \
    --platform linux/amd64,linux/arm64 \
    -t cmx-web:latest \
    --push \
    .
```

> **注意**：多平台构建必须配合 `--push` 使用，无法直接加载到本地。

### 3.4 构建缓存说明

Dockerfile 采用 `cargo-chef` 实现依赖层缓存，构建流程分为 4 个阶段：

```
阶段1 (chef)      → 安装 cargo-chef 工具
阶段2 (planner)   → 分析源码生成 recipe.json（依赖清单）
阶段3 (builder)   → 先用 recipe.json 编译依赖（可缓存层），再复制源码编译业务代码
阶段4 (runtime)   → 仅复制二进制和运行时文件，生成最终镜像
```

**缓存命中条件**：当 `Cargo.toml` / `Cargo.lock` 未变更时，阶段3 的依赖编译层会被 Docker 缓存命中，构建时间从 ~10 分钟缩短至 ~2 分钟。

---

## 4. 本地开发（Docker Compose）

### 4.1 启动全套服务

```bash
docker compose up -d
```

这将启动 4 个服务：

| 服务 | 端口 | 说明 |
|------|------|------|
| cmx-web | 8080 | Web 应用 |
| postgres | 5432 | PostgreSQL 16 数据库 |
| redis | 6379 | Redis 7 缓存 |
| nacos | 8848, 9848 | r-nacos 注册与配置中心 |

### 4.2 启用 Nacos

默认 Nacos 未启用。如需启用，创建 `.env` 文件或设置环境变量：

```bash
export NACOS_ENABLED=true
export NACOS_NAMING_ENABLED=true
export NACOS_CONFIG_ENABLED=true
docker compose up -d
```

### 4.3 查看日志

```bash
# 查看所有服务日志
docker compose logs -f

# 仅查看 cmx-web 日志
docker compose logs -f cmx-web
```

### 4.4 停止服务

```bash
docker compose down

# 停止并删除数据卷（重置所有数据）
docker compose down -v
```

### 4.5 重新构建

修改源码后需要重新构建镜像：

```bash
docker compose build cmx-web
docker compose up -d cmx-web
```

---

## 5. 生产部署

### 5.1 使用生产编排文件

```bash
# 设置版本号
export VERSION=1.0.0

# 设置加密密钥（必须修改）
export CMX_ENCRYPT_KEY=your_production_key

# 启动生产环境
docker compose -f docker/docker-compose.prod.yml up -d
```

### 5.2 生产环境差异

| 配置项 | 开发环境 | 生产环境 |
|--------|----------|----------|
| 日志级别 | `info` | `warn` |
| Nacos | 默认禁用 | 默认启用 |
| 副本数 | 1 | 2 |
| CPU 限制 | 无 | 2 核 |
| 内存限制 | 无 | 2GB |
| 重启策略 | `unless-stopped` | `always` |

### 5.3 配置文件挂载

生产环境需要将配置文件挂载到容器的 `/app/config` 目录：

```
./config/
├── docker.toml          # 主配置文件（必须）
└── (其他配置文件)
```

**关键配置项**（需根据生产环境修改 `config/docker.toml`）：

```toml
# 数据库连接（修改为生产数据库地址）
[[databases]]
db_url = "postgresql://user:password@prod-db-host:5432/cmx"

# Redis 连接（修改为生产 Redis 地址）
[redis]
url = "redis://prod-redis-host:6379/13"

# 迁移文件目录（容器内固定路径，无需修改）
[migration]
dir = "/app/docs/sql/migrations"
```

### 5.4 环境变量

生产环境必须通过环境变量注入以下配置：

| 环境变量 | 必需 | 说明 |
|----------|------|------|
| `CMX_ENCRYPT_KEY` | ✅ | 字段加密密钥，必须与数据加密时使用的密钥一致 |
| `CONFIG_FILE` | ✅ | 配置文件路径，默认 `/app/config/docker.toml` |
| `WEB_FOLDER` | ✅ | 静态文件目录，默认 `/app/web-folder` |
| `NACOS_ENABLED` | ❌ | 是否启用 Nacos，默认 `false` |
| `NACOS_SERVER_ADDR` | ❌ | Nacos 服务器地址 |
| `NACOS_NAMESPACE` | ❌ | Nacos 命名空间 |
| `NACOS_NAMING_ENABLED` | ❌ | 是否启用服务注册 |
| `NACOS_CONFIG_ENABLED` | ❌ | 是否启用配置中心 |

---

## 6. 容器内目录结构

```
/app/
├── web-server                  # 应用二进制
├── web-folder/                 # 静态文件（Swagger UI 等）
├── docs/sql/migrations/        # 数据库迁移 SQL 文件
├── config/                     # 配置文件目录（Volume 挂载）
│   └── docker.toml
├── plugins/                    # 插件目录（Volume 挂载）
│   ├── root/                   # 插件安装根目录
│   ├── backup/                 # 插件备份目录
│   ├── temp/                   # 插件临时目录
│   └── uploads/                # 插件上传目录
└── logs/                       # 日志目录（Volume 挂载）
```

---

## 7. 健康检查

容器内置健康检查端点：

```bash
# 检查服务状态
curl http://localhost:8080/api/health

# 返回示例
{"status":"ok"}
```

Docker 自动健康检查配置：

- 检查间隔：30 秒
- 超时时间：5 秒
- 启动等待：10 秒
- 重试次数：3 次

手动检查容器健康状态：

```bash
docker inspect --format='{{.State.Health.Status}}' cmx-web
```

---

## 8. 数据库迁移

容器启动时会自动执行数据库迁移（由 `MigrationRunner` 处理）：

1. 迁移文件位于容器内 `/app/docs/sql/migrations/` 目录
2. 迁移记录写入 `cmx_schema_migrations` 表
3. 集群环境下通过 Redis 分布式锁保证只有一个节点执行迁移
4. 其他节点等待锁释放后跳过迁移，继续启动

如需禁用自动迁移，在配置文件中设置：

```toml
[migration]
dir = ""    # 空路径将跳过迁移
```

---

## 9. 常见问题

### 9.1 构建时内存不足

Rust 编译过程内存消耗较大。如果构建时 OOM，可增加 Docker 的内存限制：

- Docker Desktop → Settings → Resources → Memory → 建议至少 4GB

### 9.2 构建缓存失效

以下变更会导致依赖缓存失效（需要重新编译依赖）：

- 修改任何 `Cargo.toml` 文件
- 修改 `Cargo.lock` 文件
- 添加/删除 crate

仅修改 `src/` 下的 Rust 源码不会导致依赖缓存失效。

### 9.3 容器无法连接 PostgreSQL

检查以下几点：

1. 数据库 URL 中的主机名应使用 Docker Compose 服务名 `postgres`，而非 `localhost`
2. 确保 PostgreSQL 健康检查通过后再启动 cmx-web（`depends_on` + `condition: service_healthy`）
3. 首次启动时 PostgreSQL 需要初始化，可能需要等待 10-30 秒

### 9.4 容器无法连接 Redis

同 PostgreSQL，主机名应使用 Docker Compose 服务名 `redis`。

### 9.5 Nacos 连接失败

- 确认 `NACOS_ENABLED=true` 已设置
- 确认 Nacos 服务已启动：`docker compose logs nacos`
- Nacos 启动较慢，可能需要等待 15-30 秒
- Nacos 连接失败不会阻止应用启动，仅记录警告日志

### 9.6 插件目录为空

插件目录 `/app/plugins` 通过 Volume 挂载，首次启动时为空。可通过以下方式安装插件：

1. 在配置文件中启用自动安装（`[plugin.auto_install]`）
2. 通过 API 接口上传安装
3. 从插件市场安装

### 9.7 查看容器内日志

```bash
# Docker 标准输出日志
docker compose logs cmx-web

# 容器内文件日志（挂载到宿主机 ./logs/ 目录）
ls ./logs/
```

---

## 10. 镜像信息

| 属性 | 值 |
|------|------|
| 基础镜像 | `debian:bookworm-slim` |
| 运行用户 | `cmx`（非 root） |
| 预期镜像大小 | 120-170MB |
| 暴露端口 | 8080 |
| 架构 | linux/amd64, linux/arm64 |
