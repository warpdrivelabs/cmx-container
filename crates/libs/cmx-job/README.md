# cmx-job/

> 异步任务中心域分组：api（HTTP 皮肤 + SSE）+ core（内核：领域模型 / 状态机 / 调度）+ store-pg（PG 持久化）三件套。

## 分组定位

本分组组织**异步任务中心**域，采用平台标准三层拆分，但第二层名为 `core`
而非 `model`：`cmx-job-core` 不只是数据模型，还内含状态机、`JobManager`
生命周期调度与 `JobEventHub` SSE 扇出等运行时内核。`cmx-job-api` 是 HTTP
协议皮肤（含 SSE 实时进度端点），`cmx-job-store-pg` 实现 core 层的
`JobStore` trait 完成持久化。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-job-core` | 异步任务中心内核：作业领域模型 + 状态机 + `JobHandler` / `JobContext` 业务接触面 + `JobManager` 生命周期调度 + `JobEventHub` SSE 扇出——语义中立，零业务、零 DB 依赖 | [README](./cmx-job-core/README.md) |
| `cmx-job-api` | 异步任务中心的 HTTP 协议皮肤：提交 / 列表 / 详情 / 控制的薄 axum handler + SSE 实时进度端点（单作业流 + 全库汇总流）+ `JobModule` 路由聚合 | [README](./cmx-job-api/README.md) |
| `cmx-job-store-pg` | 异步任务中心的 PostgreSQL 持久化层：实现 `cmx-job-core::JobStore` trait——作业主表 / 日志 / 断点 / 历史表的自 DDL、写穿与查询，含 `FOR UPDATE SKIP LOCKED` 原子抢占与 RU / HI 归档事务 | [README](./cmx-job-store-pg/README.md) |

## 组织规则

- 三层单向依赖：`cmx-job-api` → `cmx-job-store-pg` → `cmx-job-core`。
- core 层零 DB、零业务依赖，通过 `JobStore` trait 与 `JobHandler` 接口
  同两侧解耦，可独立单测。
- 抢占语义：多 worker 并发抢作业由 store-pg 的
  `FOR UPDATE SKIP LOCKED` 保证原子性。

## 相关背景

- 同款三件套分组：`../cmx-dct/`、`../cmx-doc/`、`../cmx-mdm/`。
- 长耗时作业的业务方（如 MDM 扫描、模型部署）经任务中心挂后台执行。
