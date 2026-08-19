# cmx-storage

> 统一对象存储抽象层，支持本地文件系统、S3 兼容存储等多平台，基于 OpenDAL 构建，为上层应用提供一致的文件操作接口。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

## 快速开始

### 安装

cmx-storage 是内部 crate，通常通过 workspace 依赖引入：

```toml
# Cargo.toml
[dependencies]
cmx-storage = { workspace = true }
```

### 核心示例

```rust
use cmx_storage::config::{StorageInstanceConfig, StorageManagerConfig, StorageType};
use cmx_storage::manager::StorageManager;
use cmx_storage::service::DefaultStorageService;
use cmx_storage::types::{FileInfo, UploadRequest};
use bytes::Bytes;
use std::sync::Arc;

// 1. 配置存储实例（以 MinIO S3 为例）
let instance = StorageInstanceConfig {
platform: "minio-1".to_string(),
storage_type: StorageType::S3,
enable_storage: true,
domain: Some("http://localhost:9000/".to_string()),
base_path: "uploads/".to_string(),
access_key: Some("minioadmin".to_string()),
secret_key: Some("minioadmin".to_string()),
region: Some("us-east-1".to_string()),
endpoint: Some("http://localhost:9000".to_string()),
bucket_name: Some("my-bucket".to_string()),
enable_access: false,
path_patterns: None,
storage_path: None,
};

// 2. 构建存储管理器配置
let config = StorageManagerConfig {
instances: vec![instance],
default_platform: Some("minio-1".to_string()),
};

// 3. 创建存储管理器和服务（服务依赖数据库管理器，文件元数据写入 cmx_file_detail）
let manager = Arc::new(StorageManager::new(&config).expect("初始化失败"));
let db = cmx_database::get_default_db_manager();
let storage_service = Arc::new(DefaultStorageService::new(manager, db));

// 4. 上传文件
let request = UploadRequest {
data: Bytes::from("Hello, World!"),
original_filename: Some("hello.txt".to_string()),
content_type: Some("text/plain".to_string()),
object_id: None,
object_type: Some("document".to_string()),
platform: None,  // 使用默认平台
user_metadata: None,
acl: None,
};

let file_info: FileInfo = storage_service.upload(request).await?;
println!("文件上传成功: {}", file_info.url);
```

## 核心功能与特性

| 功能         | 说明                                       |
|------------|------------------------------------------|
| 多平台存储      | 支持 Local 文件系统、S3、MinIO、腾讯云 COS、阿里云 OSS 等 |
| 秒传         | 基于 MD5 哈希检测文件是否已存在，实现秒传                  |
| 缩略图自动生成    | 上传图片时自动生成 200x200 JPEG 缩略图并上传到 OSS       |
| 预签名 URL    | 支持生成下载/上传的临时签名 URL（S3 后端）                |
| 分片上传       | 支持大文件分片上传和断点续传                           |
| 跨平台复制      | 支持不同存储平台间的文件复制                           |
| REST API   | 基于 axum 的 HTTP 接口（路由定义在 cmx-apis/cmx-storage-api） |
| OpenAPI 文档 | 使用 utoipa 生成 Swagger UI 文档               |

### 可选 Features

| Feature   | 默认启用 | 说明                   |
|-----------|------|----------------------|
| `default` | ✅    | 基础功能（OpenDAL、所有存储后端） |

## 模块结构

```
cmx-storage
├── config.rs        # 配置解析（TOML 配置 → Rust 结构体）
├── error.rs         # 错误类型定义
├── global.rs        # GlobalStorageService 全局单例
├── manager.rs       # StorageManager 多平台后端管理器
├── backend/         # 存储后端抽象层
│   ├── mod.rs      # StorageBackend trait + 工厂函数
│   ├── s3.rs       # S3 后端实现（支持预签名、分片）
│   └── local.rs    # 本地文件系统后端实现
├── service/         # StorageService trait + DefaultStorageService（按职责拆分子模块）
│   ├── mod.rs      # trait 定义 + DefaultStorageService（按职责拆分子模块）
│   ├── upload.rs / download.rs / delete.rs / query.rs
│   ├── presign.rs / copy.rs / multipart.rs / thumbnail.rs
│   └── helpers.rs    # 内部辅助（MD5 计算、文件记录创建/查询、秒传检测，pub(super)）
├── types.rs         # 公共类型定义（FileInfo、UploadRequest 等）
├── bmc.rs           # 数据库表元信息与 CRUD 实体
├── path_gen.rs      # 文件存储路径生成策略
├── mime_detect.rs   # MIME Type 检测（魔数 + 扩展名）
└── handler.rs       # handler 侧 AppState（HTTP 路由已迁至 cmx-apis/cmx-storage-api）
```

### 主要模块说明

#### `config` - 配置管理

解析 TOML 配置文件，定义存储实例配置结构：

```toml
[storage]
default_platform = "local-1"

[[storage.instances]]
platform = "local-1"
storage_type = "local"
enable_storage = true
domain = "http://localhost:8080/files/"
base_path = "uploads/"
storage_path = "/data/storage/"

[[storage.instances]]
platform = "amazon-s3-1"
storage_type = "s3"
enable_storage = true
access_key = "AKIAIOSFODNN7EXAMPLE"
secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
region = "ap-east-1"
endpoint = "https://s3.ap-east-1.amazonaws.com/"
bucket_name = "my-bucket"
domain = "https://my-bucket.s3.ap-east-1.amazonaws.com/"
base_path = "s3/"
```

#### `manager` - 存储管理器

通过 `StorageManager` 统一管理多个存储后端实例：

```rust
use cmx_storage::manager::StorageManager;

// 创建存储管理器（从配置初始化所有后端）
let manager = StorageManager::new( & config) ?;

// 获取指定平台的后端
let backend = manager.get_backend(Some("amazon-s3-1")) ?;

// 获取默认平台的后端
let default_backend = manager.get_default_backend() ?;

// 检查平台是否存在
if manager.has_platform("local-1") {
// ...
}
```

#### `service` - 存储服务层

`StorageService` trait 提供面向业务的高级文件操作：

```rust
use cmx_storage::service::{StorageService, DefaultStorageService};
use cmx_storage::types::{FileInfo, FileDownload, FileQuery, FilePage};

// 创建存储服务（需传入数据库管理器，文件元数据入库）
let service: Arc<dyn StorageService> = Arc::new(DefaultStorageService::new(manager, db));

// 上传文件
let file_info: FileInfo = service.upload(request).await?;

// 下载文件
let download: FileDownload = service.download( & file_id).await?;

// 删除文件（软删除/归档）
service.delete( & file_id).await?;

// 查询文件列表
let page: FilePage = service.list_files(FileQuery {
object_type: Some("avatar".to_string()),
page: Some(1),
page_size: Some(20),
..Default::default ()
}).await?;

// 生成预签名下载 URL（有效期 1 小时）
let presigned_url = service.presign_download( & file_id, Duration::from_secs(3600)).await?;

// 复制文件到另一平台
let new_file = service.copy_file( & file_id, Some("minio-1")).await?;
```

#### `handler` - REST API

HTTP 接口（axum）的 handler 与路由已迁至皮肤 crate `crates/libs/cmx-apis/cmx-storage-api`
（2026-07/08 handler 大迁移），经其 `ModuleRoutes`（前缀 storage）由 `cmx-platform-app`
挂载到 `/api` 下；cmx-storage 本体仅保留 handler 使用的 `AppState`。集成后访问 `/api/storage/*`：

| 方法     | 路径                                | 说明                        |
|--------|-----------------------------------|---------------------------|
| POST   | `/api/storage/upload`             | 上传文件（multipart/form-data） |
| GET    | `/api/storage/download`           | 下载文件                      |
| POST   | `/api/storage/batch-download`     | 批量下载（ZIP）                 |
| GET    | `/api/storage/info`               | 获取文件信息                    |
| POST   | `/api/storage/delete`             | 删除文件（既有接口，已按新规范改用 POST） |
| POST   | `/api/storage/page`               | 分页查询文件列表                  |
| POST   | `/api/storage/presign-download`   | 预签名下载 URL                 |
| POST   | `/api/storage/presign-upload`     | 预签名上传 URL                 |
| POST   | `/api/storage/multipart/init`     | 初始化分片上传                   |
| POST   | `/api/storage/multipart/part`     | 分片上传回调                    |
| POST   | `/api/storage/multipart/complete` | 完成分片上传                    |
| POST   | `/api/storage/multipart/abort`    | 取消分片上传                    |

## 使用指南

### 一、配置与初始化

#### 1.1 在 dev.toml 中配置存储

在应用配置文件中添加存储配置：

```toml
[storage]
default_platform = "local-1"

# 本地文件系统存储（开发和测试环境推荐）
[[storage.instances]]
platform = "local-1"
storage_type = "local"
enable_storage = true
domain = "http://localhost:8080/files/"
base_path = "uploads/"
storage_path = "/data/cmx/storage"
enable_access = true

# MinIO / S3 存储（生产环境推荐）
[[storage.instances]]
platform = "amazon-s3-1"
storage_type = "s3"
enable_storage = true
access_key = "your-access-key"
secret_key = "your-secret-key"
region = "us-east-1"
endpoint = "http://192.168.1.14:9000/"
bucket_name = "cmx-bucket"
domain = "http://192.168.1.14:9000/"
base_path = "portalcenter/"
```

#### 1.2 在应用启动流程中初始化存储服务

初始化函数由 `cmx-service-base` 提供（`crates/libs/cmx-service-base/src/storage.rs`，
feature `storage`），返回 `Result<()>`，并会额外注册本地文件的静态访问路由：

```rust
pub async fn init_storage() -> Result<()> {
    // 1. 从配置加载存储配置
    let config = ConfigManager::global();
    let storage_config = StorageManagerConfig::from_config(&config)?;

    // 2. 创建存储管理器（初始化所有后端），并收集本地访问配置
    let manager = Arc::new(StorageManager::new(&storage_config)?);
    let local_access_configs: Vec<(String, String)> = manager
        .get_local_access_configs()
        .into_iter()
        .map(|(pattern, path)| (pattern.to_string(), path.to_string()))
        .collect();

    // 3. 创建存储服务（依赖数据库管理器，文件元数据入库）
    let db_manager = get_default_db_manager();
    let service: Arc<dyn cmx_storage::service::StorageService> =
        Arc::new(DefaultStorageService::new(manager, db_manager));

    // 4. 注册到全局单例，并注册本地文件静态访问路由（local 后端）
    GlobalStorageService::initialize(service)?;
    GlobalStorageService::init_local_access_configs(local_access_configs);
    Ok(())
}
```

#### 1.3 初始化顺序

`init_storage()` 由 `cmx-platform-app` 的启动流程自动调用，位于数据源初始化之后
（存储服务依赖数据库写文件元数据）：

```rust
// cmx-platform-app/src/lib.rs（节选）
init_datasources()...   // 数据库先就绪（存储元数据入库依赖数据库）
init_storage().await;   // 文件存储服务初始化
```

自行装配的应用需保证：先初始化数据库（`cmx-database` 默认数据源），再调用 `init_storage()`。

#### 1.4 全局存储服务使用

`GlobalStorageService` 提供线程安全的全局单例，用于在应用任意位置获取存储服务实例。

##### 1.4.1 获取全局服务

```rust
use cmx_storage::global::GlobalStorageService;
use cmx_storage::service::StorageService;
use std::sync::Arc;

// 方式一：获取 Arc<dyn StorageService>
let service: &Arc<dyn StorageService> = GlobalStorageService::get().service();

// 方式二：在 async 上下文中直接使用
let file_info = GlobalStorageService::get().service().upload(request).await?;
```

**⚠️ 注意**：`GlobalStorageService::get()` 会在服务未初始化时 panic，因此确保在应用启动时调用 `initialize()`。

##### 1.4.2 在 axum Handler 中使用

有三种方式在 handler 中使用存储服务：

**方式一：通过 State 提取（推荐）**

```rust
use axum::{extract::State, Json};
use cmx_api_core::CmxAppState;

// 在主应用状态中注入存储服务
// CmxAppState 已实现 FromRef<StorageService>，可自动提取
pub async fn my_handler(
    State(state): State<CmxAppState>,
    Json(payload): Json<MyRequest>,
) -> Result<Json<Response>, AppError> {
    // 通过 state.storage_service() 获取
    let service = state.storage_service().expect("存储服务未初始化");
    let file_info = service.upload(request).await?;
    Ok(Json(Response::ok(file_info)))
}
```

**方式二：通过 FromRef 为 handler::AppState 实现自动提取**

```rust
// cmx-api-core/src/app_state.rs
// （注：此 impl 必须定义在 CmxAppState 的本地 crate cmx-api-core，以满足孤儿规则）
impl axum::extract::FromRef<CmxAppState> for cmx_storage::handler::AppState {
    fn from_ref(state: &CmxAppState) -> Self {
        Self {
            storage_service: state
                .storage_service()
                .cloned()
                .expect("存储服务未初始化"),
        }
    }
}

// handler 中直接使用
use cmx_storage::handler::AppState;

pub async fn upload_handler(
    State(state): State<AppState>,  // 自动从 CmxAppState 提取
    // ...
) {
    let file_info = state.storage_service.upload(request).await?;
}
```

**方式三：直接使用全局单例**

```rust
use cmx_storage::global::GlobalStorageService;

pub async fn my_handler() -> Result<Json<Response>, AppError> {
    // 任意位置直接获取
    let service = GlobalStorageService::get().service();
    let file_info = service.download(&file_id).await?;
    Ok(Json(Response::ok(file_info)))
}
```

##### 1.4.3 初始化检查

在调试或测试时，可以检查服务是否已初始化：

```rust
use cmx_storage::global::GlobalStorageService;

// 检查是否已初始化
if GlobalStorageService::get().service().exists(&file_id).await? {
    // 文件存在
}

// 安全获取（使用前检查）
use std::sync::OnceLock;
static GLOBAL_STORAGE_SERVICE: OnceLock<()> = OnceLock::new();

// 不要这样做！仅作示例
fn get_service_unsafe() -> &'static Arc<dyn StorageService> {
    GlobalStorageService::get().service()
}
```

##### 1.4.4 生命周期示意

```
应用启动
    │
    ▼
init_storage() 调用
    │
    ├── StorageManager::new()  ──→ 初始化所有存储后端
    │
    ├── DefaultStorageService::new(manager, db)  ──→ 创建服务实例
    │
    ├── GlobalStorageService::initialize()  ──→ 注册全局单例 ✅
    │
    └── GlobalStorageService::init_local_access_configs()  ──→ 注册本地静态访问路由
    │
    ▼
应用运行中
    │
    └── GlobalStorageService::get().service()  ──→ 任意位置获取服务
    │
    ▼
应用关闭（无需显式清理，进程结束时自动释放）
```

### 二、服务层使用

#### 2.1 文件上传

```rust
use cmx_storage::service::StorageService;
use cmx_storage::types::{FileInfo, UploadRequest};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;

// 获取全局存储服务
let service = cmx_storage::global::GlobalStorageService::get().service();

// 构建上传请求
let mut user_meta = HashMap::new();
user_meta.insert("uploader_id".to_string(), "user_123".to_string());

let request = UploadRequest {
    data: Bytes::from("文件内容"),
    original_filename: Some("report.pdf".to_string()),
    content_type: Some("application/pdf".to_string()),
    object_id: Some("doc_456".to_string()),
    object_type: Some("document".to_string()),
    platform: None,  // None 表示使用默认平台
    user_metadata: Some(user_meta),
    acl: None,
};

// 执行上传（自动计算 MD5 秒传检查）
let file_info: FileInfo = service.upload(request).await?;
println!("上传成功！文件 ID: {}", file_info.id);
println!("访问 URL: {}", file_info.url);
```

#### 2.2 文件下载

```rust
use cmx_storage::types::FileDownload;

// 根据文件 ID 下载
let download: FileDownload = service.download("file_123_id").await?;

// 输出文件内容
println!("文件名: {:?}", download.file_info.original_filename);
println!("文件大小: {} bytes", download.content_length);
println!("Content-Type: {}", download.content_type);

// 将文件写入磁盘
std::fs::write("/tmp/downloaded_file", &*download.data)?;
```

#### 2.3 分页查询文件

```rust
use cmx_storage::types::{FileQuery, FilePage};

// 构建查询条件
let query = FileQuery {
    object_type: Some("avatar".to_string()),
    object_id: Some("user_123".to_string()),
    platform: None,
    page: Some(1),
    page_size: Some(20),
    original_filename: None,
};

// 执行分页查询
let page: FilePage = service.list_files(query).await?;

println!("共 {} 条记录，第 {}/{} 页",
    page.total, page.page, (page.total + page.page_size - 1) / page.page_size);

for file in page.items {
    println!("- {} ({})", file.original_filename, file.url);
}
```

#### 2.4 预签名 URL（临时访问链接）

```rust
use std::time::Duration;

// 生成预签名下载链接（有效期 1 小时）
let download_url = service.presign_download("file_123", Duration::from_secs(3600)).await?;
println!("下载链接（1小时内有效）: {}", download_url);

// 生成预签名上传链接
use cmx_storage::types::PresignUploadRequest;
let presign_req = PresignUploadRequest {
    filename: "upload.pdf".to_string(),
    content_type: Some("application/pdf".to_string()),
    platform: None,
};
let upload_result = service.presign_upload(presign_req, Duration::from_secs(3600)).await?;
println!("上传链接: {}", upload_result.url);
println!("文件 ID: {}", upload_result.file_id);
```

#### 2.5 分片上传（大文件）

```rust
use cmx_storage::types::{MultipartInitRequest, PartData, MultipartSession};

// 1. 初始化分片上传（假设 10 个分片）
let init_req = MultipartInitRequest {
    filename: "large_video.mp4".to_string(),
    total_parts: 10,
    content_type: Some("video/mp4".to_string()),
    object_type: Some("video".to_string()),
    object_id: None,
    platform: None,
};
let session: MultipartSession = service.init_multipart_upload(init_req).await?;
println!("分片上传会话 ID: {}", session.upload_id);

// 2. 上传每个分片（通常由客户端直接上传到预签名 URL，这里模拟回调）
for i in 1..=10 {
    let part = PartData {
        upload_id: session.upload_id.clone(),
        part_number: i,
        e_tag: format!("\"etag-{}\"", i),
        part_size: 5 * 1024 * 1024,  // 每个分片 5MB
    };
    let _part_info = service.upload_part(&session.upload_id, part).await?;
    println!("分片 {}/{} 上传完成", i, 10);
}

// 3. 完成分片上传
let final_file: FileInfo = service.complete_multipart_upload(&session.upload_id).await?;
println!("大文件上传完成: {}", final_file.url);

// 4. 取消分片上传（如果中途失败）
// service.abort_multipart_upload(&session.upload_id).await?;
```

### 三、REST API 使用

#### 3.1 文件上传（multipart/form-data）

```bash
curl -X POST http://localhost:8080/api/storage/upload \
  -F "file=@/path/to/file.pdf" \
  -F "object_type=document" \
  -F "object_id=doc_123" \
  -F "platform=local-1"
```

返回示例：

```json
{
  "code": 0,
  "message": "success",
  "data": {
    "id": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
    "url": "http://localhost:8080/files/uploads/document/2026/05/14/3fa85f64-5717-4562-b3fc-2c963f66afa6.pdf",
    "size": 1048576,
    "filename": "3fa85f64-5717-4562-b3fc-2c963f66afa6.pdf",
    "original_filename": "file.pdf",
    "platform": "local-1",
    "content_type": "application/pdf"
  }
}
```

#### 3.2 文件下载

```bash
# 下载原文件
curl -O http://localhost:8080/api/storage/download?file_id=3fa85f64-5717-4562-b3fc-2c963f66afa6

# 下载缩略图
curl -O "http://localhost:8080/api/storage/download?file_id=xxx&thumbnail=1"
```

#### 3.3 批量下载（ZIP）

```bash
curl -X POST http://localhost:8080/api/storage/batch-download \
  -H "Content-Type: application/json" \
  -d '{"file_ids": ["id1", "id2", "id3"]}' \
  -o files.zip
```

#### 3.4 分页查询

```bash
curl -X POST http://localhost:8080/api/storage/page \
  -H "Content-Type: application/json" \
  -d '{
    "object_type": "avatar",
    "object_id": "user_123",
    "page": 1,
    "page_size": 20
  }'
```

### 四、错误处理

cmx-storage 定义了清晰的错误类型，便于定位问题：

```rust
use cmx_storage::{Error, Result};

// 错误类型枚举
enum Error {
    ConfigError(String),       // 配置错误
    UploadError(String),        // 上传失败
    DownloadError(String),     // 下载失败
    DeleteError(String),        // 删除失败
    NotFoundError(String),     // 文件不存在
    CopyError(String),          // 复制失败
    PresignError(String),       // 预签名失败
    MultipartError(String),     // 分片上传错误
    UnsupportedError(String),   // 不支持的操作
    StorageError(String),      // 其他存储错误
}

// 错误处理示例
match service.upload(request).await {
    Ok(file_info) => {
        println!("上传成功: {}", file_info.url);
    }
    Err(e) => {
        match e {
            Error::NotFoundError(msg) => {
                eprintln!("文件不存在: {}", msg);
            }
            Error::UploadError(msg) => {
                eprintln!("上传失败: {}", msg);
            }
            Error::UnsupportedError(msg) => {
                eprintln!("后端不支持此操作: {}", msg);
            }
            _ => {
                eprintln!("其他错误: {}", e);
            }
        }
    }
}
```

### 五、数据库表结构

存储服务依赖两张数据库表：

#### cmx_file_detail（文件详情表）

| 字段                | 类型           | 说明               |
|-------------------|--------------|------------------|
| id                | varchar(64)  | 主键 UUID          |
| url               | varchar(512) | 文件访问 URL         |
| size              | int8         | 文件大小（字节）         |
| filename          | varchar(256) | 存储文件名（UUID）      |
| original_filename | varchar(256) | 原始文件名            |
| base_path         | varchar(256) | 基础存储路径           |
| path              | varchar(256) | 完整存储路径           |
| ext               | varchar(32)  | 文件扩展名            |
| content_type      | varchar(128) | MIME 类型          |
| platform          | varchar(32)  | 存储平台标识           |
| th_url            | varchar(512) | 缩略图 URL          |
| object_id         | varchar(64)  | 关联对象 ID          |
| object_type       | varchar(32)  | 关联对象类型           |
| hash_info         | text         | MD5 等哈希信息        |
| upload_id         | varchar(128) | 分片上传会话 ID        |
| archived          | int4         | 归档状态（0-正常，1-已删除） |
| create_time       | timestamp    | 创建时间             |
| update_time       | timestamp    | 更新时间             |

#### cmx_file_part_detail（分片信息表）

| 字段          | 类型           | 说明           |
|-------------|--------------|--------------|
| id          | varchar(64)  | 主键 UUID      |
| platform    | varchar(32)  | 存储平台标识       |
| upload_id   | varchar(128) | 分片上传会话 ID    |
| e_tag       | varchar(255) | 分片 ETag      |
| part_number | int4         | 分片编号（从 1 开始） |
| part_size   | int8         | 分片大小（字节）     |
| hash_info   | text         | 分片哈希信息       |
| archived    | int4         | 归档状态         |
| create_time | timestamp    | 创建时间         |
| update_time | timestamp    | 更新时间         |

详细 DDL 参见 `example/sqlexample/oss_pg.sql`（PostgreSQL）和 `example/sqlexample/oss.sql`（MySQL）。

## 缩略图自动生成

上传图片文件时，系统会自动生成缩略图：

- **触发条件**：MIME 类型为 `image/jpeg`、`image/png`、`image/gif`、`image/webp`、`image/bmp`
- **缩略图尺寸**：最大 200x200，保持原始宽高比
- **输出格式**：统一为 JPEG
- **存储路径**：`{base_path}/thumbnails/thumb_{file_id}.jpg`
- **容错**：缩略图生成或上传失败不会影响主文件上传，仅记录 warn 日志

上传成功后，`FileInfo` 中的 `th_url`、`th_filename`、`th_size`、`th_content_type` 字段会自动填充，同时更新数据库记录。

## 存储路径格式

不同存储类型使用不同的日期目录格式：

| 存储类型  | 路径格式                                                | 示例                                   |
|-------|-----------------------------------------------------|--------------------------------------|
| Local | `{base_path}/{object_type}/{yyyyMM}/{uuid}.{ext}`   | `uploads/avatar/202605/a1b2c3d4.jpg` |
| S3    | `{base_path}/{object_type}/{yyyyMMdd}/{uuid}.{ext}` | `s3/avatar/2026/05/15/a1b2c3d4.jpg`  |

Local 存储使用年月（`yyyyMM`）作为目录层级，简化目录结构；S3 存储保持年月日（`yyyy/MM/dd`）的细粒度目录结构。

## 常见问题

### Q: 如何选择 Local 和 S3 存储类型？

**A**: 根据场景选择：

- **Local 存储**：开发/测试环境、小规模部署、不需要跨机器访问
- **S3 存储**：生产环境、需要跨服务/跨机器访问、需要 CDN 加速

### Q: 如何实现文件秒传？

**A**: 上传时系统会自动计算文件 MD5 哈希：

1. 如果数据库中已存在相同 hash_info + platform 的记录，直接复制记录（秒传）
2. 否则执行正常的上传流程

### Q: 如何添加新的存储后端类型？

**A**: 参考 `backend/local.rs` 或 `backend/s3.rs` 的实现：

1. 创建新的后端模块，实现 `StorageBackend` trait
2. 在 `backend/mod.rs` 的 `create_backend()` 工厂函数中添加分支
3. 在 `config.rs` 的 `StorageType` 枚举中添加新类型

### Q: Local 存储支持预签名 URL 吗？

**A**: 不支持。`LocalBackend` 的 `presign_read` 和 `presign_write` 返回 `UnsupportedError`。如需预签名功能，请使用 S3 类型存储。

### Q: 分片上传适用于哪些场景？

**A**: 适用于：

- 上传超大文件（超过 100MB）
- 网络不稳定环境（支持断点续传）
- 需要前端直传 OSS 的场景（通过预签名 URL）

### Q: 缩略图生成失败会影响文件上传吗？

**A**: 不会。缩略图生成和上传采用容错设计：

1. 非图片文件不触发缩略图生成
2. 图片解码失败仅记录 warn 日志，返回 `None`
3. 缩略图上传到 OSS 失败也仅记录 warn 日志
4. 主文件上传流程不受任何影响
