# 事务模式 / DataSet 提取 / 完整示例

> 本文件是 cmx-sql-execution 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

## 六、事务模式

### 6.1 事务内执行 SQL

```rust
// 1. 开启事务
let txn_id = mm.get_transaction_context().begin(&db_id).await?;

// 2. 事务内执行(传 txn_id: Some)
let result = mm.execute_sql_with_datavalues(
    &db_id,
    Some(&txn_id),   // ★ 事务内执行
    "INSERT INTO cmx_permission (id, code) VALUES ($1, $2)",
    dv![id, code],
).await?;

// 3. 提交或回滚
match verify_result {
    Ok(_) => mm.commit_transaction(&txn_id).await?,
    Err(e) => {
        mm.rollback_transaction(&txn_id).await?;
        return Err(e);
    }
}
```

### 6.2 事务内查询

```rust
let dataset = mm.query_sql_with_datavalues(
    &db_id,
    Some(&txn_id),   // 事务内查询
    "SELECT id, code FROM cmx_permission WHERE domain_code = $1",
    dv![domain_code],
    "perm_scope",    // dataset_id(用于日志/调试)
).await?;
```

### 6.3 非事务执行

```rust
// txn_id: None → 自动提交
mm.execute_sql_with_datavalues(&db_id, None, sql, params).await?;
```

---

## 七、从 DataSet 提取结果

### 7.1 遍历行

```rust
let dataset = mm.query_sql_with_datavalues(&db_id, None, sql, params, "query_name").await?;
let schema = dataset.schema.as_ref();

for row in dataset.iter() {
    let id: String = row.get_by_name_as::<String>(schema, "id").unwrap_or_default();
    let name: Option<String> = row.get_by_name_as::<String>(schema, "name");
    let count: i64 = row.get_by_name_as::<i64>(schema, "count").unwrap_or(0);
}
```

### 7.2 提取单行(首行)

```rust
let row = dataset.iter().next()
    .ok_or_else(|| IamError::Business("记录不存在".into()))?;
let json_val = row.to_json_value(schema);
let permission: Permission = serde_json::from_value(json_val)?;
```

### 7.3 提取整列为 Vec

```rust
let ids: Vec<String> = dataset.iter()
    .filter_map(|row| row.get_by_name_as::<String>(schema, "id"))
    .collect();
```

---

## 八、完整示例:权限创建(事务 + DataValue + Option 糖)

```rust
use cmx_core::model::cell::DataValue;
use cmx_core::ParamsBuilder;

async fn create_permission(
    &self,
    txn_id: &str,
    data: &PermissionForCreate,
) -> Result<Permission, TraitError> {
    let id = cmx_utils::id::snowflake_id_str();
    let full_code_path = format!("/{}", data.code);
    let level = 1i64;

    let sql = "INSERT INTO cmx_permission \
               (id, code, name, resource_type, parent_id, sort_order, description, \
                domain_code, app_code, module_code, extension, status, archived, \
                parent_code, full_code_path, is_leaf, level) \
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 0, $13, $14, 1, $15)";

    // ★ 使用 .into() 糖,Option<String> 自动转 Null,Option<i64> 自动转 NullTyped(Int)
    let params = vec![
        DataValue::String(id),
        DataValue::String(data.code.clone()),
        DataValue::String(data.name.clone()),
        data.resource_type.clone().into(),      // Option<String> → String 或 Null
        data.parent_id.clone().into(),           // Option<String>
        data.sort_order.unwrap_or(0).into(),     // ★保留 None→0 语义
        data.description.clone().into(),         // Option<String>
        data.domain_code.clone().into(),
        data.app_code.clone().into(),
        data.module_code.clone().into(),
        data.extension.clone().into(),
        DataValue::Int(1),                       // status 默认 1
        parent_code.clone().into(),              // Option<String>
        DataValue::String(full_code_path),
        DataValue::Int(level),
    ];

    self.mm
        .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
        .await
        .map_err(|e| TraitError::Business(format!("新增权限失败: {e}")))?;

    // 查询返回
    let sql = "SELECT * FROM cmx_permission WHERE id = $1";
    let ds = self.mm
        .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, vec![DataValue::String(id)], "perm")
        .await?;
    extract_permission(ds)
}
```

## 九、完整示例:动态 UPDATE(ParamsBuilder)

```rust
use cmx_core::ParamsBuilder;
use cmx_core::model::cell::DataValue;

async fn update_rule(
    &self,
    rule_id: &str,
    data: UpdateRuleRequest,
) -> Result<(), TraitError> {
    // ★ ParamsBuilder 自动管理占位符,SET 从 $1 起
    let mut b = ParamsBuilder::new(0);
    b.set_opt("name", data.name)               // Option<String>
     .set_opt("priority", data.priority)       // Option<i64> → NullTyped(Int)
     .set_opt("status", data.status)           // Option<i64>
     .set_opt("description", data.description); // Option<String>
    let (set_clause, mut params) = b.build();

    if set_clause.is_empty() {
        return Err(TraitError::Business("未提供任何更新字段".into()));
    }

    // WHERE id 放最后
    let where_idx = params.len() + 1;
    params.push(DataValue::String(rule_id.to_string()));
    let sql = format!(
        "UPDATE cmx_rule SET {set_clause}, update_time = NOW() WHERE id = ${where_idx}"
    );

    self.mm.execute_sql_with_datavalues(&self.db_id, None, &sql, params).await?;
    Ok(())
}
```

---
