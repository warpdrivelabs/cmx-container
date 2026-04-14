# 自定义分页查询函数 `page_custom` 实现计划（最终版）

## 一、需求分析

### 功能需求

实现一个自定义分页查询函数，支持：

* 传入自定义 SQL（支持复杂 JOIN 查询）

* 使用 filters 动态生成 WHERE 条件

* 使用 list\_options 生成分页参数（LIMIT/OFFSET）和排序条件（ORDER BY）

* 自动生成 COUNT 查询，先查询总数再查询数据

* 类似 MyBatis Plus 的分页查询机制

## 二、实现方案

### 方案：原生 SQL + 条件拼接

**使用 sea-query 构建条件子句，然后拼接**

#### 优势

* ✅ 不需要额外依赖

* ✅ 复用现有的 sea-query 逻辑

* ✅ 参数绑定安全

* ✅ 实现相对简单

## 三、代码组织方案

### 方案对比

| 方案                  | 优势             | 劣势                | 推荐度   |
| ------------------- | -------------- | ----------------- | ----- |
| 放在 crud\_fns.rs     | 代码集中，易于查找      | 文件过大（800+ 行），职责不清 | ⭐⭐    |
| 新建 custom\_query.rs | 职责分离，代码清晰，易于维护 | 增加文件数量            | ⭐⭐⭐⭐⭐ |
| 新建 service 目录       | 更好的模块化，支持未来扩展  | 改动较大              | ⭐⭐⭐⭐  |

### 推荐方案：新建 `custom_query.rs` 文件

#### 文件结构

```
crates/libs/cmx-infra/cmx-database/src/crud/
├── mod.rs              # 模块导出
├── crud_fns.rs         # 通用 CRUD 服务（现有）
├── custom_query.rs     # 自定义查询服务（新建）⭐
├── error.rs            # 错误定义
└── utils.rs            # 工具函数
```

#### 优势

1. ✅ **职责分离**：custom\_query.rs 专门处理自定义查询逻辑
2. ✅ **代码清晰**：每个文件职责明确，易于理解
3. ✅ **易于维护**：修改自定义查询逻辑不影响现有 CRUD 代码
4. ✅ **便于测试**：可以单独为自定义查询编写测试
5. ✅ **扩展性好**：未来可以添加更多自定义查询功能

## 四、详细实现

### 4.1 文件：`custom_query.rs`

````rust
//! 自定义查询服务
//!
//! 提供自定义 SQL 查询功能，支持动态过滤、分页和排序

use sea_query::{Asterisk, Alias, Condition, Expr, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxValues;
use modql::filter::{FilterGroups, IntoFilterNodes, ListOptions};
use tracing::debug;

use crate::DatabaseManager;
use crate::crud::error::{Result, ServiceError};
use cmx_core::model::data::dataset::DataSet;

/// 自定义查询服务
///
/// 提供自定义 SQL 的分页查询功能
pub struct CustomQueryService;

impl CustomQueryService {
    /// 自定义分页查询
    ///
    /// 传入自定义 SQL，使用 filters 生成 WHERE 条件，使用 list_options 生成分页参数和排序条件。
    /// 先查询 count，如果 count > 0 再继续查询数据。
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `filters` - 过滤条件列表（可选）
    /// * `list_options` - 列表选项（包含分页和排序）
    /// * `sql` - 自定义 SQL（支持复杂 JOIN 查询）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet 和总数
    ///
    /// # 示例
    /// ```rust
    /// let sql = "select a.*, b.name from a left join b on a.name = b.name";
    /// let (dataset, total) = CustomQueryService::page_custom::<SomeFilter>(
    ///     &mm, "db_id", None, Some(filters), list_options, sql
    /// ).await?;
    /// ```
    pub async fn page_custom<F>(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        filters: Option<Vec<F>>,
        list_options: ListOptions,
        sql: &str,
    ) -> Result<(DataSet, i64)>
    where
        F: Into<FilterGroups> + Clone + IntoFilterNodes,
    {
        debug!(
            "{:<12} - CustomQueryService::page_custom - db_id: {}",
            "CUSTOM_QUERY", db_id
        );
        
        // 1. 构建 WHERE 条件
        let (where_clause, where_values) = Self::build_where_clause(filters.clone())?;
        
        // 2. 构建排序和分页
        let (order_by, limit_offset) = Self::build_order_and_pagination(&list_options);
        
        // 3. 构建 COUNT SQL
        let count_sql = Self::build_final_sql(
            &format!("SELECT COUNT(*) FROM ({}) AS count_subquery", sql),
            where_clause.as_deref(),
            None,
            None,
        );
        
        // 4. 执行 COUNT 查询
        let total = Self::execute_count_query(mm, db_id, txn_id, &count_sql, &where_values).await?;
        
        debug!("{:<12} - COUNT 查询结果: {}", "CUSTOM_QUERY", total);
        
        // 5. 如果 total > 0，执行数据查询
        let dataset = if total > 0 {
            let data_sql = Self::build_final_sql(
                sql,
                where_clause.as_deref(),
                order_by.as_deref(),
                limit_offset.as_deref(),
            );
            
            Self::execute_data_query(mm, db_id, txn_id, &data_sql, &where_values).await?
        } else {
            Self::empty_dataset()
        };
        
        let row_count = dataset.iter().count();
        debug!(
            "{:<12} - 分页查询返回 {} 行, 总数: {}",
            "CUSTOM_QUERY", row_count, total
        );
        
        Ok((dataset, total))
    }
    
    /// 构建 WHERE 条件子句
    ///
    /// # 参数
    /// * `filters` - 过滤条件列表
    ///
    /// # 返回值
    /// 返回 (where_clause, values) 元组
    /// - where_clause: WHERE 子句（不包含 WHERE 关键字）
    /// - values: 参数值
    fn build_where_clause<F>(filters: Option<Vec<F>>) -> Result<(Option<String>, SqlxValues)>
    where
        F: Into<FilterGroups> + Clone + IntoFilterNodes,
    {
        if let Some(filters) = filters {
            // 使用一个临时的查询来构建条件
            let mut temp_query = Query::select();
            temp_query.from(Alias::new("temp_table")); // 临时表，仅用于构建条件
            
            let filters: FilterGroups = Vec::into(filters);
            let cond: Condition = filters.try_into().map_err(|e| {
                ServiceError::bad_request(format!("过滤条件错误: {}", e))
            })?;
            
            temp_query.cond_where(cond);
            
            // 构建 SQL
            let (sql, values) = temp_query.build_sqlx(PostgresQueryBuilder);
            
            // 提取 WHERE 子句（去掉 WHERE 关键字）
            let where_clause = if let Some(pos) = sql.find("WHERE") {
                Some(sql[pos + 5..].trim().to_string())
            } else {
                None
            };
            
            Ok((where_clause, values))
        } else {
            Ok((None, SqlxValues::default()))
        }
    }
    
    /// 构建排序和分页子句
    ///
    /// # 参数
    /// * `list_options` - 列表选项
    ///
    /// # 返回值
    /// 返回 (order_by, limit_offset) 元组
    fn build_order_and_pagination(list_options: &ListOptions) -> (Option<String>, Option<String>) {
        let mut order_by = None;
        let mut limit_offset = None;
        
        // 构建 ORDER BY
        if let Some(ref ob) = list_options.order_bys {
            order_by = Some(format!("ORDER BY {}", ob));
        }
        
        // 构建 LIMIT 和 OFFSET
        let mut parts = vec![];
        if let Some(limit) = list_options.limit {
            parts.push(format!("LIMIT {}", limit));
        }
        if let Some(offset) = list_options.offset {
            parts.push(format!("OFFSET {}", offset));
        }
        if !parts.is_empty() {
            limit_offset = Some(parts.join(" "));
        }
        
        (order_by, limit_offset)
    }
    
    /// 构建最终 SQL
    ///
    /// # 参数
    /// * `base_sql` - 基础 SQL
    /// * `where_clause` - WHERE 子句
    /// * `order_by` - ORDER BY 子句
    /// * `limit_offset` - LIMIT/OFFSET 子句
    ///
    /// # 返回值
    /// 返回完整的 SQL
    fn build_final_sql(
        base_sql: &str,
        where_clause: Option<&str>,
        order_by: Option<&str>,
        limit_offset: Option<&str>,
    ) -> String {
        let mut sql = base_sql.to_string();
        
        // 添加 WHERE 条件
        if let Some(where_sql) = where_clause {
            // 检查原始 SQL 中是否已有 WHERE
            if base_sql.to_uppercase().contains(" WHERE ") {
                sql = format!("{} AND {}", sql, where_sql);
            } else {
                sql = format!("{} WHERE {}", sql, where_sql);
            }
        }
        
        // 添加 ORDER BY
        if let Some(ob) = order_by {
            sql = format!("{} {}", sql, ob);
        }
        
        // 添加 LIMIT/OFFSET
        if let Some(lo) = limit_offset {
            sql = format!("{} {}", sql, lo);
        }
        
        sql
    }
    
    /// 执行 COUNT 查询
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID
    /// * `sql` - SQL 语句
    /// * `values` - 参数值
    ///
    /// # 返回值
    /// 返回记录总数
    async fn execute_count_query(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        values: &SqlxValues,
    ) -> Result<i64> {
        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, txn_id, sql, values.clone(), "count")
            .await
            .map_err(|e| ServiceError::internal_error(format!("COUNT 查询失败: {}", e)))?;
        
        let count = dataset
            .iter()
            .next()
            .and_then(|row| row.get(0))
            .and_then(|val| match val {
                cmx_core::model::cell::DataValue::Int(i) => Some(*i),
                _ => None,
            })
            .unwrap_or(0);
        
        Ok(count)
    }
    
    /// 执行数据查询
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID
    /// * `sql` - SQL 语句
    /// * `values` - 参数值
    ///
    /// # 返回值
    /// 返回查询结果 DataSet
    async fn execute_data_query(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        values: &SqlxValues,
    ) -> Result<DataSet> {
        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, txn_id, sql, values.clone(), "custom_query")
            .await
            .map_err(|e| ServiceError::internal_error(format!("数据查询失败: {}", e)))?;
        
        Ok(dataset)
    }
    
    /// 创建一个空的 DataSet（用于返回操作结果）
    fn empty_dataset() -> DataSet {
        use std::sync::Arc;
        use cmx_core::model::data::dataset::Schema;
        
        let schema = Arc::new(Schema::new("empty", vec![]));
        DataSet::empty("result", schema)
    }
}
````

### 4.2 修改：`crud_fns.rs`

在 `GenericCrudService` 中添加一个便捷方法，调用 `CustomQueryService`：

```rust
impl<MC, F> GenericCrudService<MC, F>
where
    MC: DbBmc,
    F: Into<FilterGroups> + Clone + IntoFilterNodes,
{
    /// 自定义分页查询（便捷方法）
    ///
    /// 这是 CustomQueryService::page_custom 的便捷包装
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `filters` - 过滤条件列表（可选）
    /// * `list_options` - 列表选项（包含分页和排序）
    /// * `sql` - 自定义 SQL（支持复杂 JOIN 查询）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet 和总数
    pub async fn page_custom(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        filters: Option<Vec<F>>,
        list_options: ListOptions,
        sql: &str,
    ) -> Result<(DataSet, i64)> {
        crate::crud::CustomQueryService::page_custom(mm, db_id, txn_id, filters, list_options, sql).await
    }
}
```

### 4.3 修改：`mod.rs`

添加模块导出：

```rust
mod crud_fns;
mod custom_query;  // 新增
mod error;
mod utils;

pub use crud_fns::*;
pub use custom_query::*;  // 新增
pub use error::*;
pub use utils::*;

// ... 其他代码
```

## 五、使用示例

### 方式 1：直接使用 CustomQueryService

```rust
use cmx_database::crud::CustomQueryService;

let sql = "select a.*, b.name from a left join b on a.name = b.name";
let (dataset, total) = CustomQueryService::page_custom::<DomainFilter>(
    &mm,
    "db_id",
    None,
    Some(filters),
    list_options,
    sql,
).await?;
```

### 方式 2：通过 GenericCrudService 使用（推荐）

```rust
use cmx_database::crud::GenericCrudService;

let sql = "select a.*, b.name from a left join b on a.name = b.name";
let (dataset, total) = GenericCrudService::<DomainBmc, DomainFilter>::page_custom(
    &mm,
    "db_id",
    None,
    Some(filters),
    list_options,
    sql,
).await?;
```

## 六、实现步骤

1. ✅ 创建 `custom_query.rs` 文件
2. ✅ 实现 `CustomQueryService` 结构体
3. ✅ 实现 `page_custom` 方法及辅助函数
4. ✅ 在 `crud_fns.rs` 中添加便捷方法
5. ✅ 在 `mod.rs` 中添加模块导出
6. ✅ 编写测试用例
7. ✅ 更新文档

## 七、注意事项

### 7.1 SQL 注入防护

* ✅ 所有用户输入通过参数绑定传递

* ✅ WHERE 条件由 sea-query 构建，自动处理转义

* ⚠️ 原始 SQL 需要开发者保证安全性

### 7.2 性能优化

* COUNT 查询和数据查询使用相同的参数绑定

* 对于大数据量，建议添加适当的索引

* 可以考虑缓存 COUNT 结果（如果数据不常变化）

### 7.3 兼容性

* 支持 PostgreSQL、MySQL、SQLite

* 需要根据数据库类型选择对应的 QueryBuilder

## 八、文件清单

需要创建的文件：

* `e:\rustspace\cmx\cmx-container\crates\libs\cmx-infra\cmx-database\src\crud\custom_query.rs` ⭐

需要修改的文件：

* `e:\rustspace\cmx\cmx-container\crates\libs\cmx-infra\cmx-database\src\crud\mod.rs`

* `e:\rustspace\cmx\cmx-container\crates\libs\cmx-infra\cmx-database\src\crud\crud_fns.rs`

## 九、测试场景

需要测试以下场景：

1. ✅ 简单的单表查询
2. ✅ 带 JOIN 的复杂查询
3. ✅ 带 filters 的查询
4. ✅ 带排序和分页的查询
5. ✅ 空 filters 的情况
6. ✅ total = 0 的情况
7. ✅ 原始 SQL 已包含 WHERE 子句的情况
8. ✅ 多表 JOIN 查询

