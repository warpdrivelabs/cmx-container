# 插件市场集成 cmx-storage 改造方案

## 一、现状分析

### 1.1 当前插件发布流程

```
发布者 → POST /api/marketplace/plugin/publish (JSON Body)
         ↓
     提供 download_url（外部 URL 字符串）
         ↓
     直接存储到 cmx_marketplace_plugin_version.download_url 字段
```

**问题**：

- `download_url` 只是一个 URL 字符串，插件包文件不在本系统管理
- 没有统一的文件存储管理，无法控制文件的生命周期
- 无法利用 cmx-storage 的秒传、缩略图、分片上传等能力

### 1.2 当前插件安装流程

```
用户 → POST /api/marketplace/plugin/install
       ↓
   查询 cmx_marketplace_plugin_version 获取 download_url
       ↓
   构建 PluginSource::Remote { url: download_url, checksum }
       ↓
   RemoteFetcher 使用 reqwest HTTP GET 下载到临时目录
       ↓
   InstallService 执行安装流程
```

**问题**：

- 依赖外部 URL 的可用性，无法保证文件持久化
- 没有利用 cmx-storage 的统一存储能力

### 1.3 cmx-storage 能力概览

| 能力                    | 说明                     | 对插件市场的价值   |
|-----------------------|------------------------|------------|
| 多平台存储                 | Local/S3/MinIO/OSS/COS | 灵活选择存储后端   |
| 秒传                    | 基于 MD5 哈希去重            | 相同插件包不重复存储 |
| 预签名 URL               | 生成临时签名下载链接             | 安全的插件分发    |
| 分片上传                  | 大文件分片上传+断点续传           | 支持大型插件包    |
| REST API              | `/api/storage/*` 完整接口  | 统一文件管理     |
| GlobalStorageService  | 全局单例，任意位置获取            | 服务层直接调用    |
| object_type/object_id | 文件关联对象                 | 关联到插件版本    |

---

## 二、改造目标

1. **发布插件时**：上传插件包文件到 cmx-storage，自动获取存储 URL 和 file_id
2. **安装插件时**：通过 cmx-storage 服务层下载插件包，而非直接 HTTP 请求外部 URL
3. **版本管理**：在版本表中记录 storage_file_id，建立插件版本与存储文件的关联
4. **兼容性**：保留 download_url 字段兼容外部 URL，优先使用 storage_file_id

---

## 三、数据库变更

### 3.1 cmx_marketplace_plugin_version 表新增字段

```sql
-- 新增存储文件 ID 字段
ALTER TABLE cmx_marketplace_plugin_version
    ADD COLUMN IF NOT EXISTS storage_file_id VARCHAR (64);

COMMENT
ON COLUMN cmx_marketplace_plugin_version.storage_file_id
IS 'cmx-storage 文件唯一标识，关联 cmx_file_detail.id';
```

### 3.2 迁移脚本

在 `docs/sql/migrations/` 下新增迁移脚本：

**文件**：`20260520_002_add_storage_file_id_to_plugin_version.up.sql`

```sql
ALTER TABLE cmx_marketplace_plugin_version
    ADD COLUMN IF NOT EXISTS storage_file_id VARCHAR (64);

COMMENT
ON COLUMN cmx_marketplace_plugin_version.storage_file_id
IS 'cmx-storage 文件唯一标识，关联 cmx_file_detail.id';
```

**文件**：`20260520_002_add_storage_file_id_to_plugin_version.down.sql`

```sql
ALTER TABLE cmx_marketplace_plugin_version
DROP
COLUMN IF EXISTS storage_file_id;
```

---

## 四、改造详细方案

### 4.1 改造点总览

| 序号 | 模块              | 文件                                             | 改造内容                                                                                                           |
|----|-----------------|------------------------------------------------|----------------------------------------------------------------------------------------------------------------|
| 1  | 数据模型            | `cmx-plugin/src/marketplace/model.rs`          | `MarketplacePluginVersion` 新增 `storage_file_id` 字段；`MarketplacePluginVersionForCreate` 新增 `storage_file_id` 字段 |
| 2  | 发布请求            | `cmx-api/src/handlers/marketplace/request.rs`  | `PublishPluginRequest` 移除 `download_url`，改为发布时通过 cmx-storage 上传获取                                              |
| 3  | 版本响应            | `cmx-api/src/handlers/marketplace/response.rs` | `MarketplaceVersionResponse` 新增 `storage_file_id` 字段                                                           |
| 4  | 发布 Handler      | `cmx-api/src/handlers/marketplace/handler.rs`  | `marketplace_plugin_publish` 改为 multipart/form-data，集成 cmx-storage 上传                                          |
| 5  | 安装 Handler      | `cmx-api/src/handlers/marketplace/handler.rs`  | `marketplace_plugin_install` 优先使用 storage_file_id 通过 cmx-storage 下载                                            |
| 6  | 插件来源            | `cmx-plugin/src/domain/plugin.rs`              | `PluginSource` 新增 `Storage` 变体                                                                                 |
| 7  | 包获取             | `cmx-plugin/src/common/package.rs`             | `fetch_package` 支持 `PluginSource::Storage`                                                                     |
| 8  | Storage Fetcher | `cmx-plugin/src/fetcher/`                      | 新增 `StorageFetcher`，通过 cmx-storage 服务下载文件                                                                      |

---

### 4.2 改造点 1：数据模型层（cmx-plugin）

#### 文件：`crates/libs/cmx-plugin/src/marketplace/model.rs`

**MarketplacePluginVersion 结构体新增字段**：

```rust
// 在 MarketplacePluginVersion 中新增
pub struct MarketplacePluginVersion {
    // ... 现有字段 ...
    /// cmx-storage 文件唯一标识。
    pub storage_file_id: Option<String>,
}
```

**MarketplacePluginVersionForCreate 新增字段**：

```rust
pub struct MarketplacePluginVersionForCreate {
    // ... 现有字段 ...
    /// cmx-storage 文件唯一标识。
    pub storage_file_id: Option<String>,
}
```

**MarketplaceService::row_to_plugin_version 映射更新**：

在 repository.rs 中将数据库行映射到 `MarketplacePluginVersion` 时，新增 `storage_file_id` 的读取。

---

### 4.3 改造点 2：发布请求结构（cmx-api）

#### 文件：`crates/libs/cmx-api/src/handlers/marketplace/request.rs`

**关键变化**：发布接口从 JSON Body 改为 multipart/form-data，不再由调用方提供 `download_url`。

> 注意：由于 axum 的 multipart 提取器与 Json 提取器不兼容，需要在 handler 层处理 multipart 解析。`PublishPluginRequest`
> 结构体将作为 multipart 表单字段的映射目标，不再是直接反序列化的 JSON body。

保留 `PublishPluginRequest` 结构体，但语义变为从 multipart 表单字段中解析：

```rust
/// 发布插件到市场的请求（从 multipart 表单字段解析）。
pub struct PublishPluginRequest {
    // ... 现有字段保持不变，但移除 download_url ...
    // download_url 不再由调用方提供，改为上传文件后由系统自动生成

    // 移除以下字段（由系统从上传文件自动获取）：
    // pub download_url: Option<String>,
    // pub package_size: Option<i64>,
    // pub checksum: Option<String>,
}
```

**新增**：Multipart 发布表单辅助结构

```rust
/// 插件发布 multipart 表单参数
///
/// 从 multipart/form-data 请求中提取的字段
pub struct PublishPluginForm {
    /// 插件基本信息（JSON 字符串）
    pub plugin_info: PublishPluginRequest,
    /// 插件包文件数据
    pub file_data: bytes::Bytes,
    /// 插件包原始文件名
    pub file_name: String,
    /// 插件包 Content-Type
    pub file_content_type: Option<String>,
}
```

---

### 4.4 改造点 3：版本响应结构（cmx-api）

#### 文件：`crates/libs/cmx-api/src/handlers/marketplace/response.rs`

**MarketplaceVersionResponse 新增字段**：

```rust
pub struct MarketplaceVersionResponse {
    // ... 现有字段 ...
    /// 存储文件 ID。
    pub storage_file_id: Option<String>,
}
```

**convert_version_to_response 函数更新**：

```rust
fn convert_version_to_response(version: cmx_plugin::MarketplacePluginVersion) -> MarketplaceVersionResponse {
    MarketplaceVersionResponse {
        // ... 现有字段映射 ...
        storage_file_id: version.storage_file_id,
    }
}
```

---

### 4.5 改造点 4：发布 Handler 改造（核心）

#### 文件：`crates/libs/cmx-api/src/handlers/marketplace/handler.rs`

**`marketplace_plugin_publish` 改造要点**：

1. 接口从 `Json(req): Json<PublishPluginRequest>` 改为 `Multipart` 提取器
2. 从 multipart 中提取插件元信息（JSON 字段）和文件数据
3. 使用 `GlobalStorageService` 上传文件到 cmx-storage
4. 将返回的 `FileInfo` 中的 `id` 和 `url` 保存到版本记录

**改造后的伪代码流程**：

```rust
pub async fn marketplace_plugin_publish(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,  // 改为 multipart 提取器
) -> Result<Json<ApiResp<MarketplacePluginResponse>>> {
    // 1. 解析 multipart 字段
    let mut plugin_info: Option<PublishPluginRequest> = None;
    let mut file_data: Option<Bytes> = None;
    let mut file_name = String::new();
    let mut file_content_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("plugin_info") => {
                let text = field.text().await?;
                plugin_info = Some(serde_json::from_str(&text)?);
            }
            Some("file") => {
                file_name = field.file_name()
                    .unwrap_or("plugin.zip").to_string();
                file_content_type = field.content_type().map(String::from);
                file_data = Some(field.bytes().await?);
            }
            _ => {}
        }
    }

    let req = plugin_info.ok_or(Error::bad_request("缺少 plugin_info 字段"))?;
    let file_bytes = file_data.ok_or(Error::bad_request("缺少 file 字段"))?;

    // 2. 上传到 cmx-storage
    let storage_service = cmx_storage::global::GlobalStorageService::get().service();
    let upload_request = cmx_storage::types::UploadRequest {
        data: file_bytes.clone(),
        original_filename: Some(file_name.clone()),
        content_type: file_content_type.or(Some("application/zip".to_string())),
        object_type: Some("marketplace_plugin".to_string()),
        object_id: Some(req.plugin_id.clone()),
        platform: None,
        user_metadata: None,
        acl: None,
    };
    let file_info = storage_service.upload(upload_request).await
        .map_err(|e| Error::internal_error(format!("上传插件包到存储失败: {}", e)))?;

    // 3. 构建 plugin_req（与现有逻辑一致）
    let plugin_req = MarketplacePluginForCreate { ... };

    // 4. 构建 version_req，使用存储信息
    let version_req = MarketplacePluginVersionForCreate {
        plugin_id: req.plugin_id.clone(),
        version: req.version,
        download_url: Some(file_info.url.clone()),      // 存储 URL
        storage_file_id: Some(file_info.id.clone()),     // 存储 file_id
        package_size: Some(file_info.size),               // 从存储获取
        checksum: file_info.hash_info,                    // MD5 哈希
        // ... 其他字段 ...
        ..Default::default()
    };

    // 5. 调用 service 发布（与现有逻辑一致）
    let service = get_marketplace_service().await;
    let plugin = service.publish_plugin(plugin_req, version_req).await?;

    Ok(Json(ApiResp::ok(convert_plugin_to_response(plugin))))
}
```

**上传 object_type 命名规范**：使用 `marketplace_plugin` 作为 object_type，便于在 cmx-storage 中按类型查询和管理插件文件。

---

### 4.6 改造点 5：安装 Handler 改造

#### 文件：`crates/libs/cmx-api/src/handlers/marketplace/handler.rs`

**`marketplace_plugin_install` 改造要点**：

1. 查询版本信息后，优先检查 `storage_file_id`
2. 如果有 `storage_file_id`，使用 cmx-storage 下载插件包
3. 如果没有（兼容旧数据），降级为使用 `download_url` 的原有逻辑

**两种实现方案**：

#### 方案 A：服务层直接下载（推荐）

```rust
pub async fn marketplace_plugin_install(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<MarketInstallRequest>,
) -> Result<Json<ApiResp<MarketInstallResponse>>> {
    let service = get_marketplace_service().await;

    // 1. 查询版本信息（与现有逻辑一致）
    let version_info = /* ... */;

    // 2. 根据存储类型决定下载方式
    let source = if let Some(ref storage_file_id) = version_info.storage_file_id {
        // 优先使用 cmx-storage 下载
        PluginSource::Storage {
            file_id: storage_file_id.clone(),
            checksum: version_info.checksum.clone(),
        }
    } else {
        // 降级：使用外部 URL
        let download_url = version_info.download_url.clone()
            .ok_or_else(|| Error::bad_request("该版本没有提供下载地址"))?;
        PluginSource::Remote {
            url: download_url,
            checksum: version_info.checksum.clone(),
        }
    };

    // 3. 执行安装（与现有逻辑一致）
    let manager = cmx_plugin::GlobalPluginManager::get();
    let install_req = InstallRequest { source, ... };
    let result = manager.install(install_req).await?;

    // 4. 记录统计（与现有逻辑一致）
    // ...

    Ok(Json(ApiResp::ok(response)))
}
```

#### 方案 B：直接使用存储 URL

如果 cmx-storage 的文件 URL 可以通过 HTTP 直接访问（如 Local 存储通过 `domain` 配置提供了 HTTP 访问地址），则可以直接使用
`download_url`（已由 cmx-storage 生成），无需修改 `PluginSource`。此方案改动最小，但要求存储 URL 必须可通过 HTTP 访问。

**推荐方案 A**，因为它：

- 支持本地文件系统存储（无需 HTTP 服务）
- 支持预签名 URL（S3 场景）
- 更好地与 cmx-storage 集成

---

### 4.7 改造点 6：PluginSource 新增 Storage 变体

#### 文件：`crates/libs/cmx-plugin/src/domain/plugin.rs`

```rust
pub enum PluginSource {
    /// 本地文件
    Local { path: PathBuf },
    /// 远程 URL
    Remote { url: String, checksum: Option<String> },
    /// 远程注册表
    Registry { registry_url: Option<String>, package_name: String },
    /// cmx-storage 存储（新增）
    Storage { file_id: String, checksum: Option<String> },
}
```

#### 文件：`crates/libs/cmx-plugin/src/fetcher/source.rs`

同步新增 `Storage` 变体：

```rust
pub enum PluginSource {
    Local { path: PathBuf },
    Remote { url: String, checksum: Option<String> },
    Registry { registry_url: String, package_name: String, version_constraint: Option<String> },
    /// cmx-storage 存储
    Storage { file_id: String, checksum: Option<String> },
}
```

---

### 4.8 改造点 7：PackageUtils 支持 Storage 来源

#### 文件：`crates/libs/cmx-plugin/src/common/package.rs`

在 `fetch_package` 方法中新增 `PluginSource::Storage` 分支：

```rust
PluginSource::Storage { file_id, checksum } => {
let fetcher = StorageFetcher::new( self.deps.temp_root.clone());
fetcher.fetch( & crate::fetcher::source::PluginSource::storage(file_id.clone(), checksum.clone()))
.await
}
```

---

### 4.9 改造点 8：新增 StorageFetcher

#### 新增文件：`crates/libs/cmx-plugin/src/fetcher/storage.rs`

```rust
//! cmx-storage 存储获取器
//!
//! 通过 cmx-storage 的 GlobalStorageService 下载插件包文件。

use std::path::PathBuf;
use cmx_storage::global::GlobalStorageService;
// ... 其他 imports

pub struct StorageFetcher {
    temp_dir: PathBuf,
}

impl StorageFetcher {
    pub fn new(temp_dir: PathBuf) -> Self {
        Self { temp_dir }
    }

    pub async fn fetch(&self, source: &PluginSource) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Storage { file_id, checksum } => {
                // 1. 通过 GlobalStorageService 下载
                let service = GlobalStorageService::get().service();
                let download = service.download(file_id).await
                    .map_err(|e| PluginError::Fetcher(
                        format!("从 cmx-storage 下载文件失败: {}", e)
                    ))?;

                // 2. 确定目标文件路径
                let filename = download.file_info.original_filename
                    .unwrap_or_else(|| format!("{}.zip", file_id));
                let target_path = self.temp_dir.join(&filename);

                // 3. 确保临时目录存在
                std::fs::create_dir_all(&self.temp_dir)?;

                // 4. 写入临时文件
                std::fs::write(&target_path, &download.data)?;

                // 5. 可选的校验和验证
                if let Some(expected_checksum) = checksum {
                    // 验证校验和（MD5 或 SHA256）
                    self.verify_checksum(&target_path, expected_checksum)?;
                }

                Ok(target_path)
            }
            _ => Err(PluginError::Fetcher("来源类型不是 cmx-storage".to_string())),
        }
    }
}
```

**依赖关系**：cmx-plugin 需要新增对 cmx-storage 的依赖：

```toml
# crates/libs/cmx-plugin/Cargo.toml
# 内部依赖 - 存储服务
cmx-storage = { workspace = true }
```

---

## 五、改造流程图

### 5.1 改造后的发布流程

```
发布者 → POST /api/marketplace/plugin/publish (multipart/form-data)
         ↓
     ┌────────────────────────────────────────────┐
     │ Handler 解析 multipart：                      │
     │   - plugin_info: JSON 字符串（插件元信息）     │
     │   - file: 二进制文件数据（插件包）              │
     └────────────────────────────────────────────┘
         ↓
     ┌────────────────────────────────────────────┐
     │ cmx-storage 上传：                           │
     │   - UploadRequest { data, object_type, ... }│
     │   - 返回 FileInfo { id, url, size, hash }   │
     └────────────────────────────────────────────┘
         ↓
     ┌────────────────────────────────────────────┐
     │ 保存版本记录：                                │
     │   - download_url = file_info.url            │
     │   - storage_file_id = file_info.id          │
     │   - package_size = file_info.size            │
     │   - checksum = file_info.hash_info           │
     └────────────────────────────────────────────┘
         ↓
     返回发布结果
```

### 5.2 改造后的安装流程

```
用户 → POST /api/marketplace/plugin/install
       ↓
   查询版本信息，获取 storage_file_id
       ↓
   ┌─── storage_file_id 存在？ ───┐
   │                               │
   │ 是                            │ 否
   ↓                               ↓
   PluginSource::Storage         PluginSource::Remote
   { file_id, checksum }         { download_url, checksum }
   ↓                               ↓
   StorageFetcher                RemoteFetcher
   (cmx-storage 服务层下载)       (HTTP GET 下载)
   ↓                               ↓
   └───────────┬───────────────────┘
               ↓
       InstallService::install()
               ↓
       安装完成 + 记录统计
```

---

## 六、接口变更汇总

### 6.1 变更的接口

| 接口                                     | 变更类型         | 变更说明                                                                        |
|----------------------------------------|--------------|-----------------------------------------------------------------------------|
| `POST /api/marketplace/plugin/publish` | **Breaking** | Content-Type 从 `application/json` 改为 `multipart/form-data`，新增 `file` 文件上传字段 |
| `POST /api/marketplace/plugin/install` | 增强           | 内部逻辑优先使用 cmx-storage 下载，接口参数不变                                              |

### 6.2 变更后的发布接口

```
POST /api/marketplace/plugin/publish
Content-Type: multipart/form-data

字段：
- plugin_info: JSON 字符串，包含插件元信息（原 PublishPluginRequest 去掉 download_url/package_size/checksum）
- file: 二进制文件，插件包（.zip/.wasm）
```

**plugin_info JSON 示例**：

```json
{
  "plugin_id": "my-plugin",
  "name": "我的插件",
  "description": "插件描述",
  "version": "1.0.0",
  "category": "工具类",
  "changelog": "初始版本",
  "icon_url": "https://example.com/icon.png"
}
```

**curl 调用示例**：

```bash
curl -X POST http://localhost:8080/api/marketplace/plugin/publish \
  -F "plugin_info=@plugin-meta.json;type=application/json" \
  -F "file=@my-plugin-v1.0.0.zip;type=application/zip"
```

### 6.3 版本响应新增字段

```json
{
  "id": "...",
  "pluginId": "my-plugin",
  "version": "1.0.0",
  "downloadUrl": "http://localhost:8080/files/uploads/marketplace_plugin/202605/xxx.zip",
  "storageFileId": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
  "packageSize": 1048576,
  "checksum": "d41d8cd98f00b204e9800998ecf8427e",
  ...
}
```

---

## 七、实施步骤（建议顺序）

### 第 1 步：数据库迁移

1. 创建迁移脚本 `20260520_002_add_storage_file_id_to_plugin_version.up.sql`
2. 创建回滚脚本 `20260520_002_add_storage_file_id_to_plugin_version.down.sql`
3. 执行迁移，验证字段添加成功

### 第 2 步：cmx-plugin 数据模型更新

1. 在 `marketplace/model.rs` 中为 `MarketplacePluginVersion` 和 `MarketplacePluginVersionForCreate` 添加
   `storage_file_id` 字段
2. 在 `marketplace/repository.rs` 中更新行映射逻辑
3. 确保编译通过

### 第 3 步：PluginSource 扩展

1. 在 `domain/plugin.rs` 的 `PluginSource` 枚举中新增 `Storage` 变体
2. 在 `fetcher/source.rs` 的 `PluginSource` 枚举中同步新增
3. 在 `common/package.rs` 的 `fetch_package` 中新增 `Storage` 分支

### 第 4 步：新增 StorageFetcher

1. 创建 `fetcher/storage.rs`，实现通过 `GlobalStorageService` 下载文件
2. 在 `fetcher/mod.rs` 中注册模块
3. 在 `Cargo.toml` 中添加对 `cmx-storage` 的依赖

### 第 5 步：cmx-api 发布接口改造

1. 修改 `marketplace/request.rs`，调整 `PublishPluginRequest`
2. 修改 `marketplace/handler.rs` 中的 `marketplace_plugin_publish`，改为 multipart 提取器
3. 集成 cmx-storage 上传逻辑

### 第 6 步：cmx-api 安装接口改造

1. 修改 `marketplace/handler.rs` 中的 `marketplace_plugin_install`
2. 优先使用 `storage_file_id` + `PluginSource::Storage`
3. 保留 `download_url` + `PluginSource::Remote` 作为降级方案

### 第 7 步：cmx-api 响应更新

1. 在 `marketplace/response.rs` 的 `MarketplaceVersionResponse` 中添加 `storage_file_id`
2. 更新 `convert_version_to_response` 映射函数

### 第 8 步：编译验证与测试

1. 执行 `rtk cargo check` 确保编译通过
2. 执行 `rtk cargo clippy` 检查代码质量
3. 手动测试发布流程（上传文件 + 存储验证）
4. 手动测试安装流程（下载 + 安装验证）

---

## 八、注意事项

### 8.1 依赖方向

cmx-plugin 依赖 cmx-storage（通过 `GlobalStorageService` 全局单例），这是合理的单向依赖。cmx-storage 不依赖 cmx-plugin。

### 8.2 兼容性

- 旧的版本记录（没有 `storage_file_id`）仍然可以通过 `download_url` + `PluginSource::Remote` 安装
- 新发布的插件将同时拥有 `storage_file_id` 和 `download_url`（由 cmx-storage 生成）

### 8.3 文件清理

当插件版本被删除时，应同步删除 cmx-storage 中的文件。可在 `delete_plugin` 流程中调用 `storage_service.delete(file_id)`。

### 8.4 大文件处理

对于大型插件包（>100MB），可考虑：

1. 前端使用 cmx-storage 的分片上传 API（`/api/storage/multipart/*`）
2. 分片上传完成后，在发布接口中传入 `storage_file_id` 而非文件数据

### 8.5 错误处理

遵循项目规范使用 `thiserror`，新增的错误类型：

```rust
// cmx-plugin/src/error.rs 中可能需要新增
#[error("存储服务操作失败: {0}")]
StorageError(String),
```

### 8.6 object_type 命名

在 cmx-storage 中上传插件包时，使用以下命名规范：

| object_type          | 说明                 |
|----------------------|--------------------|
| `marketplace_plugin` | 插件市场的插件包文件         |
| `marketplace_icon`   | 插件市场的图标文件（如需要单独上传） |

这样便于在 cmx-storage 中按类型查询和管理插件相关文件。
