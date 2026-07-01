# Node → Rust 后端迁移：完成与切换说明

CMXPortalManager / CMXHTMLDesigner 的 Node.js 后端已全量迁移到 cmx-container（Rust / axum），
所有业务逻辑逐端点与 Node 对拍一致（数值 / md5 / 逐字段），并接入 JWT 认证。

## 新架构

- **统一后端**：`cmx-container` web-server（默认 `:8080`），业务在 `crates/libs/cmx-portal`，
  HTTP 在 `crates/libs/cmx-api/src/handlers/portal/`。
- **数据根**：`cmx-container/data/`（由 `dev-local.toml` 的 `[portal] data_root = "./data"` 指定；
  亦可用环境变量 `CMX_PORTAL_DATA_ROOT` 覆盖）。Portal 的 6.5M JSON 数据 + Designer 6 个独有页面
  + activities/portal + 两个 flat 菜单 + portal-overview 节点已全部并入。
- **认证**：所有 `/api/**` 业务端点受 `mw_auth` 保护；前端登录页 `login.html` + `lib/auth.js`
  + `api-client.js` 的全局 fetch 拦截器（自动带 Bearer token + 401 跳登录）。

## 启动

```bash
cd cmx-container
# .env 已设 CONFIG_FILE=./dev-local.toml；PG/Redis 需就绪（本地 Docker）
./target/debug/web-server          # 或 cargo run -p web-server
```

前端（两者 `/api` 默认代理到 `:8080`）：
```bash
cd CMXPortalManager && npm run dev   # CMX_VITE_BACKEND 可覆盖后端地址
cd CMXHTMLDesigner  && npm run dev
```

生产：cmx-container 可用 `ServeDir` 同源托管两个前端 `dist`（各自子路径）。

## 下线 Node

迁移与对拍完成后，两个 `cmx-node-server` 已无需运行，可停：
```bash
lsof -tiTCP:3000 -sTCP:LISTEN | xargs -r kill      # 停 Node
```
**状态：Node 已下线（2026-06）。** cmx-container `:8080` 作为唯一后端独立运行，
全量端点回归在 Node 停机后仍 5/5 通过，认证闭环（login→token→受保护端点 / 无 token 401）正常。

原 `CMXPortalManager/cmx-node-server/data` 与 `CMXHTMLDesigner/cmx-node-server/_data`
作为迁移源**原样保留为备份**，未删除。两前端 `package.json` 的 Node 后端启动脚本已改名
`legacy:start` / `legacy:dev:server` 并加 `_deprecated_note`，防止误启。

## 待补（低优先）

- ~~`service-catalog` 的 Bruno `.bru` 解析~~ **已完成**（`crates/libs/cmx-portal/src/service_catalog/`，
  零依赖 mini `.bru` 解析器 + DAM 分类 + `{{var}}` 展开；端点 `GET /api/service-catalog` 与 `/:id`）。
- `agent` 的 LlmPlanner（`CMX_AGENT_PLANNER=llm`）—— 当前默认 LocalRulePlanner（正则意图），
  已覆盖前端 agent 控制台的全部交互。

## 验证基线

- 后端：全量端点回归 10/10 通过（domains/menu(dam)/activities/dam-registry/definitions/dict/
  context-profile/html-pages(77)/fact/agent）。
- 前端：Playwright 实测两前端 登录→token→主应用挂载→零失败 API。
- 测试账号：`migtest / Test@1234`（经 `/api/iam/users/create` 创建）。
