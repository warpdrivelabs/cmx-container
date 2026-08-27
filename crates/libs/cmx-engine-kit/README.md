# cmx-engine-kit

引擎应用层**请求期横切件**单源——"中立核们的中立核"。五引擎（flow / rules / model / mdm / report）平台中立应用层的通用件收编处，消除各仓 `cmx-*-app` 手写副本。

## 与 cmx-service-base 的分工

按**生命周期**分家、互不依赖：

| | cmx-service-base | cmx-engine-kit（本 crate） |
| --- | --- | --- |
| 执行时机 | 启动期一次（起服原语：init_infra / BaseConfig / register_pg_datasources） | 每 HTTP 请求（中间件 / 上下文 / 路由 / 信封） |
| 有无 axum | 无 | 有 |
| 主要消费方 | 引擎 server 壳 main.rs | 引擎 app 核路由 / handlers |

## 模块

| 模块 | 内容 | 收编来源 |
| --- | --- | --- |
| `tenant` | `TenantCtx` + task_local scope + `current_tenant/current_user/current_display_user/current_display_nickname/current_roles/identity_snapshot` | cmx-flow-app / cmx-rule-app 同源副本（flow 版含 nickname 管道为蓝本） |
| `dbid` | `resolve_db_id(_from_headers)`：`db_id` 头显式优先，缺失回退 biz 库 | cmx-model-app / cmx-mdm-app 逐字节相同副本 |
| `auth` | 两族认证中间件：族 A `delegated`（委托令牌 → `AuthContext`，model/mdm 形态）与族 B `jwt`（JWT/API-Key → `TenantCtx`，flow 超集语义）——按方案 P2/P3 陆续迁入；P1 已收编 `common::unauthorized` 401 响应体 | 四仓 auth.rs |
| `resp`（P4） | `ApiResp`/`Error`/`Result` 的 cmx-api-types 别名 | flow / rule resp.rs |

## 依赖边界

**依赖**：cmx-core / cmx-traits / cmx-utils / cmx-database-pg / cmx-web-monitor / cmx-api-types(light) + jsonwebtoken / axum。
**不依赖**：cmx-auth（引擎保持 database-pg 新链路，不拖 sqlx 栈；未来需平台令牌能力时经 `auth::decode` 演进开关切换）、cmx-form、cmx-web-chassis、cmx-service-base。

## 设计决策

落点裁决、四项拍板决策与分阶段实施清单见方案文档：`documents/plans/20260826_cmx-container_五引擎应用层通用代码去重抽取方案.md`（工作区根）。
