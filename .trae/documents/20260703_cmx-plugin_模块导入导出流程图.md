# 模块导入导出流程图

> 用 Mermaid 流程图描述 cmx-container 模块导入/导出的完整代码路径，便于快速理解架构。
>
> **更新日期**：2026-07-03（`PluginDataImporterImpl` 从 cmx-iam 迁移至 cmx-biz，新增 `PermissionZipImporter` trait 解耦）

---

## 一、整体架构（部署模式与数据流）

```mermaid
graph TB
    subgraph 调用方["调用方（代码完全一致）"]
        MIS["ModuleInstallService<br/>install_module_package"]
        MES["ModuleExportService<br/>export_module"]
    end

    subgraph Bundle["DefinitionImporterBundle（四元组）"]
        FI["form:<br/>FormDefinitionImporter"]
        MI["menu:<br/>MenuDefinitionImporter"]
        TI["table:<br/>TableDefinitionImporter"]
        PI["permission:<br/>PermissionDefinitionImporter"]
    end

    MIS --> Bundle
    MES --> Bundle

    subgraph 本地模式["本地模式 mode=local（默认）"]
        LF["LocalFormDefinitionImporter<br/>→ FormService"]
        LM["LocalMenuDefinitionImporter<br/>→ MenuService"]
        LT["LocalTableDefinitionImporter<br/>→ PgTableDefineExecutor<br/>+ TableMetadataService"]
        LP["PermissionServiceImpl<br/>→ 两阶段 upsert cmx_permission"]
    end

    subgraph 远程模式["远程模式 mode=grpc/http_url/http_discovery"]
        RF["RemoteFormDefinitionImporter"]
        RM["RemoteMenuDefinitionImporter"]
        RT["RemoteTableDefinitionImporter"]
        RP["RemotePermissionDefinitionImporter"]
        CTX["RemoteImporterContext.send()"]
    end

    FI -->|mode=local| LF
    MI -->|mode=local| LM
    TI -->|mode=local| LT
    PI -->|mode=local| LP

    FI -->|mode=远程| RF
    MI -->|mode=远程| RM
    TI -->|mode=远程| RT
    PI -->|mode=远程| RP

    RF --> CTX
    RM --> CTX
    RT --> CTX
    RP --> CTX

    CTX -->|mode=grpc| GRPC["gRPC<br/>PluginDataClient<br/>→ CmxPluginDataServerImpl"]
    CTX -->|mode=http_url| HTTP["HTTP multipart<br/>POST config.urls.*"]
    CTX -->|mode=http_discovery| HTTPD["HTTP multipart<br/>POST 服务发现实例"]

    GRPC --> PDI["PluginDataImporterImpl.import_data<br/>（cmx-biz，按 category 路由）"]
    HTTP --> PDI
    HTTPD --> PDI

    PDI -->|Perm| PZI["PermissionZipImporter trait<br/>→ PermissionServiceImpl (cmx-iam)"]
    PDI -->|Form| LF
    PDI -->|Menu| LM

    style 本地模式 fill:#e8f5e9
    style 远程模式 fill:#fff3e0
    style Bundle fill:#e3f2fd
```

---

## 二、模块导入流程

```mermaid
flowchart TD
    REQ["HTTP POST /api/module/package/import<br/>multipart 上传 module zip"] --> PH["package_handler.rs<br/>module_package_import"]

    PH --> CONST["构造 ModuleInstallService<br/>.new(package_utils, deploy_service)<br/>.with_definition_importers(bundle)"]

    CONST --> S1["1. fetch_and_extract<br/>解压 zip 到临时目录"]
    S1 --> S2["2. parse_manifest<br/>解析 module.manifest.json"]
    S2 --> S3{"3. app_id 守卫<br/>module.code == get_app_id()?"}
    S3 -->|不等| REJECT["❌ 拒绝导入"]
    S3 -->|相等| S4{"4. validate_import<br/>版本校验"}

    S4 -->|SkipSame| SKIP["跳过（已是当前版本）"]
    S4 -->|RejectOldVersion| REJECT
    S4 -->|AllowUpgrade / AllowForceDowngrade<br/>AllowSameSecondPatch| S5

    S5["5. install_module_resources<br/>（失败只 warn 不中断）"] --> S5A
    S5A["5a. 读 forms/*.json<br/>→ bundle.form.apply_form_definitions"] --> S5B
    S5B["5b. 读 menus/*.json<br/>→ bundle.menu.apply_menu_definitions"] --> S5C
    S5C["5c. 读 metadata/tables/*.json<br/>→ bundle.table.apply_table_definitions"] --> S5D
    S5D["5d. 读 permissions/*.json<br/>→ bundle.permission.apply_permission_definitions"]

    S5D --> S6["6. record_version<br/>ModuleVersionService.record_import<br/>（current_version + version_history）"]
    S6 --> S7["7. 遍历 manifest.plugins"]
    S7 --> S7A["每个插件子包 →<br/>deploy_service.deploy()<br/>自动判断 Install/Upgrade/跳过<br/>内部上传 OSS"]

    S7A --> DONE["✅ 导入完成"]

    style REJECT fill:#ffebee
    style SKIP fill:#fff9c4
    style DONE fill:#e8f5e9
```

---

## 三、模块导出流程

```mermaid
flowchart TD
    REQ["HTTP GET /api/module/package/export<br/>?domain_code=&application_code=&module_code="] --> PH["package_handler.rs<br/>module_package_export"]

    PH --> CONST["构造 ModuleExportService<br/>.new(plugin_root)<br/>.with_definition_importers(bundle)"]

    CONST --> E1["创建临时导出目录"]
    E1 --> E2["1. bundle.form.list_form_definitions(module)<br/>→ forms/form_0.json, form_1.json..."]
    E2 --> E3["2. bundle.menu.list_menu_definitions(module)<br/>→ menus/menu_0.json, menu_1.json..."]
    E3 --> E4["3. bundle.table.list_table_definitions(app, module)<br/>→ metadata/tables/module_tables.json"]
    E4 --> E5["4. bundle.permission.list_permission_definitions(domain, app, module)<br/>→ permissions/{module}_permissions.json"]

    E5 --> E6["5. export_plugins<br/>SQL 查 cmx_plugin → 取安装目录<br/>→ plugins/{plugin_id}.zip"]
    E6 --> E7["6. 写 module.json + module.manifest.json<br/>package_version = yyyyMMddHHmmSS"]
    E7 --> E8["7. ZipCompressor 打成单一 zip"]
    E8 --> E9["8. 清理临时目录"]
    E9 --> RESP["返回 zip 字节流"]

    style RESP fill:#e8f5e9
```

---

## 四、远程传输流程（Remote importer → 接收端入库）

```mermaid
flowchart LR
    subgraph 发送端["发送端（Remote Importer）"]
        DEF["结构化定义列表<br/>Vec&lt;FormDefinition&gt; 等"]
        PACK["packer::pack_definitions_to_zip<br/>或 pack_payload_to_zip<br/>→ ZIP 字节"]
        REQ["构造 PluginDataImportRequest<br/>{category, zip_data, domain/app/module}"]
        SEND["RemoteImporterContext.send()"]
    end

    DEF --> PACK --> REQ --> SEND

    SEND -->|mode=grpc| GRPC_SEND["send_via_grpc<br/>plugin_data_client()<br/>.import_plugin_data(svc, req)"]
    SEND -->|mode=http_url| HTTP_SEND["send_via_http<br/>POST config.urls.{category}<br/>multipart: file=zip + 元数据字段"]
    SEND -->|mode=http_discovery| HTTP_D_SEND["send_via_http<br/>服务发现解析实例<br/>POST http://{ip}:{port}/import"]

    GRPC_SEND --> RECV
    HTTP_SEND --> RECV
    HTTP_D_SEND --> RECV

    subgraph 接收端["接收端（cmx-biz PluginDataImporterImpl）"]
        RECV{"PluginDataImporterImpl<br/>(cmx-biz/src/plugin_data_importer.rs)<br/>.import_data(request)<br/>按 category 路由"}

        RECV -->|Perm| PERM["perm_zip_importer<br/>(PermissionZipImporter trait)<br/>→ PermissionServiceImpl (cmx-iam)<br/>解压ZIP → diff → 事务upsert → 审计"]
        RECV -->|Form| FORM["extract_json_files_from_zip<br/>→ Vec&lt;FormDefinition&gt;<br/>→ form_importer.apply_form_definitions"]
        RECV -->|Menu| MENU["extract_json_files_from_zip<br/>→ Vec&lt;MenuDefinition&gt;<br/>→ menu_importer.apply_menu_definitions"]
    end

    PERM --> DB[("cmx_permission")]
    FORM --> DB2[("cmx_form")]
    MENU --> DB3[("cmx_menu")]

    style 发送端 fill:#fff3e0
    style 接收端 fill:#e8f5e9
```

---

## 五、装配流程（web-server 启动）

```mermaid
flowchart TD
    START["web-server 启动<br/>init_iam_services()"] --> MM["获取 DatabaseManager + default_db_id"]

    MM --> RECV["始终构造接收端 Local 导入器<br/>receiver_form_importer (cmx-biz)<br/>receiver_menu_importer (cmx-biz)"]

    RECV --> PDI["PluginDataImporterImpl::new(perm_svc, perm_svc)<br/>(cmx-biz, 不依赖 cmx-iam 具体类型)<br/>  ↑ 第1参数: Arc&lt;dyn PermissionZipImporter&gt;<br/>  ↑ 第2参数: Arc&lt;dyn PermissionDefinitionImporter&gt;<br/>.with_form_importer(receiver_form)<br/>.with_menu_importer(receiver_menu)<br/>（本节点可作为远程接收端）"]

    PDI --> MODE["CenterClientConfig::load()<br/>读取 center_client.mode"]

    MODE --> IS_REMOTE{"mode ∈<br/>grpc/http_url/http_discovery?"}

    IS_REMOTE -->|否（local）| LOCAL_BUNDLE["本地 Bundle<br/>form = receiver_form_importer（复用）<br/>menu = receiver_menu_importer（复用）<br/>table = LocalTableDefinitionImporter（cmx-plugin）<br/>permission = permission_service_impl（cmx-iam）"]

    IS_REMOTE -->|是| REMOTE_CTX["构造 RemoteImporterContext<br/>（按 mode 初始化 reqwest/gRPC）"]
    REMOTE_CTX --> REMOTE_BUNDLE["远程 Bundle<br/>form = RemoteFormDefinitionImporter<br/>menu = RemoteMenuDefinitionImporter<br/>table = RemoteTableDefinitionImporter<br/>permission = RemotePermissionDefinitionImporter"]

    LOCAL_BUNDLE --> STATE["CmxAppState<br/>.definition_importers = Some(bundle)<br/>.plugin_data_importer = Some(pdi)"]
    REMOTE_BUNDLE --> STATE

    STATE --> HANDLER["package_handler<br/>import → ModuleInstallService.with_definition_importers<br/>export → ModuleExportService.with_definition_importers"]

    style LOCAL_BUNDLE fill:#e8f5e9
    style REMOTE_BUNDLE fill:#fff3e0
    style STATE fill:#e3f2fd
```

---

## 六、Trait 与类型关系图

```mermaid
graph LR
    subgraph cmx-traits["cmx-traits"]
        BUNDLE["DefinitionImporterBundle<br/>{form, menu, table, permission}"]

        FDI["FormDefinitionImporter<br/>apply + list"]
        MDI["MenuDefinitionImporter<br/>apply + list"]
        TDI["TableDefinitionImporter<br/>apply + list"]
        PDI2["PermissionDefinitionImporter<br/>apply + list"]

        subgraph cmx-traits-iam["cmx-traits::iam"]
            PDI2
        end

        subgraph cmx-traits-module["cmx-traits::module"]
            FDI
            MDI
            TDI
        end
    end

    subgraph cmx-core["cmx-core（契约结构体）"]
        FD["FormDefinition"]
        MD["MenuDefinition"]
        TD["TableDefine（已有）"]
        PD["PermissionDefinition（已有）"]
    end

    subgraph 本地实现["Local 实现"]
        LFI["LocalFormDefinitionImporter<br/>(cmx-biz)"]
        LMI["LocalMenuDefinitionImporter<br/>(cmx-biz)"]
        LTI["LocalTableDefinitionImporter<br/>(cmx-plugin)"]
        LPI["PermissionServiceImpl<br/>(cmx-iam)<br/>实现 PermissionDefinitionImporter<br/>+ PermissionZipImporter"]
    end

    subgraph 远程实现["Remote 实现"]
        RFI["RemoteFormDefinitionImporter<br/>(cmx-plugin)"]
        RMI["RemoteMenuDefinitionImporter<br/>(cmx-plugin)"]
        RTI["RemoteTableDefinitionImporter<br/>(cmx-plugin)"]
        RPI["RemotePermissionDefinitionImporter<br/>(cmx-plugin)"]
    end

    BUNDLE --> FDI
    BUNDLE --> MDI
    BUNDLE --> TDI
    BUNDLE --> PDI2

    FDI -.->|参数| FD
    MDI -.->|参数| MD
    TDI -.->|参数| TD
    PDI2 -.->|参数| PD

    FDI -.->|实现| LFI
    FDI -.->|实现| RFI
    MDI -.->|实现| LMI
    MDI -.->|实现| RMI
    TDI -.->|实现| LTI
    TDI -.->|实现| RTI
    PDI2 -.->|实现| LPI
    PDI2 -.->|实现| RPI

    style cmx-traits fill:#e3f2fd
    style cmx-core fill:#f3e5f5
    style 本地实现 fill:#e8f5e9
    style 远程实现 fill:#fff3e0
```

---

## 七、四类资源 apply / list 对照

```mermaid
graph LR
    subgraph 导入["导入（apply_*）"]
        F1["form.apply_form_definitions<br/>先 delete_by_code 再 create<br/>→ cmx_form"]
        M1["menu.apply_menu_definitions<br/>先 delete_by_code 再 create<br/>（MenuService 自动算树形字段）<br/>→ cmx_menu"]
        T1["table.apply_table_definitions<br/>PgTableDefineExecutor 建表到 biz 库<br/>+ TableMetadataService 登记到 default 库"]
        P1["permission.apply_permission_definitions<br/>两阶段 upsert:<br/>1. ON CONFLICT(code) upsert<br/>2. 回填 parent_id/level/full_code_path"]
    end

    subgraph 导出["导出（list_*）"]
        F2["form.list_form_definitions<br/>SELECT code,definition FROM cmx_form<br/>WHERE module_code=$1"]
        M2["menu.list_menu_definitions<br/>SELECT 根菜单 WHERE parent_id IS NULL"]
        T2["table.list_table_definitions<br/>JOIN cmx_meta_table_define + version"]
        P2["permission.list_permission_definitions<br/>SELECT cmx_permission<br/>+ 重建 parent_code"]
    end

    F1 -.->|对称| F2
    M1 -.->|对称| M2
    T1 -.->|对称| T2
    P1 -.->|对称| P2

    style 导入 fill:#e8f5e9
    style 导出 fill:#fff3e0
```

---

## 八、关键文件索引

| 层级 | 文件 | 职责 |
|------|------|------|
| **HTTP 入口** | `cmx-api/handlers/module/package_handler.rs` | import/export 端点，构造 Service + 注入 bundle |
| **导入编排** | `cmx-plugin/service/module_install.rs` | 解压→校验→install_resources→record_version→deploy 插件 |
| **导出编排** | `cmx-plugin/service/module_export.rs` | list_*→写 JSON→打包 zip |
| **Bundle 定义** | `cmx-traits/src/module/mod.rs` | DefinitionImporterBundle 四元组 |
| **四个 Trait** | `cmx-traits/src/module/{form,menu,table}.rs` + `iam/permission_definition_importer.rs` | apply + list 接口 |
| **契约结构体** | `cmx-core/src/model/module/definitions.rs` + `iam/permission.rs` | FormDefinition / MenuDefinition / PermissionDefinition |
| **Local Form/Menu** | `cmx-biz/src/{form,menu}/definition_importer.rs` | 直调 FormService/MenuService |
| **Local Table** | `cmx-plugin/service/table_definition_importer.rs` | 建表 + 元数据登记 |
| **Remote 四件套** | `cmx-plugin/service/remote_importers/{mod,form,menu,table,permission}.rs` | ZIP 打包 → ctx.send → gRPC/HTTP |
| **传输分发** | `cmx-plugin/service/remote_importers/mod.rs` | RemoteImporterContext.send → grpc/http |
| **ZIP 工具** | `cmx-plugin/src/center_client/packer.rs` | pack_definitions_to_zip / pack_payload_to_zip |
| **配置** | `cmx-plugin/src/center_client/{config,types}.rs` | CenterClientConfig + DataCategory |
| **gRPC 服务端** | `cmx-rpc/src/server/plugin_data.rs` | CmxPluginDataServerImpl → PluginDataImporter |
| **HTTP 接收端** | `cmx-api/src/handlers/iam/permission/import_handler.rs` | multipart 解析 → PluginDataImporter |
| **接收端路由** | `cmx-biz/src/plugin_data_importer.rs` | PluginDataImporterImpl 按 category 路由（从 cmx-iam 迁入） |
| **Perm ZIP 导入 trait** | `cmx-traits/src/iam/permission_definition_importer.rs` | PermissionZipImporter trait（Perm 的 ZIP 导入/清理） |
| **Perm ZIP trait 实现** | `cmx-iam/src/permission/zip_importer.rs` | PermissionServiceImpl 实现 PermissionZipImporter |
| **Perm 结构化导入** | `cmx-iam/src/permission/service/definition_importer.rs` | 两阶段 upsert（已实现 PermissionDefinitionImporter） |
| **装配** | `web-server/src/config/iam.rs` | 按 mode 选 Local/Remote bundle + 注入接收端 |
| **状态注入** | `cmx-api/src/app_state.rs` | CmxAppState.definition_importers |
