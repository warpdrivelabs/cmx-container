# 插件初始化数据（Seed Data）功能设计方案

## 一、需求概述

插件安装时，DDL 建表后表内无数据，部分业务表需要预置初始化数据。需要支持：
- 通过 CSV/JSON 文件定义初始化数据（非 SQL 格式）
- 自动生成 DML 语句并执行
- 支持 INSERT/UPDATE 的 upsert 语义
- 错误不阻断插件安装，统一收集后报告
- 执行完成后校验数据条数一致性

## 二、架构分析与设计决策

### 2.1 配置放置位置

**推荐方案：扩展 `TableDefinesConfig`，新增 `seed_data` 字段**

理由：
1. 种子数据天然依赖表先创建，与 `TableDefinesConfig` 的 `depends_on` 拓扑排序一致
2. 避免在 `PluginDefinition`/`manifest.json` 中引入新的顶层配置列表
3. 配置内聚：表定义 + 种子数据在同一个配置文件中管理

配置示例（`metadata/domain_app_module_config.json`）：
```json
{
  "name": "domain_app_module",
  "description": "域-应用-模块三层结构表定义",
  "depends_on": [],
  "priority": 0,
  "files": ["domain_app_module_tables.json"],
  "seed_data": [
    {
      "table_name": "cmx_domain_plugin",
      "file": "seeddata/domain_seed.json",
      "conflict_columns": ["code"],
      "enabled": true
    },
    {
      "table_name": "cmx_application_plugin",
      "file": "seeddata/application_seed.csv",
      "conflict_columns": ["domain_id", "code"],
      "enabled": true
    }
  ]
}
```

### 2.2 数据文件格式

**推荐 JSON 为主、CSV 为辅**，理由：项目全栈使用 JSON，类型表达力更强。

JSON 格式示例（`seeddata/domain_seed.json`）：
```json
[
  {
    "id": 1,
    "code": "FIN",
    "code_version": "1.0",
    "name": "财务域",
    "description": "财务管理相关业务",
    "type": "business",
    "sort_order": 1
  },
  {
    "id": 2,
    "code": "SCM",
    "code_version": "1.0",
    "name": "供应链域",
    "description": "供应链管理相关业务",
    "type": "business",
    "sort_order": 2
  }
]
```

CSV 格式示例（`seeddata/application_seed.csv`）：
```csv
id,domain_id,code,name,description,type,sort_order
1,1,FI,会计核算,财务会计核算应用,business,1
2,1,GL,总账,总账管理模块,business,2
```

### 2.3 DML 策略优化建议

**用户原始方案**：先 INSERT → 失败后 UPDATE → 再失败不报错

**优化方案**：使用 PostgreSQL 原生 `INSERT ... ON CONFLICT ... DO UPDATE`（UPSERT）

优势对比：

| 维度 | 用户方案（INSERT→UPDATE） | 优化方案（ON CONFLICT） |
|------|--------------------------|----------------------|
| 网络往返 | 最多 2 次/行 | 1 次/行 |
| 原子性 | 非原子，存在竞态 | 原子操作 |
| 错误处理 | 需捕获两次异常 | 无需异常捕获 |
| 性能 | 低（逐行重试） | 高（单次完成） |
| 代码复杂度 | 高 | 低 |

生成的 SQL 示例：
```sql
INSERT INTO "cmx_domain_plugin" ("id", "code", "code_version", "name", "description", "type", "sort_order")
VALUES (1, 'FIN', '1.0', '财务域', '财务管理相关业务', 'business', 1)
ON CONFLICT ("code") DO UPDATE SET
  "code_version" = EXCLUDED."code_version",
  "name" = EXCLUDED."name",
  "description" = EXCLUDED."description",
  "type" = EXCLUDED."type",
  "sort_order" = EXCLUDED."sort_order";
```

> 如果 `conflict_columns` 为空或不指定，则退化为纯 `INSERT`（不做冲突处理）。

### 2.4 批量执行策略

为了高性能，采用 **多行批量 INSERT** 代替逐行 INSERT：

```sql
INSERT INTO "cmx_domain_plugin" ("id", "code", "name", ...) 
VALUES 
  (1, 'FIN', '财务域', ...),
  (2, 'SCM', '供应链域', ...),
  (3, 'HR', '人力资源域', ...)
ON CONFLICT ("code") DO UPDATE SET
  "name" = EXCLUDED."name", ...;
```

- 默认批次大小：**100 行/批**（可配置）
- 超过批次大小的数据自动分批执行
- 每批独立收集错误，不中断后续批次

## 三、数据模型设计

### 3.1 新增 Rust 结构体

在 `cmx-core` 的 `PluginDefinition` 相关模型中新增：

```rust
/// 种子数据配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedDataConfig {
    /// 目标表名
    pub table_name: String,
    /// 数据文件路径（相对于插件根目录）
    pub file: String,
    /// 冲突检测列（用于 ON CONFLICT 子句）
    #[serde(default)]
    pub conflict_columns: Vec<String>,
    /// 是否启用（默认 true）
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }
```

### 3.2 扩展 TableDefinesConfig

在 `cmx-core` 的 `TableDefinesConfig` 中新增字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinesConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub files: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    /// 种子数据配置列表（新增）
    #[serde(default)]
    pub seed_data: Vec<SeedDataConfig>,
}
```

### 3.3 执行结果模型

```rust
/// 单表种子数据执行结果
#[derive(Debug, Clone)]
pub struct SeedDataTableResult {
    /// 目标表名
    pub table_name: String,
    /// 数据文件路径
    pub file_path: String,
    /// 文件中的数据条数
    pub file_row_count: usize,
    /// 成功执行的行数
    pub success_count: usize,
    /// 失败的行数
    pub failed_count: usize,
    /// 失败详情列表
    pub failures: Vec<SeedDataFailure>,
    /// 数据库中的实际行数（执行后查询）
    pub db_row_count: Option<usize>,
}

/// 单条种子数据执行失败记录
#[derive(Debug, Clone)]
pub struct SeedDataFailure {
    /// 行号（从 1 开始，CSV 行号或 JSON 数组索引）
    pub row_index: usize,
    /// 行数据（JSON Value 格式）
    pub row_data: serde_json::Value,
    /// 错误信息
    pub error_message: String,
}

/// 全部种子数据执行汇总结果
#[derive(Debug, Clone)]
pub struct SeedDataSummary {
    /// 各表执行结果
    pub table_results: Vec<SeedDataTableResult>,
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
}

impl SeedDataSummary {
    /// 是否有错误
    pub fn has_errors(&self) -> bool {
        self.table_results.iter().any(|r| r.failed_count > 0)
    }

    /// 是否有数据条数不一致的警告
    pub fn has_warnings(&self) -> bool {
        self.table_results.iter().any(|r| {
            r.db_row_count.map_or(false, |db_count| db_count < r.file_row_count)
        })
    }
}
```

## 四、模块设计

### 4.1 新增模块位置

在 `cmx-metadata` crate 中新增 `seed` 模块：

```
crates/libs/cmx-metadata/src/
├── seed/
│   ├── mod.rs           # 模块入口，re-exports
│   ├── config.rs        # 种子数据配置解析（SeedDataConfig 解析）
│   ├── loader.rs        # 数据文件加载（JSON/CSV → Vec<serde_json::Value>）
│   ├── dml.rs           # DML 语句生成（PostgreSQL UPSERT）
│   └── executor.rs      # DML 执行器（批量执行 + 错误收集 + 数据校验）
```

### 4.2 模块职责

| 模块 | 职责 |
|------|------|
| `seed/config` | 从 `TableDefinesConfig` 提取 `SeedDataConfig`，校验配置合法性 |
| `seed/loader` | 读取并解析 JSON/CSV 数据文件，统一转为 `Vec<serde_json::Value>` |
| `seed/dml` | 根据 `TableDefine` 列定义 + 数据行，生成 PG UPSERT SQL |
| `seed/executor` | 批量执行 DML、收集错误、校验数据条数、输出日志 |

### 4.3 核心接口设计

```rust
/// 种子数据执行器
pub struct PgSeedDataExecutor {
    /// 数据库ID
    db_id: String,
    /// 事务ID（可选）
    txn_id: Option<String>,
    /// 批次大小
    batch_size: usize,
}

impl PgSeedDataExecutor {
    pub fn new(db_id: impl Into<String>, txn_id: Option<String>) -> Self;

    /// 执行单表的种子数据初始化
    ///
    /// # 参数
    /// - `table_define`: 目标表的完整定义（用于列类型映射）
    /// - `seed_config`: 种子数据配置
    /// - `base_path`: 插件安装根路径
    ///
    /// # 返回
    /// 执行结果，包含成功/失败统计和错误详情
    pub async fn execute_seed_data(
        &self,
        table_define: &TableDefine,
        seed_config: &SeedDataConfig,
        base_path: &Path,
    ) -> SeedDataTableResult;

    /// 批量执行多个表的种子数据
    pub async fn execute_all_seed_data(
        &self,
        table_defines: &[TableDefine],
        seed_configs: &[SeedDataConfig],
        base_path: &Path,
    ) -> SeedDataSummary;

    /// 校验数据条数
    async fn verify_row_count(
        &self,
        table_name: &str,
        expected_count: usize,
    ) -> Option<usize>;
}
```

### 4.4 DML 生成核心逻辑

```rust
/// 生成 PostgreSQL UPSERT 语句
///
/// 使用 INSERT ... ON CONFLICT ... DO UPDATE 语法
pub fn generate_pg_upsert(
    table_def: &TableDefine,
    rows: &[serde_json::Value],
    conflict_columns: &[String],
    batch_index: usize,
    batch_size: usize,
) -> Result<String, MetadataError>
```

关键实现要点：
1. **列名映射**：从 `TableDefine.columns` 获取列名列表和类型信息
2. **值转义**：根据列的 `field_type` 进行类型安全的值转换
3. **UPSERT 生成**：`conflict_columns` 非空时生成 `ON CONFLICT ... DO UPDATE SET` 子句
4. **批量拼接**：多行 VALUES 用逗号连接，一次执行
5. **EXCLUDED 引用**：UPDATE SET 子句使用 `EXCLUDED.列名` 引用新值

### 4.5 CSV 解析

使用 `csv` crate（需新增依赖）：

```rust
/// 从 CSV 文件加载种子数据
///
/// 首行作为列名（表头），后续行为数据行
/// 所有值统一转为 serde_json::Value
pub fn load_seed_data_from_csv(path: &Path) -> Result<Vec<serde_json::Value>, MetadataError>
```

类型推断规则（CSV 所有值都是字符串，需要根据 `TableDefine.columns` 的 `field_type` 转换）：

| FieldType | 转换规则 |
|-----------|---------|
| Int | `parse::<i64>()` |
| Float | `parse::<f64>()` |
| Bool | `"true"/"1"` → true, `"false"/"0"` → false |
| Decimal | 原始字符串 |
| Date/DateTime | 原始字符串（PG 可自动转换） |
| 其他 | 原始字符串 |

### 4.6 错误处理与日志策略

执行流程：

```
for each seed_config:
  1. 加载数据文件 → 失败则记录到 SeedDataTableResult.failures，跳过该表
  2. 分批生成 DML:
     for each batch:
       a. 生成 UPSERT SQL
       b. 执行 SQL
       c. 成功 → success_count += batch.len()
       d. 失败 → 降级为逐行执行:
          for each row in batch:
            i.  生成单行 UPSERT SQL
            ii. 执行
            iii. 成功 → success_count += 1
            iv. 失败 → 记录到 failures，failed_count += 1
  3. 查询 SELECT COUNT(*) FROM table → 记录 db_row_count
  4. 比对 file_row_count vs db_row_count:
     - db_count < file_count → 输出 warn! 日志
     - db_count > file_count → 输出 info! 日志（正常，可能已有历史数据）
     - db_count == file_count → 输出 debug! 日志

最终汇总输出：
  info!("种子数据执行完成: {} 表, {} 成功, {} 失败, 耗时 {}ms",
        summary.table_results.len(),
        total_success,
        total_failed,
        summary.total_duration_ms);
```

## 五、集成到插件安装流程

### 5.1 安装流程变更

在 `create_plugin_tables` 函数中，DDL 执行完成后、保存元数据之前，新增种子数据执行步骤：

```
原有流程：
  加载表配置 → 解析表定义 → DDL生成执行 → 保存元数据

新增流程：
  加载表配置 → 解析表定义 → DDL生成执行 → ★种子数据执行★ → 保存元数据
```

### 5.2 修改的文件清单

| 文件 | 修改内容 |
|------|---------|
| `cmx-core/src/model/meta/plugin.rs` | `TableDefinesConfig` 新增 `seed_data` 字段；新增 `SeedDataConfig` 结构体 |
| `cmx-metadata/src/lib.rs` | 新增 `pub mod seed;` 和相关 re-exports |
| `cmx-metadata/Cargo.toml` | 新增 `csv` 依赖 |
| `cmx-metadata/src/seed/mod.rs` | 新建：模块入口 |
| `cmx-metadata/src/seed/config.rs` | 新建：种子数据配置解析 |
| `cmx-metadata/src/seed/loader.rs` | 新建：JSON/CSV 数据文件加载 |
| `cmx-metadata/src/seed/dml.rs` | 新建：PostgreSQL DML 生成 |
| `cmx-metadata/src/seed/executor.rs` | 新建：批量执行器 + 错误收集 + 校验 |
| `cmx-metadata/src/config.rs` | `load_all_tables` 后增加种子数据收集方法 |
| `cmx-plugin/src/service/utils.rs` | `create_plugin_tables` 中新增种子数据执行调用 |

### 5.3 关键代码变更示意

#### `cmx-plugin/src/service/utils.rs`

```rust
pub async fn create_plugin_tables(
    db_id: &str,
    plugin_id: &str,
    version: &str,
    install_path: &Path,
    plugin_define: &PluginDefinition,
    txn_id: Option<&str>,
) -> PluginResult<Vec<TableDefine>> {
    // ... 原有 DDL 逻辑不变 ...

    // ★ 新增：执行种子数据初始化
    let seed_executor = PgSeedDataExecutor::new(db_id, None);
    let all_seed_configs = table_config_manager.collect_seed_configs();
    
    if !all_seed_configs.is_empty() {
        let summary = seed_executor
            .execute_all_seed_data(&table_defs, &all_seed_configs, install_path)
            .await;
        
        // 输出汇总日志（不阻断安装）
        tracing::info!(
            "插件 {} 种子数据执行完成: {} 表处理, {} 成功, {} 失败, 耗时 {}ms",
            plugin_id,
            summary.table_results.len(),
            summary.table_results.iter().map(|r| r.success_count).sum::<usize>(),
            summary.table_results.iter().map(|r| r.failed_count).sum::<usize>(),
            summary.total_duration_ms,
        );

        // 输出错误详情
        for result in &summary.table_results {
            for failure in &result.failures {
                tracing::error!(
                    "种子数据执行失败: 表={}, 行={}, 错误={}",
                    result.table_name,
                    failure.row_index,
                    failure.error_message,
                );
            }
            // 数据条数校验警告
            if let Some(db_count) = result.db_row_count {
                if db_count < result.file_row_count {
                    tracing::warn!(
                        "种子数据条数不一致: 表={}, 文件={}条, 数据库={}条",
                        result.table_name,
                        result.file_row_count,
                        db_count,
                    );
                }
            }
        }
    }

    // ... 原有保存元数据逻辑不变 ...
}
```

## 六、插件包目录结构（扩展后）

```
plugindemo/
├── manifest.json
├── wasmtest.wasm
├── api/
│   └── api.json
├── metadata/
│   ├── domain_app_module_config.json    # 扩展 seed_data 字段
│   └── domain_app_module_tables.json
├── seeddata/                            # 新增：种子数据目录
│   ├── domain_seed.json
│   ├── application_seed.csv
│   └── module_seed.json
├── servicedata/
│   └── sample-flow.json
└── wit/
    └── plugin.wit
```

## 七、实施步骤（按优先级排序）

### 步骤 1：扩展数据模型（cmx-core）
1. 在 `plugin.rs` 中新增 `SeedDataConfig` 结构体
2. 在 `TableDefinesConfig` 中新增 `seed_data: Vec<SeedDataConfig>` 字段
3. 确保向后兼容（`#[serde(default)]`）

### 步骤 2：新建种子数据模块（cmx-metadata）
1. 创建 `seed/mod.rs`、`seed/config.rs`
2. 创建 `seed/loader.rs`（JSON 加载 + CSV 加载）
3. 创建 `seed/dml.rs`（PG UPSERT SQL 生成）
4. 创建 `seed/executor.rs`（批量执行 + 错误收集 + 校验）
5. 在 `cmx-metadata/Cargo.toml` 添加 `csv` 依赖
6. 在 `cmx-metadata/src/lib.rs` 中注册模块

### 步骤 3：集成到安装流程（cmx-plugin）
1. 在 `cmx-metadata/src/config.rs` 中新增 `collect_seed_configs()` 方法
2. 修改 `cmx-plugin/src/service/utils.rs` 的 `create_plugin_tables` 函数
3. 在 DDL 执行完成后调用种子数据执行器

### 步骤 4：测试与示例
1. 在 `plugindemo` 目录下创建示例种子数据文件
2. 更新 `domain_app_module_config.json` 添加 `seed_data` 配置
3. 编写单元测试和集成测试

## 八、风险与注意事项

1. **向后兼容**：`seed_data` 字段使用 `#[serde(default)]`，旧插件无此字段不影响
2. **幂等性**：UPSERT 天然幂等，重复安装不会产生重复数据
3. **性能**：批量 INSERT（100行/批）+ ON CONFLICT，预计万行级数据秒级完成
4. **DDL 不在事务中**：种子数据 DML 也不应在主事务中执行（与 DDL 保持一致）
5. **升级场景**：插件升级时重新执行种子数据，UPSERT 会更新已有数据、插入新数据
6. **CSV 编码**：建议统一使用 UTF-8 编码，CSV 解析时显式指定
7. **数据类型安全**：CSV 值均为字符串，需根据 `TableDefine.columns` 的 `field_type` 做类型转换，转换失败的值记录为错误
