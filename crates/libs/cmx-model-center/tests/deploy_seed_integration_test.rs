//! SEED 部署端到端集成测试骨架。
//!
//! 本测试需要真实的 Postgres 实例（建台账表、源表、历史表，并执行
//! `PgSeedDataExecutor` 写入种子数据）。`cmx-container` 当前未引入
//! `testcontainers`，故本测试以 `#[ignore]` 形式落地为骨架，待以下任一条件满足后补全：
//!   1. 引入 testcontainers（`Cargo.toml` 加 dev-dependency），测试内自启 pg 容器；
//!   2. 或在 CI / 本地通过 docker-compose 预置测试库，由环境变量注入连接串。
//!
//! 运行方式（实施补全后）：
//! ```bash
//! cargo test -p cmx-model-center --test deploy_seed_integration_test -- --ignored
//! ```
//!
//! 期望覆盖的端到端校验（实施清单）：
//!   - 启动 Postgres 容器
//!   - `register_datasource` 注册业务库到 `cmx-database`
//!   - 通过 `cmx-model-center` init 建台账 5 张系统表
//!   - 部署一个测试 DCT（如 `base_dct` + 含 `cf_test` 表的 DCT）
//!   - 把测试种子 JSON 放到 `data/meta/definitions/test/test/test/seed/cf_test.json`
//!   - 调用 [`cmx_model_center::deploy_seed_menu::deploy_seed_with_events`]
//!   - 校验 `cf_test` 表行数 + `cmx_model_module_kind.def_checksum` + 历史锚点状态
//!
//! 主流程被测入口（仅供实施时参考）：
//!   - [`cmx_model_center::deploy_seed_menu::compile_all_definitions_for_module`]
//!   - [`cmx_model_center::seed_scanner::scan_seed_files`]
//!   - [`cmx_model_center::deploy_seed_menu::deploy_seed_with_events`]

#![cfg(test)]

/// E2E：实际启动 Postgres 容器，建 DCT 表，跑 SEED，校验行数 + checksum + 状态。
///
/// TODO 实施时按项目既有 testcontainer / docker-compose 模式补全以下步骤：
/// 1. 启动 Postgres 容器（testcontainers 或外部连接串）
/// 2. `register_datasource` 到 `cmx-database`
/// 3. `model_center` init 建台账系统表
/// 4. 部署测试 DCT（`base_dct` + 含 `cf_test` 的 DCT）
/// 5. 写测试种子 JSON 到 `data/meta/definitions/test/test/test/seed/cf_test.json`
/// 6. 调用 `deploy_seed_with_events(db_id, "test", "test", "test", operator_id, operator_name, None)`
/// 7. 断言：`cf_test` 表行数 > 0；`cmx_model_module_kind.def_checksum` 非空；历史锚点
///    `status='success'`、`action='seed'`、`def_ref='seed/'`
#[tokio::test]
#[ignore] // 集成测试需真实 Postgres，默认不跑（避免 CI 失败）
async fn test_seed_deploy_e2e() {
    // TODO 实施时按项目既有 testcontainer / docker-compose 模式补全。
}
