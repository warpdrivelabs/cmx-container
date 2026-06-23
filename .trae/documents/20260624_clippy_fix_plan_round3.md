# Clippy 警告修复计划 - 2026-06-24 (第三轮)

## 检查结果概览

- 检查时间：2026-06-24
- 检查命令：`cargo clippy --all-targets`
- 警告总数：10 条
- 待修复数：0 条（全部为排除的 too_many_arguments）
- 已排除数：10 条

## 待修复警告列表

### 警告类型统计

| 警告类型 | 数量 | 是否修复 |
|---------|------|---------|
| too_many_arguments (8/7) | 7 | **排除** |
| too_many_arguments (9/7) | 3 | **排除** |

### 详细警告列表（全部已排除）

#### too_many_arguments (8/7) - 7 处

- `crates/libs/cmx-infra/cmx-auth/src/jwt/encoder.rs:58`
- `crates/libs/cmx-infra/cmx-auth/src/oauth2/flows.rs:108`
- `crates/libs/cmx-infra/cmx-auth/src/policy/oauth2_policy.rs:79`
- `crates/libs/cmx-biz/src/function_invoker.rs:72`
- `crates/libs/cmx-plugin/src/service/record_builder.rs:128`
- `crates/libs/cmx-plugin/src/service/utils.rs:274`
- `crates/libs/cmx-iam/src/service_traits.rs:162`

#### too_many_arguments (9/7) - 3 处

- `crates/libs/cmx-debug/src/lib.rs:292`
- `crates/libs/cmx-debug/src/lib.rs:342`
- `crates/libs/cmx-service/src/repository.rs:380`

## 修复方案

第三轮无需修复，所有警告均为用户指定排除的 `too_many_arguments` 类型。

## 验证方案

1. 运行 `cargo build --all-targets` 验证编译通过
2. 确认无新增警告
