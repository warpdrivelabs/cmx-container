# cmx-core: TableDefine 相关类型迁移评估

## 1. 当前结构分析

### 1.1 cell.rs 包含的内容

| 类型                     | 行数     | 职责                | 是否应迁移     |
| ---------------------- | ------ | ----------------- | --------- |
| `DataValue`            | ~400行 | ERP通用数据值枚举（运行时数据） | ❌ 保留      |
| `DataValue` 序列化/反序列化   | ~150行 | JSON序列化策略         | ❌ 保留      |
| `impl From/TryFrom` 转换 | ~150行 | 基础类型转换            | ❌ 保留      |
| `FieldType`            | ~35行  | 字段类型枚举            | ✅ 迁移到meta |
| `Field`                | ~5行   | 简单字段定义            | ✅ 迁移到meta |
| `ColumnDefine`         | ~50行  | 列定义               | ✅ 迁移到meta |
| `IndexKind`            | ~10行  | 索引类型枚举            | ✅ 迁移到meta |
| `IndexDefine`          | ~15行  | 索引定义              | ✅ 迁移到meta |
| `PartitionType`        | ~15行  | 分区类型枚举            | ✅ 迁移到meta |
| `TableDefine`          | ~50行  | 表定义               | ✅ 迁移到meta |

### 1.2 cell.rs 测试代码分析

cell.rs 中的 `#[cfg(test)]` 模块（约 520 行）包含：

| 测试类别 | 行范围 | 测试内容 | 是否迁移 |
|---------|--------|---------|---------|
| Binary 类型测试 | ~646-702 | Binary 创建、From trait、序列化、反序列化 | ❌ 保留 |
| Array 类型测试 | ~708-774 | Array 创建、嵌套、序列化 | ❌ 保留 |
| Json 类型测试 | ~780-847 | Json 创建、序列化、反序列化 | ❌ 保留 |
| Uuid 类型测试 | ~853-911 | Uuid 创建、序列化、反序列化 | ❌ 保留 |
| **FieldType 枚举测试** | ~917-959 | FieldType 序列化/反序列化 | ✅ 迁移 |
| ERP 场景集成测试 | ~965-1069 | 附件、订单标签、自定义字段等场景 | ❌ 保留 |
| DateTime/Date 测试 | ~1075-1155 | DateTime/Date 序列化往返 | ❌ 保留 |

**结论**：仅 `FieldType 枚举测试`（约 40 行）需要随 TableDefine 类型迁移。

### 1.3 关键发现

**cell.rs 实际包含两类完全不同职责的内容：**

1. **DataValue 体系** - 运行时数据值处理，位于"数据层"
2. **TableDefine 体系** - 表元数据定义，位于"元数据层"

**meta 目录的职责（来自 README.md）：**

> 提供基于配置的表结构定义方式，相比传统的enum方式更加灵活和可扩展。

当前 meta 目录只有 `fields.rs`（枚举字段）、`tables.rs`（枚举表名）、`plugin.rs`（插件定义），
**缺少配置驱动的表结构定义核心类型**。

***

## 2. 迁移方案

### 2.1 推荐方案：迁移 TableDefine 到 meta/table.rs

```
cmx-core/src/model/meta/
├── mod.rs           # 现有
├── fields.rs        # 现有（保留）
├── tables.rs        # 现有（保留）
├── plugin.rs        # 现有（保留）
├── table.rs         # 【新建】TableDefine 相关类型 + 单元测试
└── README.md        # 现有（更新）
```

**table.rs 包含的内容：**
```rust
// ==========================================
// 表元数据类型定义
// ==========================================

/// 字段类型枚举
pub enum FieldType { ... }

/// 列定义
pub struct ColumnDefine { ... }

/// 索引类型
pub enum IndexKind { ... }

/// 索引定义
pub struct IndexDefine { ... }

/// 分区类型
pub enum PartitionType { ... }

/// 表定义
pub struct TableDefine { ... }

// ==========================================
// 单元测试（迁移自 cell.rs）
// ==========================================
#[cfg(test)]
mod tests {
    // FieldType 枚举序列化/反序列化测试
    // ...
}
```

### 2.2 迁移后的兼容层

为避免大规模修改依赖方代码，**保留 cell.rs 中的类型作为重导出**：

```rust
// cell.rs - 保留作为兼容层
pub use model::meta::table::{
    FieldType, ColumnDefine, IndexKind, IndexDefine, PartitionType, TableDefine
};
```

### 2.3 测试迁移策略

| 原位置 | 目标位置 | 测试内容 |
|--------|---------|---------|
| cell.rs ~917-959 | meta/table.rs #[cfg(test)] | FieldType 枚举序列化/反序列化测试 |

**注意**：
- DataValue 相关测试（Binary、Array、Json、Uuid、DateTime/Date）保留在 cell.rs
- ERP 场景集成测试保留在 cell.rs

***

## 3. 依赖影响分析

### 3.1 当前依赖关系

```
cmx-core (cell.rs: TableDefine, DataValue, ColumnDefine, ...)
    ↓ 导出类型
cmx-metadata (使用 TableDefine, ColumnDefine, IndexDefine, FieldType)
    ↓ 依赖
cmx-database, cmx-plugin, ...
```

### 3.2 需要修改的文件清单

#### A. cmx-core 内部修改

| 文件 | 修改内容 |
|------|---------|
| `src/model/meta/mod.rs` | 添加 `pub mod table;` |
| `src/model/meta/table.rs` | 【新建】从 cell.rs 迁移的类型 + 测试 |
| `src/model/cell.rs` | 移除已迁移类型 + 测试，添加兼容重导出 |
| `src/model/mod.rs` | 无需修改 |

#### B. 依赖方（若使用兼容层则无需修改）

| Crate | 文件 | 现状 | 需修改 |
|-------|------|------|--------|
| cmx-metadata | 所有使用 TableDefine 的文件 | `use cmx_core::model::cell::*` | 无（兼容层） |
| cmx-core/tests | test_dataset_serde.rs | 使用 DataValue | 无需修改 |

***

## 4. 实施步骤

### 步骤 1：创建 meta/table.rs（含测试）

**从 cell.rs 迁移的内容：**
- 类型定义：`FieldType`, `ColumnDefine`, `IndexKind`, `IndexDefine`, `PartitionType`, `TableDefine`
- 测试代码：`test_fieldtype_serialization`, `test_fieldtype_deserialization`

**保留在 cell.rs 的内容：**
- 类型定义：`DataValue`, `Field`
- 测试代码：Binary、Array、Json、Uuid、DateTime/Date 相关测试
- ERP 场景集成测试

### 步骤 2：修改 cell.rs

1. **移除已迁移的类型定义**（约 180 行）
2. **移除已迁移的测试代码**（约 40 行）
3. **添加兼容重导出**：
   ```rust
   pub use model::meta::table::{
       FieldType, ColumnDefine, IndexKind, IndexDefine, PartitionType, TableDefine
   };
   ```

### 步骤 3：更新 meta/mod.rs

```rust
pub mod plugin;
pub mod table;  // 新增
```

### 步骤 4：验证编译

```bash
cargo build -p cmx-core
cargo build -p cmx-metadata
```

### 步骤 5：运行测试

```bash
cargo test -p cmx-core
cargo test -p cmx-metadata
```

### 步骤 6：检查所有依赖方

确保以下 crate 构建和测试通过：
- cmx-infra/cmx-database
- cmx-plugin

***

## 5. 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| 依赖方代码需修改 import | 低 | 使用兼容重导出，依赖方无感知 |
| 循环依赖 | 无 | cmx-core 不依赖 cmx-metadata |
| JSON 序列化兼容性 | 低 | 类型定义不变，仅移动位置 |
| 测试遗漏 | 低 | FieldType 测试完整迁移 |

***

## 6. 结论

**推荐迁移**，理由：

1. ✅ 符合代码内聚性原则：元数据类型应归元数据模块
2. ✅ 符合 meta 目录的设计目标：配置驱动的表结构定义
3. ✅ 风险可控：兼容层设计使改动透明
4. ✅ 提升可维护性：cell.rs 职责更单一
5. ✅ 测试同步迁移：FieldType 测试随类型一起迁移

**迁移范围：**

| 类别 | 内容 |
|------|------|
| 类型定义 | `FieldType`, `ColumnDefine`, `IndexKind`, `IndexDefine`, `PartitionType`, `TableDefine` |
| 测试代码 | FieldType 枚举序列化/反序列化测试（约 40 行） |

**保留在 cell.rs：**

| 类别 | 内容 |
|------|------|
| 类型定义 | `DataValue`, `Field` |
| 测试代码 | Binary、Array、Json、Uuid、DateTime/Date 测试 + ERP 场景集成测试（约 480 行） |
