# 配置驱动的表和列元数据系统

这个模块提供了基于配置的表结构定义方式，相比传统的enum方式更加灵活和可扩展。

## 🎯 核心特性

### 优势
- **运行时灵活性**: 无需重新编译即可添加新表和修改结构
- **配置驱动**: 支持JSON/YAML配置文件
- **类型安全**: 提供完整的类型定义和验证
- **扩展性**: 支持复杂的数据类型、外键、索引等
- **序列化**: 支持JSON序列化，便于存储和传输

### 与Enum方式的对比

| 特性 | Enum方式 | 配置驱动方式 |
|------|----------|-------------|
| 编译时检查 | ✅ 强类型 | ⚠️ 运行时验证 |
| 灵活性 | ❌ 固定结构 | ✅ 动态配置 |
| 扩展性 | ❌ 需要修改代码 | ✅ 配置文件修改 |
| 内存占用 | ✅ 最小化 | ⚠️ 配置数据占用 |
| IDE支持 | ✅ 自动补全 | ⚠️ 部分支持 |
| 维护成本 | ❌ 高 | ✅ 低 |

## 📁 模块结构

```
meta/
├── schema.rs          # 核心配置驱动实现
├── tables.rs          # 表名定义（兼容现有代码）
├── fields.rs          # 字段定义（兼容现有代码）
├── mod.rs            # 模块导出
└── README.md         # 文档
```

## 🚀 快速开始

### 基本用法

```rust
use cmx_core::model::meta::schema::{create_default_registry, MetadataRegistry};

// 创建默认注册表（包含所有系统表）
let registry = create_default_registry();

// 查询表信息
if let Some(table) = registry.get_table("CMX_SYS_DOMAINS") {
    println!("表名: {}", table.name);
    println!("描述: {}", table.description);
    println!("列数: {}", table.columns.len());
    println!("主键: {:?}", table.primary_keys);
}
```

### 自定义表定义

```rust
use cmx_core::model::meta::schema::{TableMetadata, ColumnMetadata, DataType};

let custom_table = TableMetadata::new("CUSTOM_USERS", "自定义用户表")
    .add_column(
        ColumnMetadata::new("id", DataType::BigInteger)
            .primary_key(true)
            .auto_increment(true)
            .description("用户ID")
    )
    .add_column(
        ColumnMetadata::new("username", DataType::Varchar(50))
            .unique(true)
            .nullable(false)
            .description("用户名")
    )
    .add_column(
        ColumnMetadata::new("email", DataType::Varchar(255))
            .unique(true)
            .nullable(false)
            .description("邮箱")
    );

let mut registry = MetadataRegistry::new();
registry.add_table(custom_table);
```

### JSON配置

```rust
// 从JSON字符串加载配置
let json_config = r#"[
    {
        "name": "MY_TABLE",
        "description": "我的表",
        "columns": [
            {
                "name": "id",
                "data_type": {"BigInteger": null},
                "nullable": false,
                "description": "主键",
                "is_primary_key": true
            }
        ],
        "primary_keys": ["id"]
    }
]"#;

let registry = MetadataRegistry::load_from_json(json_config)?;
```

### 建表 JSON 多文件配置

当表定义拆分为多个 JSON 文件（如 `oracle_tables_01.json` … `oracle_tables_22.json`）时，可用**建表配置文件**描述文件列表，并由 `TableDefinesConfigManager` 统一加载：

**配置文件格式**（如 `oracle_tables_config.json`）：

```json
{
  "name": "oracle_tables",
  "description": "Oracle DDL 导出的表定义（多文件）",
  "depends_on": ["sys_tables"],
  "priority": 10,
  "files": [
    "oracle_tables_01.json",
    "oracle_tables_02.json"
  ]
}
```

- **depends_on**（可选）：依赖的配置名称列表；被依赖的配置会**先于**本配置加载。
- **priority**（可选）：数值越小越先加载；同层级或无依赖关系时按此排序。缺省视为 0。

**Rust 用法**：

```rust
use cmx_core::model::meta::base::{
    TableDefinesConfig, TableDefinesConfigManager,
    load_table_defines_config_from_path,
};

// 方式一：从多个配置文件路径构建管理器
let manager = TableDefinesConfigManager::from_config_paths(&[
    "path/to/oracle_tables_config.json",
    "path/to/sys_tables_config.json",
])?;

// 加载所有配置指向的表定义（base_path 为表定义 JSON 所在目录）
let all_tables = manager.load_all_tables(Path::new("path/to/meta"))?;

// 方式二：只加载指定名称的配置
let oracle_tables = manager.load_tables_by_config_name(
    Path::new("path/to/meta"),
    "oracle_tables",
)?;

// 按依赖与优先级排序后的配置顺序（被依赖的在前）
let order = manager.sorted_configs()?;
```

若存在**循环依赖**或**依赖了不存在的配置名**，`sorted_configs()` 与 `load_all_tables()` 会返回 `BaseError::ConfigDependency`。

### 企业应用分层表（域-应用-模块）

提供**域 → 应用 → 模块**三层结构的表定义，用于企业应用分层管理：

| 表名 | 说明 | 示例 |
|------|------|------|
| `cmx_domain` | 域（顶层） | 财务域、供应链域、人力资源域 |
| `cmx_application` | 应用（中层，归属域） | 会计核算应用、资金管理应用 |
| `cmx_module` | 模块（底层，归属应用） | 总账模块、应收应付模块、报表模块 |

- **配置文件**：`domain_app_module_config.json`，引用 `domain_app_module_tables.json`。
- **关系**：`cmx_application.domain_id` → `cmx_domain.id`；`cmx_module.application_id` → `cmx_application.id`。
- **唯一约束**：域编码全局唯一；同一域下应用编码唯一；同一应用下模块编码唯一。
- **类型与标签**：每张表均有可选字段 **`type`**（单类型）与 **`tags`**（多标签，存 JSON 数组字符串如 `["财务","核心"]`），便于按类型筛选与多维度打标；`type` 列已建普通索引。推荐取值示例：
  - **域 type**：`business` 业务域、`technical` 技术域、`product_line` 产品线；
  - **应用 type**：`product` 产品应用、`platform` 共享中台/平台能力、`integration` 集成应用；
  - **模块 type**：`business` 业务模块、`extension` 扩展点、`integration` 集成点。

**与 SAP / Oracle EBS 术语对照**（便于与主流 ERP 概念对齐）：

| 本系统（域-应用-模块） | SAP 近似术语 | SAP 示例 | Oracle EBS 近似术语 | Oracle EBS 示例 |
|------------------------|--------------|----------|----------------------|------------------|
| **域** `cmx_domain` | Solution / Product Line（解决方案/产品线） | SAP S/4HANA、SAP ERP、SAP SuccessFactors | Product Family / Suite（产品系列/套件） | Financials、HRMS、SCM、CRM |
| **应用** `cmx_application` | Application Area / Module（应用领域/模块） | FI 财务会计、CO 管理会计、MM 物料管理、SD 销售与分销 | Application（应用） | General Ledger、Payables、Receivables、Order Management |
| **模块** `cmx_module` | Component / Sub-module（组件/子模块） | FI-GL 总账、FI-AR 应收、FI-AP 应付、MM-PUR 采购 | Module / Form / Function（功能/表单） | 各应用下的表单、报表、并发程序等 |

- **SAP**：顶层为解决方案/产品线，中层为应用领域（FI、CO、MM 等，常用 2 位编码），底层为组件（如 FI-GL、FI-AR）；另有 Industry Solution（行业方案）维度。
- **Oracle EBS**：顶层为产品系列（Financials、HRMS 等），中层即具体应用（GL、AP、AR、OM 等），底层为应用内的功能/表单。
- 本系统三层与两者均可一一对应；若需在 `type` / `tags` 中区分风格，可使用如 `sap_style`、`ebs_style` 或对应编码体系。

### 插件市场与插件注册库

提供**插件市场**（可购买、下载、安装、卸载、查看）与**插件注册库**（已安装插件的查询、升级、卸载）表定义，均按域-应用-模块组织：

| 表名 | 说明 | 主要操作 |
|------|------|----------|
| `cmx_plugin_catalog` | 插件市场-商品目录 | 按域/应用/模块浏览、查看、下载 |
| `cmx_plugin_marketplace_version` | 插件市场-可下载版本 | 版本列表、下载地址、校验和 |
| `cmx_plugin_marketplace_price` | 插件市场-价格（可选） | 付费插件定价、币种、有效期 |
| `cmx_plugin_installed` | 插件注册库-已安装 | 查询、升级、卸载 |

- **配置文件**：`plugin_marketplace_registry_config.json`，依赖 `domain_app_module`。
- **关系**：`cmx_plugin_catalog` / `cmx_plugin_installed` 通过 `domain_code`、`application_code`、`module_code` 与域-应用-模块表关联；`cmx_plugin_marketplace_version` → `cmx_plugin_catalog`；`cmx_plugin_marketplace_price` → `cmx_plugin_catalog`。
- **设计文档**：见 `docs/plugin_marketplace_registry_design.md`。

## 📋 数据类型

支持以下数据类型：

- **字符串**: `Varchar(usize)`, `Char(usize)`, `Text`
- **数字**: `Integer`, `BigInteger`, `Float`, `Double`, `Decimal(u8, u8)`
- **时间**: `DateTime`, `Date`, `Time`
- **其他**: `Boolean`, `Binary`, `Json`, `Uuid`

## 🛠️ 高级功能

### 外键关系

```rust
let table = TableMetadata::new("ORDERS", "订单表")
    .add_column(
        ColumnMetadata::new("customer_id", DataType::BigInteger)
            .foreign_key("CUSTOMERS.id")
            .description("客户ID")
    );
```

### 索引定义

```rust
let index = IndexMetadata {
    name: "idx_orders_customer_date".to_string(),
    columns: vec!["customer_id".to_string(), "order_date".to_string()],
    unique: false,
    index_type: IndexType::BTree,
};

let table = TableMetadata::new("ORDERS", "订单表")
    .add_index(index);
```

### 检查约束

```rust
let constraint = CheckConstraint {
    name: "chk_age_positive".to_string(),
    expression: "age > 0".to_string(),
    description: "年龄必须为正数".to_string(),
};

let table = TableMetadata::new("USERS", "用户表")
    .add_check_constraint(constraint);
```

## 🔍 验证和检查

### 模式验证

```rust
use cmx_core::examples::schema_usage::SchemaValidator;

let registry = create_default_registry();
let validator = SchemaValidator::new(registry);

let issues = validator.validate_schema();
for issue in issues {
    println!("验证问题: {}", issue);
}
```

### SQL生成

```rust
let validator = SchemaValidator::new(registry);
if let Some(sql) = validator.generate_create_table_sql("MY_TABLE") {
    println!("建表SQL:\n{}", sql);
}
```

## 📊 性能考虑

- **内存占用**: 配置数据常驻内存，可考虑lazy loading
- **序列化**: 大型schema建议使用二进制格式
- **验证**: 生产环境建议预验证所有配置
- **缓存**: 频繁访问的表元数据建议缓存

## 🔄 迁移指南

### 从Enum方式迁移

1. **保留兼容层**: 保持现有enum定义用于向后兼容
2. **逐步迁移**: 新功能使用配置驱动方式
3. **数据同步**: 确保两套定义的数据一致性
4. **测试验证**: 充分测试迁移后的功能

### 配置文件管理

建议的配置结构：
```
config/
├── tables/           # 表定义
│   ├── system.json   # 系统表
│   ├── business.json # 业务表
│   └── custom.json   # 自定义表
└── schema.yaml       # 主配置文件
```

## 🧪 测试

运行测试：
```bash
cargo test -p cmx-core schema
```

## 📚 相关文档

- [Enum vs 配置驱动对比](./comparison.md)
- [数据类型参考](./data_types.md)
- [最佳实践](./best_practices.md)
