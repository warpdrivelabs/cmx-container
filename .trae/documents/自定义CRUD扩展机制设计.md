# 自定义 CRUD 扩展机制设计

## 1. 设计目标

当默认的通用 CRUD 不满足需求时，开发者需要能够：
1. 扩展现有的 CRUD 方法
2. 添加自定义的业务方法
3. 覆盖默认的 CRUD 行为
4. 组合多个 Service

## 2. 目录结构建议

### 2.1 应用层目录结构（推荐）

开发者应在**应用层**创建自定义实现，而非修改 cmx-api 库：

```
your-app/
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── model/                    # 业务模型层
│   │   ├── mod.rs
│   │   └── domain/               # 按实体组织
│   │       ├── mod.rs
│   │       ├── entity.rs         # 实体定义
│   │       ├── filter.rs         # 过滤器定义
│   │       ├── bmc.rs            # DbBmc 实现
│   │       ├── service.rs        # 自定义 Service（扩展 CRUD）
│   │       └── handler.rs        # 自定义 Handler
│   ├── api/                      # API 层
│   │   ├── mod.rs
│   │   └── routes.rs             # 路由注册
│   └── config/
│       └── mod.rs
```

### 2.2 cmx-api 提供的扩展点

cmx-api 库提供以下扩展点：

```rust
// 1. GenericCrudService - 可继承扩展
pub struct GenericCrudService<MC, F> { ... }

// 2. DbBmc trait - 可实现自定义表元信息
pub trait DbBmc { ... }

// 3. Handler 函数 - 可自定义
pub async fn create<MC>(...) { ... }

// 4. 宏 - 可组合使用
register_crud_routes!(router, DomainBmc, DomainFilter, "/api/domains");
```

## 3. 扩展模式

### 3.1 模式一：继承扩展（推荐）

在应用层创建自定义 Service，继承 GenericCrudService：

```rust
// your-app/src/model/domain/service.rs
use cmx_api::{GenericCrudService, DbBmc, Result};
use cmx_database::DatabaseManager;
use cmx_core::model::data::dataset::DataSet;
use serde_json::Value;

/// 自定义 Domain Service
pub struct DomainService;

impl DomainService {
    /// 扩展：自定义业务方法
    pub async fn get_by_name(
        mm: &DatabaseManager,
        db_id: &str,
        name: &str,
    ) -> Result<DataSet> {
        // 使用 GenericCrudService 的 list 方法
        let filter = DomainFilter {
            name: Some(modql::filter::OpValsString(vec![
                modql::filter::OpValString::Eq(name.to_string())
            ])),
            ..Default::default()
        };
        GenericCrudService::<DomainBmc, DomainFilter>::list(
            mm, db_id, Some(filter), None
        ).await
    }

    /// 扩展：批量操作
    pub async fn batch_create(
        mm: &DatabaseManager,
        db_id: &str,
        items: Vec<Value>,
    ) -> Result<Vec<DataSet>> {
        let mut results = Vec::new();
        for item in items {
            let result = GenericCrudService::<DomainBmc>::create(mm, db_id, item).await?;
            results.push(result);
        }
        Ok(results)
    }

    /// 覆盖：自定义创建逻辑
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        mut data: Value,
    ) -> Result<DataSet> {
        // 添加自定义验证
        if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
            if name.len() < 3 {
                return Err(cmx_api::Error::bad_request("名称长度不能小于3"));
            }
        }
        
        // 调用父类方法
        GenericCrudService::<DomainBmc>::create(mm, db_id, data).await
    }
}
```

### 3.2 模式二：组合模式

组合多个 Service 实现复杂业务：

```rust
// your-app/src/model/domain/service.rs
use cmx_api::{GenericCrudService, DbBmc, Result};
use cmx_database::DatabaseManager;

/// 组合服务
pub struct DomainWithUserService {
    domain_service: std::marker::PhantomData<DomainBmc>,
    user_service: std::marker::PhantomData<UserBmc>,
}

impl DomainWithUserService {
    /// 获取域名及其所有者信息
    pub async fn get_domain_with_owner(
        mm: &DatabaseManager,
        db_id: &str,
        domain_id: &str,
    ) -> Result<serde_json::Value> {
        // 获取域名
        let domain = GenericCrudService::<DomainBmc>::get(
            mm, db_id, domain_id.into()
        ).await?;
        
        // 获取所有者
        let owner_id = domain.iter()
            .next()
            .and_then(|row| row.get("owner_id"))
            .and_then(|v| match v {
                cmx_core::model::cell::DataValue::String(s) => Some(s.clone()),
                _ => None,
            });
        
        let owner = if let Some(owner_id) = owner_id {
            Some(GenericCrudService::<UserBmc>::get(
                mm, db_id, owner_id.into()
            ).await?)
        } else {
            None
        };
        
        Ok(serde_json::json!({
            "domain": domain,
            "owner": owner
        }))
    }
}
```

### 3.3 模式三：完全自定义

对于完全自定义的需求：

```rust
// your-app/src/model/report/service.rs
use cmx_api::{Result, Error};
use cmx_database::DatabaseManager;
use cmx_core::model::data::dataset::DataSet;
use sea_query::{Query, PostgresQueryBuilder};
use sea_query_binder::SqlxBinder;

/// 完全自定义的报表服务
pub struct ReportService;

impl ReportService {
    /// 自定义复杂查询
    pub async fn get_sales_report(
        mm: &DatabaseManager,
        db_id: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<DataSet> {
        // 使用 sea-query 构建复杂 SQL
        let sql = format!(
            r#"
            SELECT 
                DATE(created_at) as date,
                COUNT(*) as total_orders,
                SUM(amount) as total_amount
            FROM orders
            WHERE created_at BETWEEN '{}' AND '{}'
            GROUP BY DATE(created_at)
            ORDER BY date
            "#,
            start_date, end_date
        );
        
        // 直接执行 SQL
        mm.query_sql(db_id, None, &sql).await
            .map_err(|e| Error::internal_error(format!("报表查询失败: {}", e)))
    }
}
```

## 4. 自定义 Handler

### 4.1 扩展 Handler

```rust
// your-app/src/model/domain/handler.rs
use axum::{extract::State, Json};
use cmx_api::{ApiResp, Result};
use cmx_database::DatabaseManager;
use crate::model::domain::service::DomainService;

/// 自定义 Handler：按名称查询
pub async fn get_by_name(
    State(mm): State<DatabaseManager>,
    Json(params): Json<serde_json::Value>,
) -> Result<Json<ApiResp<cmx_core::model::data::dataset::DataSet>>> {
    let db_id = params.get("db_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let name = params.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cmx_api::Error::bad_request("缺少 name 参数"))?;
    
    let dataset = DomainService::get_by_name(&mm, db_id, name).await?;
    Ok(Json(ApiResp::ok(dataset)))
}
```

### 4.2 注册自定义路由

```rust
// your-app/src/api/routes.rs
use axum::Router;
use cmx_database::DatabaseManager;
use cmx_api::register_crud_routes;
use crate::model::domain::{DomainBmc, DomainFilter};
use crate::model::domain::handler as domain_handler;

pub fn setup_routes() -> Router<DatabaseManager> {
    let router = Router::new();
    
    // 注册标准 CRUD 路由
    let router = register_crud_routes!(router, DomainBmc, DomainFilter, "/api/domains");
    
    // 添加自定义路由
    router
        .route("/api/domains/by-name", axum::routing::post(domain_handler::get_by_name))
        // 更多自定义路由...
}
```

## 5. 最佳实践

### 5.1 分层架构

```
┌─────────────────────────────────────┐
│           Handler 层                 │  ← 处理 HTTP 请求/响应
│   (your-app/src/model/*/handler.rs) │
├─────────────────────────────────────┤
│           Service 层                 │  ← 业务逻辑
│   (your-app/src/model/*/service.rs) │
├─────────────────────────────────────┤
│           Model 层                   │  ← 数据模型
│   (your-app/src/model/*/bmc.rs)     │
├─────────────────────────────────────┤
│           cmx-api                    │  ← 通用 CRUD 框架
│   (cmx-api crate)                    │
└─────────────────────────────────────┘
```

### 5.2 命名约定

| 组件 | 命名 | 示例 |
|-----|------|------|
| 实体 | 名词 | `Domain` |
| DbBmc | 实体 + Bmc | `DomainBmc` |
| Filter | 实体 + Filter | `DomainFilter` |
| Service | 实体 + Service | `DomainService` |
| Handler | 动作/操作 | `get_by_name`, `batch_create` |

### 5.3 错误处理

```rust
// 统一使用 cmx_api::Error
use cmx_api::{Error, Result};

pub async fn custom_method() -> Result<()> {
    // 参数验证错误
    if invalid_input {
        return Err(Error::bad_request("参数错误"));
    }
    
    // 内部错误
    database_operation()
        .map_err(|e| Error::internal_error(format!("操作失败: {}", e)))?;
    
    Ok(())
}
```

## 6. 完整示例

参见：`examples/custom-crud/` 目录
