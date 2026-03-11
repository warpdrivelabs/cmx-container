# cmx-core

**cmx-core** 是企业级 ERP 系统的核心库，提供领域模型、数据结构和插件系统的完整实现。

## 📦 模块结构

```
cmx-core/
├── model/          # 数据模型层
│   ├── cell/       # 单元格数据值类型系统
│   ├── data/       # 业务数据传输对象
│   │   ├── request/    # 请求接口定义
│   │   ├── response/   # 响应接口定义
│   │   ├── context/    # 上下文管理
│   │   └── dataset/    # 行式数据集（支持嵌套结构）
│   ├── meta/       # 元数据定义
│   │   ├── base/       # 基础元数据
│   │   ├── fields/     # 字段定义
│   │   ├── tables/     # 表定义

│   ├── domain/     # 领域驱动设计
│   │   ├── entity/     # 领域实体
│   │   └── manager/    # 实体管理器
│   └── cell.rs     # 核心数据类型定义
└── plugin/         # 插件系统
    ├── def.rs      # 插件定义与清单
    └── registry.rs # 插件注册表与验签
```

## 🎯 核心功能

### 1. **统一数据值类型系统** ([cell.rs](file:///crates/libs/cmx-core/src/model/cell.rs))

提供高精度的数据类型支持，适用于 ERP 复杂业务场景：

- **基础类型**: `Null`, `Bool`, `Int`, `Float`, `String`, `Decimal`
- **时间类型**: `DateTime<Utc>`, `NaiveDate`
- **优化类型**: `SmolStr` (小字符串优化，减少堆分配)
- **序列化支持**: 完整的 serde 序列化/反序列化

```rust
// 使用示例
let value = DataValue::from(42i32);
let decimal_val = DataValue::from(Decimal::new(100, 2));
let date_val = DataValue::from(Utc::now());
```

### 2. **行式数据集** ([dataset](file:///crates/libs/cmx-core/src/model/data/dataset/mod.rs))

高性能内存数据集，支持嵌套数据结构：

- **Schema 优化**: O(1) 字段查找（维护 name->index 映射）
- **嵌套支持**: Row 可包含子 DataSet，适合主从表结构
- **零拷贝读取**: 高效的数据访问模式
- **序列化友好**: 直接序列化为 JSON，前端友好

```rust
// 订单头 + 订单行嵌套示例
let mut lines_ds = DataSet::empty("order_lines", line_schema);
lines_ds.add_row(Row::new(vec![10.into(), "Mat-A".into(), 5.into()]));

header_row.add_child("order_lines", lines_ds);
```

### 3. **元数据系统** ([meta](file:///crates/libs/cmx-core/src/model/meta/mod.rs))

完整的数据库表元数据定义：

- **TableDefine**: 表结构定义（列、索引、主键、分区等）
- **ColumnDefine**: 列定义（类型、约束、外键、多语言支持）
- **IndexDefine**: 索引定义（唯一索引、复合索引）
- **多语言支持**: i18n 字段级别的多语言翻译
- **分区表支持**: RANGE/LIST/HASH/INTERVAL 分区

```rust
// 表定义示例
let table_define = TableDefine {
    table_name: "sale_order".to_string(),
    display_name: "销售订单".to_string(),
    columns: vec![/* ... */],
    primary_keys: vec!["id".to_string()],
    indexes: vec![/* ... */],
    i18n: true, // 支持多语言
    ..default
};
```

### 4. **请求 - 响应框架** ([request](file:///crates/libs/cmx-core/src/model/data/request/mod.rs), [response](file:///crates/libs/cmx-core/src/model/data/response/mod.rs))

标准化的服务调用接口：

```rust
// 请求 trait
pub trait CMXRequest: Any + Send + Sync {
    fn get_request_id(&self) -> &str;
    fn get_service_name(&self) -> &str;
    fn get_function_name(&self) -> &str;
    fn get_parameters(&self) -> &str;
    fn get_headers(&self) -> &HashMap<String, String>;
    fn get_timeout(&self) -> u64;
    fn set_timeout(&mut self, timeout: u64);
    fn add_header(&mut self, key: String, value: String);
}

// 响应 trait
pub trait CMXResponse: Any + Send + Sync {
    fn get_request_id(&self) -> &str;
    fn get_status_code(&self) -> i32;
    fn get_data(&self) -> Option<&str>;
    fn get_error(&self) -> Option<&str>;
}
```

**设计亮点**:
- 使用 `Any` trait 支持运行时类型识别和向下转型
- `Send + Sync` 确保多线程安全
- 适合插件系统、动态加载等反射场景

### 5. **插件系统** ([plugin](file:///crates/libs/cmx-core/src/plugin/mod.rs))

基于 WASM 的插件架构，支持安全的插件加载和验签：

#### 插件定义 ([PluginDefinition](file:///crates/libs/cmx-core/src/plugin/def.rs#L42))

```json
{
  "id": "com.example.sales",
  "name": "销售管理插件",
  "version": "1.0.0",
  "wasm_file": "target/sales.wasm",
  "table_config_files": ["tables/sale_order.json"],
  "supported_databases": ["postgres", "mysql"],
  "domain_code": "SALES",
  "development_languages": ["rust"]
}
```

#### 装配清单 ([PluginManifest](file:///crates/libs/cmx-core/src/plugin/def.rs#L123))

ZIP 格式的插件包，包含：
- **manifest.json**: 装配清单（含签名）
- **WASM 二进制**: 插件逻辑
- **表定义 JSON**: 数据库表配置
- **其他资源**: 静态文件、配置等

#### 安全特性

- **数字签名**: Ed25519 签名防篡改
- **验签机制**: 加载前验证插件完整性
- **信任链**: 基于公钥标识的密钥管理

```rust
// 插件注册表使用
let registry = PluginRegistry::new();
registry.load_plugin("plugin.zip", VerifySignatureConfig::Strict)?;
```

## 🔧 技术栈

### 核心依赖

| 类别 | 库 | 用途 |
|------|---|------|
| **序列化** | serde, serde_json | 数据序列化框架 |
| **日期时间** | chrono | 日期时间处理（支持 serde） |
| **数值计算** | rust_decimal, bigdecimal | 高精度十进制运算 |
| **字符串优化** | smol_str | 小字符串内联存储（≤22 字节） |
| **错误处理** | thiserror | 自定义错误类型 |
| **唯一 ID** | uuid | UUID v4 生成 |
| **加密** | ed25519-dalek, base64 | 数字签名与编码 |
| **压缩** | zip | ZIP 文件处理 |

### 异步与工具

- **futures, tokio-stream, tokio-util**: 异步编程工具
- **proc-macro2, quote, syn**: 过程宏支持
- **strum**: 枚举字符串转换
- **lazy_static, once_cell**: 延迟初始化

## 🏗️ 设计哲学

### 1. **性能优先**

- Schema 使用 HashMap 实现 O(1) 字段查找
- SmolStr 优化短字符串，减少堆分配
- 零拷贝设计，避免不必要的数据复制

### 2. **类型安全**

- 强类型枚举 `DataValue` 替代动态 JSON
- 编译时类型检查，减少运行时错误
- 完整的序列化/反序列化支持

### 3. **扩展性**

- Trait-based 设计（CMXRequest/CMXResponse）
- 插件化架构，支持热插拔
- 元数据驱动，支持动态表结构

### 4. **安全性**

- 插件签名验签机制
- 多线程安全（Send + Sync）
- 严格的类型转换和验证

## 📝 使用示例

### 创建数据集

```rust
use cmx_core::model::data::dataset::{DataSet, Row, Schema, Field};
use cmx_core::model::cell::{FieldType, DataValue};
use std::sync::Arc;

// 定义 Schema
let schema = Arc::new(Schema::new("users", vec![
    Field { name: "id".into(), field_type: FieldType::Int, label: "ID".into() },
    Field { name: "name".into(), field_type: FieldType::String, label: "姓名".into() },
]));

// 创建数据集
let mut ds = DataSet::empty("user_list", schema.clone());
ds.add_row(Row::new(vec![1.into(), "张三".into()]));
ds.add_row(Row::new(vec![2.into(), "李四".into()]));

// 序列化为 JSON
let json = serde_json::to_string_pretty(&ds).unwrap();
```

### 定义表结构

```rust
use cmx_core::model::cell::{TableDefine, ColumnDefine, FieldType, IndexDefine, IndexKind};

let table = TableDefine {
    table_name: "sale_order".to_string(),
    display_name: "销售订单".to_string(),
    columns: vec![
        ColumnDefine {
            name: "id".to_string(),
            label: "订单 ID".to_string(),
            field_type: FieldType::Int,
            is_primary_key: true,
            is_nullable: false,
            ..Default::default()
        },
        ColumnDefine {
            name: "doc_no".to_string(),
            label: "单据号".to_string(),
            field_type: FieldType::String,
            length: Some(30),
            ..Default::default()
        },
    ],
    primary_keys: vec!["id".to_string()],
    indexes: vec![
        IndexDefine {
            name: "uk_doc_no".to_string(),
            columns: vec!["doc_no".to_string()],
            kind: IndexKind::Unique,
        }
    ],
    i18n: true, // 支持多语言
    ..Default::default()
};
```

### 插件加载

```rust
use cmx_core::plugin::{PluginRegistry, VerifySignatureConfig};

let registry = PluginRegistry::new();

// 加载并验证插件签名
match registry.load_plugin("my_plugin.zip", VerifySignatureConfig::Strict) {
    Ok(plugin_id) => println!("插件加载成功：{}", plugin_id),
    Err(e) => eprintln!("插件加载失败：{:?}", e),
}
```

## 🎓 关键概念

### DataValue vs serde_json::Value

| 特性 | DataValue | serde_json::Value |
|------|-----------|-------------------|
| **类型精度** | 保留 Decimal/DateTime 类型 | 全部转为 JSON 类型 |
| **性能** | 栈上存储，零拷贝 | 堆分配为主 |
| **类型安全** | 强类型枚举 | 动态类型 |
| **适用场景** | 业务逻辑处理 | 数据交换 |

### Schema 优化原理

```rust
// 传统方式：O(n) 查找
fields.iter().find(|f| f.name == "target")

// cmx-core 方式：O(1) 查找
schema.get_index("target") // HashMap 查找
```

### 插件签名流程

1. **提取载荷**: 从 PluginManifest 提取不含签名的部分
2. **规范序列化**: 转为规范 JSON 字节（键顺序固定）
3. **数字签名**: 使用私钥对字节签名（Ed25519）
4. **Base64 编码**: 将签名转为字符串存入 manifest
5. **验签**: 反向流程验证完整性

## 📚 后续扩展

- [ ] 增加数组类型支持 `Array(Vec<DataValue>)`
- [ ] 增加二进制类型支持 `Binary(Vec<u8>)`
- [ ] 完善查询构建器集成（sea-query）
- [ ] 增加数据验证规则引擎
- [ ] 支持更多数据库方言

## 🤝 贡献指南

欢迎提交 Issue 和 Pull Request！

## 📄 许可证

与主项目保持一致
