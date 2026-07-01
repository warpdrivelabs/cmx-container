# 模块为中心的插件解耦与迁移包方案 — 流程图

> 配套文档:`docs/superpowers/plans/20260630_cmx-plugin_模块为中心的插件解耦与迁移包方案.md`
>
> 本文用 Mermaid 流程图可视化方案的各核心操作流程,便于快速理解全貌与实施。

---

## 一、整体架构:模块下的资源归属关系

```mermaid
graph TD
    Domain["域 cmx_domain<br/>(FIN 财务域)"]
    App["应用 cmx_application<br/>(FI 财务会计)"]
    Module["模块 cmx_module<br/>(GL 总账) — 字典表,保持纯净"]

    Domain --> App --> Module

    Module --> Forms["表单 cmx_form<br/>(新增独立表)"]
    Module --> Menus["菜单 cmx_menu<br/>(新增独立表,树形)"]
    Module --> Metadata["表元数据 cmx_meta_table_define<br/>(已有,归属模块)"]
    Module --> Perms["权限 cmx_permission<br/>(已有,字段重命名)"]
    Module --> Plugins["插件 cmx_plugin<br/>(瘦身)"]

    Plugins --> P1["plugin_gl_posting<br/>manifest + servicedata<br/>+ wasm + wit + api + seeddata"]
    Plugins --> P2["plugin_gl_report<br/>manifest + servicedata<br/>+ wasm + wit + api + seeddata"]

    Module -.当前态.-> CurVer["cmx_module_current_version<br/>(每模块一行,版本/校验和/快照)"]
    Module -.历史.-> HisVer["cmx_module_version_history<br/>(多行,完整导入历史)"]

    classDef new fill:#e1f5ff,stroke:#0288d1,stroke-width:2px
    classDef modified fill:#fff4e1,stroke:#f57c00,stroke-width:2px
    classDef pure fill:#f1f8e9,stroke:#558b2f,stroke-width:2px
    class Forms,Menus,CurVer,HisVer new
    class Perms,Metadata modified
    class Module pure
```

---

## 二、模块迁移包结构

```mermaid
graph LR
    Pkg["module_FIN_GL_20260630103000.zip<br/>(版本号 = 导出时间戳)"]
    Pkg --> MF["module.manifest.json<br/>聚合清单"]
    Pkg --> MJ["module.json<br/>模块定义"]
    Pkg --> F["forms/<br/>表单 JSON"]
    Pkg --> M["menus/<br/>菜单树 JSON"]
    Pkg --> Meta["metadata/<br/>tables/*.json<br/>domain_app_module_config.json"]
    Pkg --> Perm["permissions/<br/>权限树 JSON"]
    Pkg --> PL["plugins/<br/>插件子包"]

    PL --> PL1["plugin_gl_posting.zip"]
    PL --> PL2["plugin_gl_report.zip"]

    PL1 --> PL1C["manifest.json<br/>servicedata/*<br/>*.wasm<br/>wit/*<br/>api/api.json<br/>seeddata/* ⭐"]

    classDef highlight fill:#fff9c4,stroke:#fbc02d,stroke-width:2px
    class PL1C highlight
```

> **关键约定:** 所有 `seeddata`(字典种子 + 业务表种子)统一保留在**插件子包内**,模块包不存储、不导出 seeddata。

---

## 三、模块导出流程(生成迁移包)

```mermaid
flowchart TD
    Start(["调用<br/>POST /api/module/package/export<br/>(domain_code, application_code, module_code)"])

    Start --> Q1["ModuleExportService::export_module"]

    Q1 --> Q2["1. 查 cmx_module<br/>→ 写 module.json"]
    Q2 --> Q3["2. 查 cmx_form WHERE module_code<br/>→ forms/{code}.json"]
    Q3 --> Q4["3. 查 cmx_menu WHERE module_code<br/>→ 组装菜单树 → menus/{module}_menu.json"]
    Q4 --> Q5["4. 查 cmx_permission WHERE module_code<br/>→ permissions/{module}_permissions.json"]
    Q5 --> Q6["5. 查 cmx_meta_table_define WHERE module_code<br/>→ metadata/tables/*.json + config"]

    Q6 --> Q7["6. 查 cmx_plugin WHERE module_code<br/>对每个插件读取安装目录"]
    Q7 --> Q8["只取 manifest + servicedata + wasm + wit + api + seeddata<br/>打成子 zip → plugins/{plugin_id}.zip"]

    Q8 --> Q9["7. 组装 module.manifest.json<br/>⭐ package_version = now().format(yyyyMMddHHmmSS)<br/>(自动生成时间戳,无需手动输入)"]
    Q9 --> Q10["8. 计算 checksum (sha256)<br/>可选 Ed25519 签名"]
    Q10 --> Q11["9. ZipCompressor 打成单一聚合 zip"]
    Q11 --> End(["返回 zip 字节流<br/>文件名: module_FIN_GL_20260630103000.zip"])

    style Start fill:#e8f5e9,stroke:#388e3c
    style End fill:#e8f5e9,stroke:#388e3c
    style Q9 fill:#fff9c4,stroke:#fbc02d,stroke-width:2px
```

---

## 四、模块导入流程(含版本校验)

```mermaid
flowchart TD
    Start(["调用<br/>POST /api/module/package/import?force=false<br/>(multipart 上传 zip)"])

    Start --> S1["ModuleInstallService::install_module_package"]
    S1 --> S2["1. fetch + 解压模块 zip 到临时目录<br/>(复用 PackageUtils)"]
    S2 --> S3["2. 解析 module.manifest.json<br/>得到 package_version + checksum"]

    S3 --> S4{"3. 查 cmx_module_current_version<br/>WHERE module_code (一行当前版本)"}

    S4 -->|模块不存在| S5["AllowUpgrade<br/>(新模块直接放行)"]
    S4 -->|checksum 相同| S6["SkipSame<br/>幂等跳过: '已是当前版本'"]
    S4 -->|需比对版本| S7{"package_version<br/>字符串比较<br/>(定长14位时间戳)"}

    S7 -->|新 > 当前| S5
    S7 -->|新 == 当前 checksum不同| S8["AllowSameSecondPatch<br/>同秒补丁"]
    S7 -->|新 < 当前| S9{"force?"}
    S9 -->|false| S10["RejectOldVersion ❌<br/>'无法用旧版本覆盖新版本'"]
    S9 -->|true| S11["AllowForceDowngrade<br/>强制降级"]

    S5 --> Install
    S8 --> Install
    S11 --> Install

    Install["4. upsert cmx_module<br/>(仅字典字段,保持纯净)"]
    Install --> VR["5. ⭐ 版本登记(事务内)<br/>a. cmx_module_current_version upsert (一行)<br/>b. cmx_module_version_history INSERT (防重)"]

    VR --> R1["6. 安装模块级资源"]
    R1 --> R1a["6a. metadata: execute_ddl_with_lock 建表<br/>+ save_plugin_table_metadata"]
    R1 --> R1b["6b. permissions: upsert cmx_permission"]
    R1 --> R1c["6c. forms: FormBmc.upsert_by_code"]
    R1 --> R1d["6d. menus: MenuBmc.upsert_by_code<br/>(计算 full_path/is_leaf/level)"]

    R1a --> P
    R1b --> P
    R1c --> P
    R1d --> P

    P["7. ⭐ 遍历 manifest.plugins<br/>逐个安装插件子包"]
    P --> P1["复用 InstallService::install<br/>(归属填模块三段式)<br/>seeddata 随插件子包加载<br/>建表/中心分发已注释,不重复"]

    P1 --> Done(["8. 事件发布 + 审计<br/>导入完成"])

    style Start fill:#e3f2fd,stroke:#1976d2
    style Done fill:#e8f5e9,stroke:#388e3c
    style S10 fill:#ffebee,stroke:#c62828
    style Install fill:#fff3e0,stroke:#f57c00
    style VR fill:#fff9c4,stroke:#fbc02d,stroke-width:2px
    style P fill:#e1f5fe,stroke:#0288d1,stroke-width:2px
```

---

## 五、版本校验决策树

```mermaid
flowchart TD
    Start(["导入模块包<br/>解析 package_version + checksum"])
    Start --> Q1{"cmx_module_current_version<br/>中该模块存在?"}

    Q1 -->|不存在| A1["✅ AllowUpgrade<br/>新模块,直接安装"]

    Q1 -->|存在| Q2{"checksum == 当前 checksum?"}
    Q2 -->|是| A2["⏭️ SkipSame<br/>幂等跳过: 已是当前版本"]

    Q2 -->|否| Q3{"package_version<br/>对比当前<br/>(字符串比较)"}

    Q3 -->|"新 > 当前"| A3["✅ AllowUpgrade<br/>正常升级"]
    Q3 -->|"新 == 当前"| A4["✅ AllowSameSecondPatch<br/>同秒补丁"]
    Q3 -->|"新 < 当前"| Q4{"force == true?"}

    Q4 -->|否| A5["❌ RejectOldVersion<br/>拒绝: 旧版本不可覆盖新版本"]
    Q4 -->|是| A6["⚠️ AllowForceDowngrade<br/>强制降级"]

    style A1 fill:#e8f5e9,stroke:#388e3c
    style A3 fill:#e8f5e9,stroke:#388e3c
    style A4 fill:#e8f5e9,stroke:#388e3c
    style A2 fill:#f5f5f5,stroke:#9e9e9e
    style A5 fill:#ffebee,stroke:#c62828
    style A6 fill:#fff3e0,stroke:#f57c00
```

---

## 六、插件单独安装流程(保留,注释旧逻辑)

```mermaid
flowchart TD
    Start(["POST /api/plugin/install<br/>(现有端点,保留不变)"])
    Start --> S1["InstallService::install"]
    S1 --> S2["1. fetch + 解压插件包"]
    S2 --> S3["2. 安全验证 security_validator"]
    S3 --> S4["3. 持久化 install_persist<br/>写 cmx_plugin + 复制 wasm"]
    S4 --> S5["4. ⛔ 建表 DDL<br/>execute_ddl_with_lock<br/>—— 已注释(TODO:module)"]
    S5 --> S6["5. ⛔ 中心分发 dispatch_install<br/>菜单/权限/表单推送<br/>—— 已注释(TODO:module)"]
    S6 --> S7["6. 运行时注册<br/>Registry + Contexts + Cache"]
    S7 --> S8["7. 审计日志 + 事件发布"]
    S8 --> End(["安装完成<br/>插件可正常运行"])

    style Start fill:#e3f2fd,stroke:#1976d2
    style End fill:#e8f5e9,stroke:#388e3c
    style S5 fill:#eeeeee,stroke:#9e9e9e,stroke-dasharray: 5 5
    style S6 fill:#eeeeee,stroke:#9e9e9e,stroke-dasharray: 5 5
```

> **说明:** 插件单独安装链路**完整保留**。仅注释掉两块(虚线):① 建表 DDL(已迁到模块流程);② 菜单/权限/表单中心分发(已迁到模块流程)。服务编排解析、运行时注册、审计、事件均保留。`seeddata` 加载也保留。

---

## 七、旧格式迁移脚本流程

```mermaid
flowchart TD
    Start(["migrate_to_module_packages<br/>--dry-run 可选"])
    Start --> S1["1. 查询所有已安装插件 cmx_plugin"]
    S1 --> S2["2. 按 domain_code + application_code + module_code 分组"]

    S2 --> S3["3. 对每个模块组"]
    S3 --> S4["3a. upsert cmx_module<br/>(从插件归属推导字典字段)"]
    S4 --> S5["3b. 遍历组内每个插件安装目录"]

    S5 --> S6["提取旧目录内容"]
    S6 --> S6a["formdata/*.json<br/>→ 写入 cmx_form"]
    S6 --> S6b["menudata/*.json<br/>→ 写入 cmx_menu<br/>(计算树形字段)"]
    S6 --> S6c["permdata/*.json<br/>→ 确认 cmx_permission<br/>(app_code→application_code)"]
    S6 --> S6d["metadata/*.json<br/>→ 重挂 module_code 归属"]

    S6a --> S7["3c. 写入版本记录<br/>cmx_module_current_version<br/>(package_version=迁移时间戳)"]
    S6b --> S7
    S6c --> S7
    S6d --> S7

    S7 --> S8["4. 输出迁移报告<br/>(成功/失败/跳过)"]
    S8 --> End(["迁移完成"])

    style Start fill:#fce4ec,stroke:#c2185b
    style End fill:#e8f5e9,stroke:#388e3c
```

> **幂等保证:** 基于 `code` 唯一索引 upsert,可重复运行;`--dry-run` 仅预检不写库。

---

## 八、模块版本管理:三表协作

```mermaid
sequenceDiagram
    autonumber
    participant Exp as 导出端
    participant Imp as ModuleInstallService
    participant Cur as cmx_module_current_version
    participant His as cmx_module_version_history
    participant Dic as cmx_module (字典表)

    Note over Exp: 导出时自动生成<br/>package_version = yyyyMMddHHmmSS

    Exp->>Imp: 上传模块 zip<br/>(含 package_version=20260630103000)

    Imp->>Cur: 读取当前版本<br/>WHERE module_code=GL
    Cur-->>Imp: package_version=20260630090000

    Note over Imp: 版本校验<br/>20260630103000 > 20260630090000<br/>→ AllowUpgrade

    rect rgb(255, 249, 196)
        Note over Imp,Cur: 事务内执行
        Imp->>Dic: upsert cmx_module<br/>(仅 code/name 字典字段)
        Imp->>Cur: upsert current_version<br/>(uk=module_code 保证一行)
        Imp->>His: INSERT version_history<br/>(uk=module_code+package_version 防重)
    end

    Note over Imp: 安装资源 + 插件子包

    Imp-->>Exp: 导入成功
```

---

## 九、实施阶段总览(6 阶段)

```mermaid
gantt
    title 实施阶段(每阶段可独立测试提交)
    dateFormat YYYY-MM-DD
    axisFormat %m-%d

    section 阶段1 数据模型与持久化
    SQL迁移(cmx_form/cmx_menu/权限重命名/版本两表)  :a1, 2026-07-01, 3d
    cmx-core 模型(Form/Menu/ModuleManifest)         :a2, after a1, 2d
    cmx-biz BMC(Form/Menu/Version)                  :a3, after a2, 3d

    section 阶段2 API层
    表单/菜单 CRUD Handler(宏生成)                   :b1, after a3, 2d

    section 阶段3 注释旧分发逻辑
    persistence.rs 注释建表DDL                        :c1, after b1, 1d
    executor.rs 注释中心分发                          :c2, after c1, 1d

    section 阶段4 模块包导入
    ModuleInstallService(含版本校验)                  :d1, after c2, 3d
    模块导入 API端点                                  :d2, after d1, 1d

    section 阶段5 模块包导出
    ModuleExportService                              :e1, after d2, 3d
    导出 API端点                                      :e2, after e1, 1d

    section 阶段6 迁移脚本
    migrate_to_module_packages                       :f1, after e2, 2d
```

---

## 十、端到端数据流总览

```mermaid
flowchart LR
    subgraph 源环境
        DB1[("cmx_module<br/>cmx_form<br/>cmx_menu<br/>cmx_permission<br/>cmx_meta_table_define<br/>cmx_plugin + 安装目录")]
    end

    subgraph 导出
        EXP["ModuleExportService<br/>export_module()"]
    end

    PKG["module_FIN_GL_20260630103000.zip<br/>(单一聚合 zip)"]

    subgraph 导入
        IMP["ModuleInstallService<br/>install_module_package()"]
    end

    subgraph 目标环境
        DB2[("cmx_module_current_version ✨<br/>cmx_module_version_history ✨<br/>cmx_module<br/>cmx_form ✨<br/>cmx_menu ✨<br/>cmx_permission (重命名)<br/>cmx_meta_table_define<br/>cmx_plugin + 安装目录")]
    end

    DB1 --> EXP --> PKG
    PKG --> IMP --> DB2

    style PKG fill:#fff9c4,stroke:#fbc02d,stroke-width:3px
    style EXP fill:#e8f5e9,stroke:#388e3c
    style IMP fill:#e3f2fd,stroke:#1976d2
```

> ✨ = 新增表;迁移包以模块为原子单位,版本号自动生成时间戳,旧版本默认不可覆盖新版本。

---

## 图例说明

| 标记 | 含义 |
|------|------|
| ✨ / 蓝色 | 新增(表/模块/代码) |
| 🟡 / 黄色 | 版本管理关键点 / 需特别注意 |
| ⛔ / 灰色虚线 | 已注释保留的代码块 |
| ❌ / 红色 | 拒绝/错误路径 |
| ⭐ | 核心复用点 / 关键约定 |
