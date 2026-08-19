# cmx-code/

> 通用业务编码引擎分组：`cmx-code-model`（纯逻辑）+ `cmx-code-api`（HTTP / DB / 引擎装配）两件套，无独立 store-pg。

## 分组定位

本分组承载平台**通用业务编码引擎**——为 DCT / DOC / MDM 等域提供集中式
"铸号"能力：编码规则定义、预览、生成、校验、断号登记与反解析。

组织上分两层：`cmx-code-model` 是无 DB、无 HTTP 依赖的纯逻辑层（规则类型
与七种段类型求值算法）；`cmx-code-api` 合并了 HTTP 层、DB 访问层与引擎装配，
并实现 `cmx-traits` 的 `CodeMinter` trait 供各域写入钩子全局铸号。
持久化未拆独立 store-pg，是本分组与 dct / doc / mdm 三件套分组的主要差异。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-code-model` | 纯逻辑层：编码规则类型定义（`RuleSpec` / `CodeRule`）、七种段类型求值、补位 / 截断 / 校验位与断号反解析等纯算法——无 DB、无 HTTP 依赖 | [README](./cmx-code-model/README.md) |
| `cmx-code-api` | HTTP 层 + DB 访问层 + 引擎装配：`CodeModule` 聚合规则库 / 预览 / 生成 / 校验 / 断号端点（`/api/code/*`），`CodeEngine` 实现 `cmx-traits` 的 `CodeMinter` trait 供 DCT / DOC / MDM 钩子全局铸号 | [README](./cmx-code-api/README.md) |

## 组织规则

- 依赖方向：`cmx-code-api` → `cmx-code-model`；model 层零基础设施依赖，可独立单测。
- DB 抽象：model 层把"取下一序列 / 断号登记"抽象为 `Advance` trait，
  由 api 层注入 PostgreSQL 实现，保持算法与存储解耦。
- 端点前缀：`/api/code/*`。

## 相关背景

- 铸号消费方：`../cmx-dct/cmx-dct-store-pg/`、`../cmx-doc/cmx-doc-store-pg/`、
  `../cmx-mdm/cmx-mdm-store-pg/` 在写入时经 `CodeMinter` 完成主键铸号。
- HTTP 皮肤共享骨架见 `../cmx-apis/cmx-api-core/`；本分组 crate 命名亦遵循
  `cmx-{domain}-api` / `cmx-{domain}-model` 约定。
