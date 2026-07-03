# 模块资源导入导出统一抽象方案

> **文档日期**：2026-07-03
> **状态**：设计稿（待评审）
> **范围**：权限 / 表单 / 菜单 / 元数据（表结构）四类模块资源的导入导出抽象
> **关联**：`20260703_cmx-plugin_安装升级与模块导入导出代码复用评审报告.md`（已完成的第一轮改造）

---

## 一、背景与目标

### 1.1 问题陈述

当前模块资源（表单/菜单/元数据/权限）的导入导出存在三个核心割裂：

| # | 问题 | 现状 |
|---|------|------|
| 1 | **本地调用与远程调用割裂** | 模块导入（`module_install.rs`）直调本地 `FormService`/`MenuService`；而插件数据分发（`center_client`）走 gRPC/HTTP 推 ZIP。两套代码、两种数据形态（结构体 vs ZIP），无法透明切换。 |
| 2 | **`center_client` 整套死代码** | 9 个文件（dispatcher + 3 sender + config + packer + types）已实现，但全仓库**无任何构造/调用点**。`dispatch_install`/`dispatch_cleanup` 从未被调用，gRPC 插件数据导入链路实际未接入。 |
| 3 | **导入导出不对称** | 导入侧基本走 Service 层（form/menu 走 FormService/MenuService，permission 走 trait）；导出侧**全部是内联 SQL**（`module_export.rs` 直接 `query_sql_with_datavalues`），且元数据导入用私有 `save_table_metadata` 手写 SQL 绕开了 `TableMetadataService`。 |

### 1.2 设计目标

```
不管服务是「单体完整部署」还是「分体式微服务」，
模块导入导出的调用方代码完全一致。
```

具体而言：

1. **统一接口**：每类资源定义一个细粒度 `DefinitionImporter` trait（接收已解析的结构体列表，非 ZIP）。
2. **双实现透明**：每个 trait 有 `Local` 实现（直调本地 Service）和 `Remote` 实现（经 gRPC 远程调用专门中心），调用方代码不变。
3. **分体式兼容**：即使分体式部署「本地也包含了代码」，`Remote` 实现内部走 gRPC 调用专门服务，不直接调本地 Service。
4. **复用现有资产**：下层复用 `PluginDataImporter`（ZIP）+ `PluginDataClient`（gRPC）已建好的传输通道，不另起 proto。
5. **激活死代码**：把 `center_client` 从死代码改造为正式的「资源安装分发器」，接入模块导入流程。

### 1.3 关键约束

- `cmx-plugin` **不能直接依赖** `cmx-iam`（通过 `cmx-traits` trait 连接，已在第一轮改造确立）。
- 同理，`cmx-plugin` 不直接依赖 `cmx-form`、`cmx-model` 等业务 crate —— 一律经 trait 注入。
- proto 框架是 **volo-grpc + pilota**（非 tonic），proto 文件在 `cmx-rpc-gen/idl/`。

---

## 二、现状架构分析

### 2.1 center_client 模块（死代码）

```
cmx-plugin/src/center_client/
├── mod.rs          (36行)   模块导出
├── types.rs        (181行)  DataCategory / DispatchContext / DispatchResult / CenterSendRequest
├── config.rs       (162行)  CenterClientConfig (mode: mock|http_url|http_discovery|grpc)
├── sender.rs       (53行)   ServiceCenterSender trait (send_data / cleanup_data)
├── packer.rs       (57行)   pack_directory_to_zip (目录→ZIP 字节)
├── dispatcher.rs   (205行)  CenterDataDispatcher (按 DataCategory 并行分发 ZIP)
├── http_sender.rs  (338行)  HTTP form-data 实现（仅 Perm 路径完整）
├── grpc_sender.rs  (169行)  gRPC 实现（复用 cmx_rpc::plugin_data_client()）
└── mock_sender.rs  (48行)   Mock 实现
```

**致命问题**：全仓库搜索 `CenterDataDispatcher::new` / `dispatch_install` / `dispatch_cleanup`，在 `center_client/` 目录之外**零命中**。这套机制从未被接入插件安装/卸载流程。`lib.rs:28` 仅有 `pub mod center_client;` 声明，无调用。

**设计局限**：
- 单向 fire-and-forget push，无查询/回调语义。
- 数据载体是 ZIP 字节流（`zip_data: Vec<u8>`），非结构化 RPC 参数。
- `CenterResponse` 只保留 `success/message`，丢弃了 `created_count/updated_count` 统计。
- 单 sender 多目标耦合：4 类资源共享一个 sender，靠 category 内部路由。

### 2.2 现有传输抽象（可复用资产）

**两层已建好的传输基础设施**：

```
┌────────────────────────── 调用方 ──────────────────────────┐
│  center_client::CenterDataDispatcher.dispatch_install(ctx) │
└────────────────────────────┬───────────────────────────────┘
                             │ (ZIP 字节)
                  ┌──────────▼──────────┐
                  │ ServiceCenterSender │ trait (cmx-plugin)
                  │  send_data(req)     │
                  └──────────┬──────────┘
            ┌────────────────┼────────────────┐
            │                │                │
     ┌──────▼──────┐  ┌──────▼──────┐  ┌──────▼──────┐
     │ HttpSender  │  │ GrpcSender  │  │ MockSender  │
     │ (form-data) │  │             │  │             │
     └─────────────┘  └──────┬──────┘  └─────────────┘
                             │
                  ┌──────────▼──────────────────────┐
                  │ PluginDataClient (cmx-traits)    │ trait
                  │  import_plugin_data(svc, req)    │
                  └──────────┬───────────────────────┘
                             │ gRPC (volo)
                  ┌──────────▼──────────────────────┐
                  │ CmxPluginDataServerImpl           │
                  │  → PluginDataImporter.import_data │
                  └──────────┬───────────────────────┘
                             │
                  ┌──────────▼──────────────────────┐
                  │ PluginDataImporterImpl (cmx-iam)  │ trait
                  │  按 category 路由 → 各 Service    │
                  └──────────────────────────────────┘
```

**关键发现**：接收端已有统一抽象 `PluginDataImporter` trait（`cmx-traits/src/plugin/data_importer.rs`），gRPC server 和 HTTP handler **都汇聚到它**。但当前 `PluginDataImporterImpl` **仅实现 Perm 类别**，Menu/Form/Flow 返回「不支持」。

### 2.3 四类资源导入导出现状对照

| 维度 | Form | Menu | Metadata | Permission |
|---|---|---|---|---|
| **导入是否经服务层** | ✅ FormService | ✅ MenuService | ⚠️ 部分（建表经 Executor，登记经私有 SQL） | ✅ trait 委托 |
| **导入幂等策略** | delete_by_code+create | delete_by_code+create | create_or_upgrade+upsert | ON CONFLICT(code) |
| **导出是否经服务层** | ❌ 内联 SQL | ❌ 内联 SQL | ❌ 内联 JOIN SQL | ❌ 内联 SQL |
| **数据形态** | definition JSONB 透传 | definition JSONB 透传 | 结构化 TableDefine | 结构化 PermissionDefinition |
| **是否有 trait 抽象** | ❌ 无 | ❌ 无 | ❌ 无 | ✅ PermissionDefinitionImporter（已建） |
| **特殊逻辑** | 无 | create 计算树形字段 | 建表(DDL)+双库登记 | 两阶段 upsert+parent 回填 |

**结论**：Permission 是最成熟的范式（已有独立 trait + cmx-iam 实现 + Builder 注入）。其余三类应参照此范式补齐。

---

## 三、双层架构设计

### 3.1 架构总览

```
╔══════════════════════════════════════════════════════════════════╗
║                    上层：结构化 DefinitionImporter                  ║
║              (细粒度 trait，本地/远程统一，调用方一致)                ║
╠══════════════════════════════════════════════════════════════════╣
║  FormDefinitionImporter   MenuDefinitionImporter                   ║
║  TableDefinitionImporter  PermissionDefinitionImporter(已存在)      ║
╚═══════════╤═════════════════════════════╤═════════════════════════╝
            │ Local 实现                   │ Remote 实现
            │ (直调 Service)                │ (序列化→ZIP→gRPC)
   ┌────────▼────────┐           ┌─────────▼─────────┐
   │ LocalForm...    │           │ RemoteForm...      │
   │ → FormService   │           │ → PluginDataClient │
   │ → MenuService   │           │   (ZIP 传输)        │
   │ → TableMeta...  │           │ → 远程 PluginData   │
   │ → PermissionSvc │           │   Importer 解压     │
   └─────────────────┘           │   → Local 实现      │
                                 └─────────────────────┘
╔══════════════════════════════════════════════════════════════════╗
║                  下层：PluginDataImporter (ZIP 传输)                ║
║          (粗粒度，gRPC/HTTP 通道，Remote 实现的底层依赖)             ║
╚══════════════════════════════════════════════════════════════════╝
```

### 3.2 分层职责

| 层级 | 职责 | 数据形态 | 部署形态 |
|------|------|---------|---------|
| **上层 DefinitionImporter** | 业务语义（应用结构体列表到作用域） | 结构化 DTO（`&[FormDefinition]` 等） | 本地直调或远程透明 |
| **下层 PluginDataImporter** | 传输语义（ZIP 批量投递） | ZIP 字节流 | 仅远程（gRPC/HTTP） |

### 3.3 本地/远程透明切换原理

调用方（`ModuleInstallService`）只持有 `Arc<dyn FormDefinitionImporter>` 等上层 trait 对象：

- **单体部署**（mode=local）：注入 `LocalFormDefinitionImporter`（持 `DatabaseManager`，直调 `FormService`）。
- **分体式部署**（mode=grpc）：注入 `RemoteFormDefinitionImporter`（持 `PluginDataClient`，把结构体序列化为 JSON→打包 ZIP→gRPC 调远程中心→远程解压→调远程的 Local 实现）。

**调用方代码完全一致**：
```rust
// module_install.rs 中，无论是本地还是远程，这段代码不变
form_importer.apply_form_definitions(domain, app, module, &forms).await?;
menu_importer.apply_menu_definitions(domain, app, module, &menus).await?;
table_importer.apply_table_definitions(domain, app, module, &tables, biz_db_id).await?;
perm_importer.apply_permission_definitions(domain, app, module, &perms).await?;
```

### 3.4 Remote 实现的「结构体→ZIP」转换

Remote 实现复用下层 ZIP 通道，避免为每类资源新增 proto message：

```
RemoteFormDefinitionImporter.apply_form_definitions(domain, app, module, forms):
  1. 把 forms: &[FormDefinition] 序列化为 JSON 文件(form_0.json, form_1.json...)
  2. 打包成 ZIP 字节流 (复用 packer::pack_to_zip)
  3. 构造 PluginDataImportRequest { category: Form, zip_data, ... }
  4. plugin_data_client().import_plugin_data(service_name, req)
  5. 远程 CmxPluginDataServerImpl → PluginDataImporterImpl.import_data
  6. 远程解压 ZIP → 解析 JSON → LocalFormDefinitionImporter.apply (直调 FormService)
```

**优势**：proto 不变（复用 `cmx_plugin_data.proto` 的 `ImportPluginData`），仅扩展 `PluginDataImporterImpl` 支持 Form/Menu/Metadata 类别。

---

## 四、统一 Trait 定义

### 4.1 契约结构体（定义在 cmx-core）

为每类资源定义「导入契约」结构体（对齐导出/导入对称性），与已有的 `PermissionDefinition` 并列：

```rust
// crates/libs/cmx-core/src/model/module/definitions.rs (新增)

use serde::{Deserialize, Serialize};

/// 表单定义（模块包 forms/*.json 的单条契约）。
/// 整体 definition JSON 透传，与 FormForCreate 对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormDefinition {
    pub code: String,              // {module}:{stem}
    pub name: String,
    pub description: Option<String>,
    pub definition: serde_json::Value,  // 整体 JSON
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 菜单定义（模块包 menus/*.json 的单条契约，根菜单）。
/// definition 含完整菜单树，导入时整体透传。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuDefinition {
    pub code: String,
    pub name: String,
    pub definition: serde_json::Value,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 表结构定义契约（模块包 metadata/tables/*.json 的单表）。
/// 复用 cmx-core 已有的 TableDefine，不新建。
// 注：apply 时需额外传 biz_db_id（建表目标库），见 TableDefinitionImporter 签名

// PermissionDefinition 已存在于 cmx_core::model::iam，无需新建
```

### 4.2 四个 DefinitionImporter Trait（定义在 cmx-traits）

参照已建立的 `PermissionDefinitionImporter` 范式：

```rust
// crates/libs/cmx-traits/src/module/mod.rs (新增模块)

// ---- FormDefinitionImporter ----
#[async_trait]
pub trait FormDefinitionImporter: Send + Sync {
    /// 将表单定义列表 upsert 到指定作用域（幂等：先删同 code 再建）。
    async fn apply_form_definitions(
        &self,
        domain_code: &str, app_code: &str, module_code: &str,
        definitions: &[FormDefinition],
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有表单定义（对称：供 module_export 复用，消除内联 SQL）。
    async fn list_form_definitions(
        &self, module_code: &str,
    ) -> Result<Vec<FormDefinition>, TraitError>;
}

// ---- MenuDefinitionImporter ----
#[async_trait]
pub trait MenuDefinitionImporter: Send + Sync {
    /// 将根菜单定义列表安装到指定作用域（每个 definition 含完整菜单树）。
    async fn apply_menu_definitions(
        &self,
        domain_code: &str, app_code: &str, module_code: &str,
        definitions: &[MenuDefinition],
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有根菜单定义。
    async fn list_menu_definitions(
        &self, module_code: &str,
    ) -> Result<Vec<MenuDefinition>, TraitError>;
}

// ---- TableDefinitionImporter ----
#[async_trait]
pub trait TableDefinitionImporter: Send + Sync {
    /// 将表结构定义建表到业务库 + 登记元数据。
    /// biz_db_id: 建表目标库；元数据登记库由实现内部决定（default 库）。
    async fn apply_table_definitions(
        &self,
        domain_code: &str, app_code: &str, module_code: &str, app_id: &str,
        definitions: &[TableDefine],
        biz_db_id: &str,
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有表结构定义（连查 cmx_meta_table_define + version）。
    async fn list_table_definitions(
        &self, app_code: &str, module_code: &str,
    ) -> Result<Vec<TableDefine>, TraitError>;
}

// ---- PermissionDefinitionImporter (已存在，补充 list 方法) ----
// 现有 trait 仅有 apply_permission_definitions，需补充导出方法：
#[async_trait]
pub trait PermissionDefinitionImporter: Send + Sync {
    async fn apply_permission_definitions(
        &self, domain_code: &str, app_code: &str, module_code: &str,
        definitions: &[PermissionDefinition],
    ) -> Result<usize, TraitError>;

    /// 【新增】导出指定模块的所有权限定义（重建 parent_code）。
    async fn list_permission_definitions(
        &self, domain_code: &str, app_code: &str, module_code: &str,
    ) -> Result<Vec<PermissionDefinition>, TraitError>;
}
```

### 4.3 设计要点

1. **apply + list 对称**：每个 trait 同时有「导入」和「导出」方法，消除 `module_export.rs` 的内联 SQL（导出也走 trait）。
2. **作用域参数统一**：`(domain_code, app_code, module_code)` 三元组，与 DB 表的过滤条件一致。
3. **TableDefinitionImporter 特殊**：多一个 `biz_db_id` 参数（建表目标库），因元数据有双库语义。
4. **不引入「统一 ResourceImporter」泛型 trait**：四类资源的 DTO 类型和参数不同，强行泛型化（如 `apply<T>`）会牺牲类型安全，保持独立 trait 更清晰。

---

## 五、本地实现（LocalXxxImporter）

本地实现直调各 Service 层，**无 ZIP 开销**，是单体部署的默认实现。

### 5.1 实现位置与依赖

| Trait | Local 实现位置 | 直调的 Service | 所在 crate |
|-------|--------------|---------------|-----------|
| FormDefinitionImporter | `cmx-biz/src/form/definition_importer.rs` | FormService | cmx-biz |
| MenuDefinitionImporter | `cmx-biz/src/menu/definition_importer.rs` | MenuService | cmx-biz |
| TableDefinitionImporter | `cmx-plugin/.../table_definition_importer.rs` | PgTableDefineExecutor + TableMetadataService | cmx-plugin |
| PermissionDefinitionImporter | `cmx-iam/.../definition_importer.rs`（已存在） | PermissionServiceImpl | cmx-iam |

### 5.2 LocalFormDefinitionImporter 伪代码

```rust
// crates/libs/cmx-biz/src/form/definition_importer.rs
pub struct LocalFormDefinitionImporter {
    mm: Arc<DatabaseManager>,
    db_id: String,  // default 库
}

#[async_trait]
impl FormDefinitionImporter for LocalFormDefinitionImporter {
    async fn apply_form_definitions(&self, domain, app, module, defs) -> Result<usize> {
        let mut count = 0;
        for def in defs {
            // 幂等：先删同 code 再建
            let _ = FormService::delete_by_code(&self.mm, &self.db_id, &def.code).await;
            FormService::create(&self.mm, &self.db_id, FormForCreate {
                code: def.code.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                definition: Some(def.definition.clone()),
                domain_code: def.domain_code.clone(),
                application_code: def.application_code.clone(),
                module_code: def.module_code.clone(),
            }).await?;
            count += 1;
        }
        Ok(count)
    }

    async fn list_form_definitions(&self, module_code) -> Result<Vec<FormDefinition>> {
        // 封装原 module_export 的内联 SQL：SELECT code, definition FROM cmx_form WHERE module_code=$1
        // 返回结构化 FormDefinition
        FormService::list_by_module(&self.mm, &self.db_id, module_code).await
    }
}
```

### 5.3 LocalTableDefinitionImporter（收敛 save_table_metadata）

这是最关键的收敛点 —— 把 `module_install.rs` 私有的 `save_table_metadata` 手写 SQL **下沉到 TableMetadataService**，消除双路径：

```rust
// crates/libs/cmx-plugin/.../table_definition_importer.rs
pub struct LocalTableDefinitionImporter {
    mm: Arc<DatabaseManager>,
    default_db_id: String,  // 元数据登记库
}

#[async_trait]
impl TableDefinitionImporter for LocalTableDefinitionImporter {
    async fn apply_table_definitions(&self, domain, app, module, app_id, defs, biz_db_id) -> Result<usize> {
        let executor = PgTableDefineExecutor::new(biz_db_id, None);
        for td in defs {
            executor.create_or_upgrade_table(td).await?;  // 建表到业务库
            // 登记元数据（复用 TableMetadataService，消除 save_table_metadata 私有 SQL）
            TableMetadataService::upsert_by_table_name(
                &self.mm, &self.default_db_id, td, domain, app, module, app_id, biz_db_id,
            ).await?;
        }
        Ok(defs.len())
    }
}
```

**注**：需在 `TableMetadataService` 新增 `upsert_by_table_name` 方法（封装现有「先查 table_name 存在性 → UPDATE 或 INSERT」逻辑），替代 `module_install::save_table_metadata`。

### 5.4 本地实现的装配

各 Local 实现构造时需要 `DatabaseManager` + `db_id`，在 web-server 启动时构造：

```rust
// web-server/src/config/module_resources.rs (新增装配文件)
pub fn build_local_definition_importers(mm, default_db_id, biz_db_id, perm_svc)
    -> DefinitionImporterBundle {
    DefinitionImporterBundle {
        form: Arc::new(LocalFormDefinitionImporter::new(mm.clone(), default_db_id.clone())),
        menu: Arc::new(LocalMenuDefinitionImporter::new(mm.clone(), default_db_id.clone())),
        table: Arc::new(LocalTableDefinitionImporter::new(mm.clone(), default_db_id.clone())),
        permission: perm_svc.clone(),  // PermissionServiceImpl 已实现 trait
    }
}
```

---

## 六、远程实现（RemoteXxxImporter）

远程实现经 gRPC 调用专门中心，**适用于分体式微服务部署**。

### 6.1 Remote 实现的位置与依赖

所有 Remote 实现集中在 `cmx-plugin`（因为它依赖 `cmx-rpc` 的 `PluginDataClient`）：

```
crates/libs/cmx-plugin/src/service/remote_importers/
├── mod.rs
├── form.rs       RemoteFormDefinitionImporter
├── menu.rs       RemoteMenuDefinitionImporter
├── table.rs      RemoteTableDefinitionImporter
└── permission.rs RemotePermissionDefinitionImporter
```

### 6.2 RemoteFormDefinitionImporter 伪代码

```rust
// crates/libs/cmx-plugin/src/service/remote_importers/form.rs
pub struct RemoteFormDefinitionImporter {
    plugin_data_client: Arc<dyn PluginDataClient>,  // cmx-rpc 全局
    config: CenterClientConfig,                      // 服务名解析
}

#[async_trait]
impl FormDefinitionImporter for RemoteFormDefinitionImporter {
    async fn apply_form_definitions(&self, domain, app, module, defs) -> Result<usize> {
        // 1. 结构体 → JSON 文件 → ZIP（复用 packer）
        let zip = pack_definitions_to_zip(defs, "form")?;  // form_0.json, form_1.json...

        // 2. 解析目标服务名（如 "cmx-form-center"）
        let service_name = self.config.discovery.get_service_name(DataCategory::Form)?;

        // 3. gRPC 调用（复用 PluginDataClient）
        let req = PluginDataImportRequest {
            category: PluginDataCategory::Form,
            domain_code: domain.into(), application_code: app.into(),
            module_code: module.into(), zip_data: zip,
            plugin_id: String::new(), app_id: String::new(), version: String::new(),
        };
        let result = self.plugin_data_client.import_plugin_data(service_name, req).await?;

        Ok(result.created_count as usize + result.updated_count as usize)
    }

    async fn list_form_definitions(&self, module_code) -> Result<Vec<FormDefinition>> {
        // 远程导出：可新增 gRPC ListPluginData 方法，或暂不支持（导出一般在管理端本地）
        Err(TraitError::Business("远程导出暂不支持，请在管理端执行"))
    }
}
```

### 6.3 远程接收端的扩展

当前 `PluginDataImporterImpl`（cmx-iam）**仅支持 Perm**。需扩展为支持四类（或拆为多个 Impl 按需注入）：

```rust
// 方案A：单一 PluginDataImporterImpl 路由四类（推荐，保持现有结构）
impl PluginDataImporter for PluginDataImporterImpl {
    async fn import_data(&self, req) -> Result<PluginDataImportResult> {
        match req.category {
            PluginDataCategory::Perm => self.import_permissions(...),
            PluginDataCategory::Form => self.form_importer.apply_form_definitions(...),  // 新增
            PluginDataCategory::Menu => self.menu_importer.apply_menu_definitions(...),  // 新增
            // Flow 暂不支持
        }
    }
}
```

**关键**：远程接收端（中心服务）内部调用的还是 `LocalFormDefinitionImporter` 等 —— 即「Remote 发送 → gRPC → 远程 PluginDataImporter → 远程 Local 实现」。**远程中心和本地单体用的是同一套 Local 实现**，只是部署位置不同。

### 6.4 ZIP 转换的统一 helper

新增 `packer::pack_definitions_to_zip<T: Serialize>(defs: &[T], prefix: &str)` —— 把结构体列表序列化为 `{prefix}_0.json` 等文件再打 ZIP。四类资源共用，消除重复。

---

## 七、center_client 改造（激活死代码）

### 7.1 改造目标

把 `center_client` 从「死代码」改造为「模块资源安装分发器」，接入 `ModuleInstallService`。

### 7.2 dispatcher 改造

**当前**：`CenterDataDispatcher` 持单个 `ServiceCenterSender`，按 `DataCategory` 推 ZIP（面向插件安装目录的 `permdata/`、`menudata/` 等子目录）。

**改造后**：dispatcher 不再直接面向 ZIP 目录，而是**面向上层 DefinitionImporter**。两种可选形态：

**形态A（推荐）：dispatcher 退化为「ImporterBundle 持有者」**

`CenterDataDispatcher` 改名为 `ModuleResourceDispatcher`，持有一个四元组 importer bundle：

```rust
pub struct DefinitionImporterBundle {
    pub form: Arc<dyn FormDefinitionImporter>,
    pub menu: Arc<dyn MenuDefinitionImporter>,
    pub table: Arc<dyn TableDefinitionImporter>,
    pub permission: Arc<dyn PermissionDefinitionImporter>,
}

pub struct ModuleResourceDispatcher {
    bundle: DefinitionImporterBundle,
}

impl ModuleResourceDispatcher {
    pub async fn install_all(&self, domain, app, module, app_id, module_dir, biz_db_id) {
        // 读 forms/*.json → bundle.form.apply_form_definitions(...)
        // 读 menus/*.json → bundle.menu.apply_menu_definitions(...)
        // 读 metadata/*.json → bundle.table.apply_table_definitions(...)
        // 读 permissions/*.json → bundle.permission.apply_permission_definitions(...)
    }
}
```

`bundle` 里的 importer 是 Local 还是 Remote，由配置决定 —— **dispatcher 代码完全不变**。

**形态B（保留 ZIP 推送）：仅新增 LocalServiceCenterSender**

保留现有 dispatcher + sender 架构，新增 `LocalServiceCenterSender`（持 `PluginDataImporter` 直调，跳过网络）。mode 增加 `local`。

→ **不推荐**：形态B 仍要求本地模式打 ZIP（无谓序列化开销），且 dispatcher 的 ZIP 目录模型与上层结构化导入语义不匹配。

### 7.3 配置扩展

```toml
# config/config_template.toml [center_client] 节扩展
[center_client]
mode = "local"   # local(默认,单体) | grpc(分体式)

# mode=local 时以下 discovery 配置忽略
# mode=grpc 时按 category 解析远程服务名
[center_client.discovery]
form_service = "cmx-form-center"
menu_service = "cmx-portal-center"
perm_service = "cmx-iam-center"
flow_service = "cmx-flow-center"
```

### 7.4 ServiceCenterSender 的去留

- **形态A**：`ServiceCenterSender` trait + 3 实现（Http/Grpc/Mock）**不再需要**（被上层 Remote importer 取代）。可删除或标记 deprecated。
- **形态B**：保留，仅新增 Local 实现。

**建议形态A**：上层 Remote importer 已封装了 gRPC 调用（直接用 `PluginDataClient`），无需 center_client 的 sender 中间层。center_client 的 `packer.rs`（ZIP 工具）可保留供 Remote importer 复用，其余 sender/dispatcher 代码删除。

---

## 八、proto 与 gRPC 策略

### 8.1 proto 不变（复用）

`cmx_plugin_data.proto` 的 `ImportPluginData` 已支持 category + zip_data，**无需新增 message**：

```proto
service CmxPluginDataService {
  rpc ImportPluginData(ImportPluginDataRequest) returns (ImportPluginDataResponse);
  rpc CleanupPluginData(CleanupPluginDataRequest) returns (ImportPluginDataResponse);
}
// category 字段区分 form/menu/perm/flow，zip_data 传结构化 JSON 打成的 ZIP
```

### 8.2 导出的远程支持（可选扩展）

当前 `list_xxx_definitions`（导出）一般只在管理端本地执行。若需远程导出（如运维平台从各中心拉取），可后续新增：

```proto
rpc ListPluginData(ListPluginDataRequest) returns (ListPluginDataResponse);
message ListPluginDataRequest {
  string category = 1;
  string module_code = 2;
  // ...
}
message ListPluginDataResponse {
  bool success = 1;
  bytes json_data = 2;  // 序列化的定义列表 JSON
}
```

**本期不做**，导出默认本地执行。

### 8.3 PluginDataClient 复用

Remote importer 直接用 `cmx_rpc::plugin_data_client()`（全局单例），享受其服务发现/负载均衡/超时配置。无需新建 RPC client。

---

## 九、装配与注入

### 9.1 装配架构图

```
web-server 启动
    │
    ├─ mode == "local"（单体）
    │     └─ build_local_importers(mm, db_id)
    │         → DefinitionImporterBundle { Local×4 }
    │
    └─ mode == "grpc"（分体式）
          └─ build_remote_importers(plugin_data_client, config)
              → DefinitionImporterBundle { Remote×4 }

    │
    ▼
CmxAppState.definition_importers: Option<Arc<DefinitionImporterBundle>>
    │
    ▼
package_handler.module_package_import()
    └─ ModuleInstallService::new(...)
       .with_definition_importers(state.definition_importers.clone())
    │
    ▼
ModuleInstallService.install_module_resources()
    └─ dispatcher.install_all(bundle, domain, app, module, module_dir)
        ├─ bundle.form.apply_form_definitions(...)       # Local 或 Remote，代码一致
        ├─ bundle.menu.apply_menu_definitions(...)       # Local 或 Remote
        ├─ bundle.table.apply_table_definitions(...)     # Local 或 Remote
        └─ bundle.permission.apply_permission_definitions(...)  # Local 或 Remote
```

### 9.2 CmxAppState 扩展

```rust
// crates/libs/cmx-api/src/app_state.rs
pub struct CmxAppState {
    // ... 现有字段
    /// 模块资源定义导入器集合（本地或远程，由部署模式决定）
    pub definition_importers: Option<Arc<DefinitionImporterBundle>>,
}

impl CmxAppState {
    pub fn with_definition_importers(mut self, b: Arc<DefinitionImporterBundle>) -> Self {
        self.definition_importers = Some(b);
        self
    }
}
```

### 9.3 ModuleInstallService 改造

```rust
pub struct ModuleInstallService {
    package_utils: PackageUtils,
    deploy_service: Arc<DeployService>,
    /// 四类资源导入器（替代原有 permission_importer 单字段）
    importers: Option<Arc<DefinitionImporterBundle>>,
}

impl ModuleInstallService {
    pub fn with_definition_importers(mut self, b: Arc<DefinitionImporterBundle>) -> Self {
        self.importers = Some(b);
        self
    }

    async fn install_module_resources(&self, ...) {
        let Some(bundle) = &self.importers else {
            warn!("未注入 DefinitionImporterBundle,跳过资源安装");
            return;
        };
        // 统一调用，本地/远程透明
        Self::install_forms_via(bundle.form, ...).await;
        Self::install_menus_via(bundle.menu, ...).await;
        Self::install_metadata_via(bundle.table, ...).await;
        Self::install_permissions_via(bundle.permission, ...).await;
    }
}
```

---

## 十、导出对称化

### 10.1 消除 module_export 内联 SQL

`module_export.rs` 的四个 `export_xxx` 方法当前用内联 SQL，改为调用 trait 的 `list_xxx_definitions`：

```rust
// 改造前（module_export.rs::export_forms）
let sql = "SELECT code, definition FROM cmx_form WHERE module_code = $1 AND archived = 0";
let ds = mm.query_sql_with_datavalues(db_id, None, sql, params, "export_forms").await?;
// 手动解析 rows...

// 改造后
let defs = form_importer.list_form_definitions(module_code).await?;
// defs 已是 Vec<FormDefinition>，直接写 JSON 文件
```

### 10.2 导出也需要 importer 注入

`ModuleExportService` 同样注入 `DefinitionImporterBundle`，但导出只用 `list_*` 方法（本地模式）。导出一般在管理端执行，故导出侧固定用 Local importer。

### 10.3 收敛点汇总

| 资源 | 导入收敛 | 导出收敛 |
|------|---------|---------|
| Form | `LocalFormDefinitionImporter.apply` → FormService | `list_form_definitions` → FormService::list_by_module（新增） |
| Menu | `LocalMenuDefinitionImporter.apply` → MenuService | `list_menu_definitions` → MenuService::list_root_by_module（新增） |
| Metadata | `LocalTableDefinitionImporter.apply` → Executor + TableMetadataService::upsert_by_table_name（新增） | `list_table_definitions` → TableMetadataService::list_by_module（新增） |
| Permission | `PermissionDefinitionImporter.apply`（已存在） | `list_permission_definitions`（trait 新增方法） |

---

## 十一、分步实施路线图

### P0：本地四 trait 体系（高优先，消除导入不对称）

| 步骤 | 内容 | 工作量 |
|------|------|-------|
| P0.1 | cmx-core 新增 `FormDefinition` / `MenuDefinition` 契约结构体 | 小 |
| P0.2 | cmx-traits 新增 `FormDefinitionImporter` / `MenuDefinitionImporter` / `TableDefinitionImporter` trait + `PermissionDefinitionImporter` 补 list 方法 | 中 |
| P0.3 | cmx-biz 实现 `LocalFormDefinitionImporter` / `LocalMenuDefinitionImporter` + FormService/MenuService 新增 `list_by_module` | 中 |
| P0.4 | cmx-plugin 实现 `LocalTableDefinitionImporter` + TableMetadataService 新增 `upsert_by_table_name`（收敛 save_table_metadata） | 中 |
| P0.5 | cmx-iam `PermissionDefinitionImporter` 补 `list_permission_definitions` 实现 | 小 |
| P0.6 | `ModuleInstallService` 用 `DefinitionImporterBundle` 替代单 `permission_importer` 字段 | 中 |
| P0.7 | web-server 装配 Local bundle 注入 CmxAppState | 中 |

**收益**：导入侧四类资源完全对称，消除 `save_table_metadata` 私有 SQL 和导出内联 SQL。

### P1：远程实现（支持分体式部署）

| 步骤 | 内容 | 工作量 |
|------|------|-------|
| P1.1 | cmx-plugin 新增 `service/remote_importers/` 四个 RemoteXxxImporter | 中 |
| P1.2 | 新增 `packer::pack_definitions_to_zip<T: Serialize>` 统一 ZIP 转换 helper | 小 |
| P1.3 | cmx-iam `PluginDataImporterImpl` 扩展支持 Form/Menu/Metadata 类别（内部调 Local） | 中 |
| P1.4 | web-server 新增 mode=grpc 分支，装配 Remote bundle | 中 |
| P1.5 | 配置扩展（config_template.toml + CONFIG_MANUAL.md） | 小 |

**收益**：分体式部署时，模块导入经 gRPC 远程调用专门中心，调用方代码与单体一致。

### P2：center_client 清理与激活

| 步骤 | 内容 | 工作量 |
|------|------|-------|
| P2.1 | 删除 `center_client/` 的 sender.rs / http_sender.rs / grpc_sender.rs / mock_sender.rs / dispatcher.rs（被上层 Remote importer 取代） | 小 |
| P2.2 | 保留 `packer.rs`（供 Remote importer 复用）、`config.rs`（服务名解析）、`types.rs`（DataCategory） | — |
| P2.3 | 或保留 dispatcher 作「批量安装入口」（形态A 的 ModuleResourceDispatcher） | 中 |

**收益**：清除 ~800 行死代码，center_client 定位清晰（配置 + ZIP 工具 + DataCategory 枚举）。

### P3：导出对称化（可选，低优先）

| 步骤 | 内容 | 工作量 |
|------|------|-------|
| P3.1 | `module_export.rs` 四个 export_xxx 改为调 trait 的 list 方法 | 中 |
| P3.2 | `ModuleExportService` 注入 Local bundle | 小 |

**收益**：导出与导入完全对称，统一经 trait。

---

## 十二、影响文件清单与风险评估

### 12.1 新增文件

| 文件 | 用途 |
|------|------|
| `crates/libs/cmx-core/src/model/module/definitions.rs` | FormDefinition / MenuDefinition 契约 |
| `crates/libs/cmx-traits/src/module/mod.rs` + 子文件 | 四个 DefinitionImporter trait |
| `crates/libs/cmx-biz/src/form/definition_importer.rs` | LocalFormDefinitionImporter |
| `crates/libs/cmx-biz/src/menu/definition_importer.rs` | LocalMenuDefinitionImporter |
| `crates/libs/cmx-plugin/src/service/table_definition_importer.rs` | LocalTableDefinitionImporter |
| `crates/libs/cmx-plugin/src/service/remote_importers/*.rs` | 四个 RemoteXxxImporter |
| `crates/web/web-server/src/config/module_resources.rs` | bundle 装配 |

### 12.2 修改文件

| 文件 | 改动 |
|------|------|
| `cmx-biz/src/form/service.rs` | 新增 `list_by_module` |
| `cmx-biz/src/menu/service.rs` | 新增 `list_root_by_module` |
| `cmx-plugin/.../table_metadata/service.rs` | 新增 `upsert_by_table_name` / `list_by_module` |
| `cmx-iam/.../definition_importer.rs` | 补 `list_permission_definitions` |
| `cmx-iam/.../import_handler.rs` | PluginDataImporterImpl 扩展 Form/Menu 类别 |
| `cmx-plugin/src/service/module_install.rs` | 用 bundle 替代单 importer，install_module_resources 改调 trait |
| `cmx-plugin/src/service/module_export.rs` | export_xxx 改调 trait list 方法 |
| `cmx-api/src/app_state.rs` | 新增 definition_importers 字段 |
| `cmx-api/src/handlers/module/package_handler.rs` | 注入 bundle |
| `web-server/src/config/iam.rs` + `main.rs` | 装配 Local/Remote bundle |

### 12.3 删除文件（P2）

| 文件 | 原因 |
|------|------|
| `cmx-plugin/src/center_client/sender.rs` | 被 Remote importer 取代 |
| `cmx-plugin/src/center_client/http_sender.rs` | 同上 |
| `cmx-plugin/src/center_client/grpc_sender.rs` | 同上 |
| `cmx-plugin/src/center_client/mock_sender.rs` | 同上 |
| `cmx-plugin/src/center_client/dispatcher.rs` | 被 ModuleResourceDispatcher 取代 |

### 12.4 风险评估

| 风险 | 等级 | 缓解 |
|------|:---:|------|
| Remote 实现 ZIP 序列化开销（4MB 限制） | 🟡 中 | 大模块分包；或后续 proto 改 streaming |
| TableMetadataService.upsert_by_table_name 与 save_table_metadata 语义不一致 | 🟡 中 | P0.4 仔细对齐「先查存在 → UPDATE/INSERT」逻辑，加测试 |
| MenuService.create 树形字段计算依赖 JSON 解析（脆弱） | 🟡 中 | 本期保持现状，后续单独重构 compute_tree_fields |
| 分体式部署时 PluginDataImporterImpl 需注入 Form/Menu Local importer（循环依赖？） | 🟢 低 | cmx-iam 已依赖 cmx-biz，无循环 |
| 导出远程支持缺失 | 🟢 低 | 本期导出固定本地，远程导出留后续 |

---

## 十三、核心设计决策小结

1. **双层而非单层**：上层结构化 DefinitionImporter（业务语义）+ 下层 ZIP PluginDataImporter（传输语义）。上层对调用方，下层对传输。避免「本地也打 ZIP」的浪费。
2. **trait 不泛型化**：四类资源各自独立 trait（FormDefinitionImporter 等），而非统一 `ResourceImporter<T>`。牺牲一点重复，换取类型安全和参数清晰（Table 多 biz_db_id）。
3. **apply + list 对称**：每个 trait 同时含导入和导出方法，消除 module_export 内联 SQL。
4. **proto 不动**：复用 `ImportPluginData(category, zip_data)`，远程实现把结构体序列化为 JSON 打 ZIP 传输。
5. **center_client 大部分删除**：sender/dispatcher 被 Remote importer 取代，仅保留 packer/config/DataCategory。
6. **配置驱动切换**：`mode = local | grpc`，装配时选 Local 或 Remote bundle，调用方代码完全一致。
7. **PermissionDefinitionImporter 扩展而非新建**：已有 trait 补 list 方法，保持连续性。

---

## 附录 A：与第一轮改造的衔接

第一轮改造（`20260703_..._代码复用评审报告`）已建立：
- `PermissionDefinitionImporter` trait（cmx-traits）
- `PermissionDefinition` / `PermissionFile` 收敛到 cmx-core
- `ModuleInstallService::with_permission_importer` Builder 注入

本方案是第一轮的**延伸与泛化**：把「仅权限」的 trait 注入模式推广到「表单/菜单/元数据」全部四类，并把单字段 `permission_importer` 升级为 `DefinitionImporterBundle` 四元组。第一轮的 `PermissionDefinitionImporter` trait 保留并扩展（补 list 方法），不推翻重做。
