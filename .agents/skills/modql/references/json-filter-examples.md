# JSON 过滤表达式示例 + 使用示例代码

> 本文件是 modql 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

JSON 过滤表达式示例 + 使用示例代码

## JSON 过滤表达式示例

### 单字段多条件（AND 关系）

```json
{
  "title": {
    "$startsWith": "Hello",
    "$contains": "World"
  },
  "done": false
}
```

### 多 Filter 组（OR 关系）

```json
{
  "filters": [
    {
      "id": {
        "$gt": 123
      },
      "title": {
        "$contains": "World"
      }
    },
    {
      "title": {
        "$startsWith": "Hello"
      }
    }
  ]
}
```

### 使用示例代码

```rust
use modql::filter::{FilterNodes, IntoFilterNodes, ListOptions};
use modql::SIden;
use sea_query::{Condition, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder;

// 1. 定义 Filter
#[derive(FilterNodes, Deserialize, Default, Debug)]
pub struct TaskFilter {
    id: Option<OpValsInt64>,
    title: Option<OpValsString>,
    done: Option<OpValsBool>,
}

// 2. 从 JSON 解析 Filter
let filter: TaskFilter = serde_json::from_value(json!({
    "title": {"$startsWith": "Hello", "$contains": "World"},
    "done": false
})) ?;

// 3. 转换为 FilterGroups
let filter_groups: modql::filter::FilterGroups = filter.filter_nodes(None).into();

// 4. 转换为 sea-query Condition
let cond: Condition = filter_groups.into_sea_condition() ?;

// 5. 构建查询
let mut query = Query::select();
query.from(SIden("task"));
query.columns(Task::sea_column_refs());
query.cond_where(cond);

// 6. 应用 ListOptions
let list_options: ListOptions = serde_json::from_value(json!({
    "offset": 0,
    "limit": 10,
    "order_bys": "!created_at"
})) ?;
list_options.apply_to_sea_query( & mut query);

// 7. 生成 SQL（sea-query-sqlx 0.9.1）
let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
```
