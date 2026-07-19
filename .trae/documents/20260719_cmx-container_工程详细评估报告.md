# cmx-container 工程详细评估报告

> 评估日期 2026-07-19 · 基于实读代码与配置，非推测。工作区根目录已有一份 2026-06-27 的《评估报告v2.html》（91 分），本报告在其基线上做**增量复核**。

## 一、工程画像

| 指标 | 数值 | 对比 v2（3 周前） |
|---|---|---|
| Rust 代码量 | **约 21.3 万行** | 13.1 万 → +62% |
| workspace crate | **44 个**（含 tests/demo） | 28 个 → +16 |
| 测试函数 | **1176 个**（162 个文件）+ pytest e2e 17 个模块 | 806 → +46% |
| Git 提交 | 914 次，**近 30 天 301 次** | 715 → 开发极活跃 |
| SQL 资产 | `init_ddl.sql` 2885 行 / **64 张表**，48 个增量迁移 | — |
| 构建 | `cargo check --workspace` **通过**（增量 1m16s） | ✓ 保持 |
| Edition / 版本 | 2024（Rust 1.85+）/ 0.1.12 | — |

**定位**：插件化容器运行时，单二进制承载 —— Axum HTTP + Extism WASM 插件热插拔 + 声明式服务编排 + 自研 BPMN 流程引擎 + 报表公式引擎 + IAM/认证/审计 + Nacos 注册配置 + volo gRPC。本质是一个**低代码/业务平台的完整后端底座**。

## 二、架构评估（强项）

分层干净，依赖方向清晰，是工程最好的部分：

```
web-server → cmx-api(26K) → cmx-biz/iam/plugin/portal/model/form
           → cmx-flow-*(10K) / cmx-rpt-*(4.3K) / cmx-ai
           → cmx-infra/*(45.7K: db/buffer/storage/rpc/auth/audit)
           → cmx-core / cmx-traits / cmx-utils（零业务基础层）
```

- **Trait 解耦落实到位**：`cmx-auth` 不依赖 `cmx-iam`，经 `cmx-traits::{UserAuthQuery, PermissionChecker}` 在 `cmx-biz` 注入。抽查验证属实。
- **集群插件同步设计正确**：单一写入原则（仅 API 节点写 DB）+ Redis Pub/Sub 通知 + 60s 幂等 reconciliation 补偿。`plugin_sync.rs` 注释明确"只做运行时同步、不操作 DB、天然幂等、无需分布式锁"——符合根 AGENTS.md 的无状态/集群约束，多节点并发安全。
- **双数据库层**：`cmx-database`（sqlx，多库）+ `cmx-database-pg`（tokio-postgres，PG 专用高性能 + 零拷贝 rowsource），职责分离合理。
- **全局内存状态克制**：`OnceCell/LazyLock` 全工程仅约 8 处、DashMap 4 处，均为注册表/会话注册等合理用途，未见违规缓存业务数据。
- **自研能力内核完整**：BPMN 2.0 XML 编译器（roxmltree）+ 令牌执行内核（等待态即提交点）、报表 DSL 公式引擎，语义中立、无 DB 依赖的内核/壳分离做得规范。

## 三、规范执行（AGENTS.md 18 章对照）

执行**良好**的：thiserror 全覆盖、tracing 统一（无 `log` crate）、workspace 依赖集中 + 逐行注释、`cmx_` 表前缀、迁移命名规范、`workspace = true` 引用、crate README 覆盖 16/17。

**违规与退步项**：

1. **🔴 `.env` 已被 git 追踪且含敏感键**（严重）。`.gitignore` 里 `#.env` 被注释掉，`git ls-files` 确认 `.env` 在库内，含 `NACOS_PASSWORD`、`NACOS_USERNAME`、`TEST_DATABASE_URL` 等键名，最近提交 2026-07-14。这直接违反项目自身 4.1 硬规则，密钥已进入 git 历史，**建议立即 `git rm --cached .env` + 恢复 gitignore + 轮换已泄露凭据**。
2. **🟡 handler 手写 SQL 仍在**：`cmx-api/src/handlers/` 下 5 个文件（`portal/model_center.rs`、`dct.rs`、`doc.rs`、`auth/api_key_handler.rs`、`oauth2_client_handler.rs`）直接调 `execute_sql`，违反第 8 章"cmx-api 纯 HTTP 适配层"。AGENTS.md 自称"列入 backlog"，但 `model_center.rs` 同时还是 unwrap 热点（14 处），说明违规代码仍在生长。
3. **🟡 unwrap/panic 在新代码中回潮**：全工程 `.unwrap()` 1388 处（v2 称生产路径已收敛到 84）。新模块是重灾区——`cmx-flow-bpmn/src/lib.rs`（unwrap 17 + panic! 10）、`cmx-biz/src/doc/{saver,meta}.rs`（各 21）、`cmx-core/src/model/cell.rs` **panic! 20 处且在生产模型代码**。近 3 周 +62% 的代码未经 v2 那轮"lint 零告警"式收尾。
4. **🟡 v2 三项待办，两项原样、一项倒退**：插件签名 `validator.rs` 中 `verify_signature: false` 仍在（且与 `settings.rs` 默认 `true` 两处不一致）；行级数据权限 `get_data_scope` 仍恒返回 `DataScope::All`；限流中间件 `mw_rate_limit.rs` **整体被注释掉**（v2 时至少是"缺失"，现在是代码留着但禁用）。

## 四、其他风险

- **依赖膨胀**：`Cargo.lock` 850 个包，**76 个包存在重复版本**（hashbrown 5 版、windows-sys 4 版、rand/toml/wasmparser 各 3 版）。且同时引入 wasmtime 39 + extism 两个 WASM 运行时、volo 全家桶、nacos-sdk、image、zip 等重依赖，编译成本与供应链面都偏大。
- **无 CI 门禁**：无 `.github/workflows` / `.gitlab-ci.yml`，无 `rust-toolchain.toml` 锁定工具链。914 次提交全靠人工纪律保证"clippy 零告警"这类成果不回退——从第 3 点看，**已经在回退**。
- **`.gitignore` 残留矛盾**：`/Cargo.lock` 在 ignore 列表中但文件实际被追踪（二进制项目追踪 lock 是对的，ignore 条目是脏残留）。
- **README 信息滞后**：README 写"20+ crate"、version 0.1.9，实际 44 crate、0.1.12；目录结构里没有 `cmx-flow/cmx-rpt/cmx-ai/cmx-portal` 等新模块。
- **工作区卫生**：大量未提交修改（13 个文件 M）、多分支并行（cmx-rpt-flow/dev/dev-local/master…），根目录散落 `评估报告v2.html`、`.deploy-server.pid` 等杂物。

## 五、评分

| 维度 | 得分 | 说明 |
|---|---|---|
| 架构设计 | 88 | 分层/trait 解耦/集群设计扎实，新模块沿用了正确范式 |
| 代码质量 | 78 | 存量优秀，增量回潮（unwrap/panic 在新模块泛滥） |
| 测试与验证 | 82 | 测试数量与 e2e 体系好，但无 CI 门禁兜底 |
| 安全 | 70 | .env 入库是实锤事故；签名验证/数据权限/限流三处短板 |
| 工程化 | 76 | 规范文档一流，执行靠自觉，无自动化保障 |
| **综合** | **80 / 100** | 从 v2 的 91 回落 —— 不是代码变差了，是**3 周 62% 的野蛮生长稀释了上一轮加固成果** |

## 六、建议（按优先级）

1. **本周必做**：`.env` 出库（`git rm --cached` + 恢复 gitignore 注释 + 历史清理或凭据轮换）。
2. **设 CI 门禁**：至少 `cargo check --workspace` + `clippy --workspace --tests -D warnings` + `cargo test`，否则每次冲刺后都要重做一轮 v2 式收尾。
3. **对新模块做一轮 v2 式质量收尾**：重点是 `cmx-flow-bpmn`、`cmx-biz/doc`、`cmx-core/model/cell.rs` 的 panic!/unwrap，以及 `model_center.rs` 的手写 SQL 下沉。
4. **收尾三待办**：签名验证默认值统一并开启、数据权限要么实现要么明确标注"未生效"的 API 语义、限流在网关层的落地结论写进文档。
5. **依赖治理**：`cargo-deny` 查重 + 评估 wasmtime/extism 双运行时是否都有必要。
6. **刷新 README** 与 crate 数量、版本、模块清单同步。

总体判断：这是一个**规范意识罕见地强、架构功底扎实的工程**——AGENTS.md 18 章、技能体系、SQL/配置双台账都是 mature 做法。当前最大的敌人不是技术债，而是**高速增长下规范执行力的稀释**，以及一个已经泄露进 git 历史的 `.env`。
