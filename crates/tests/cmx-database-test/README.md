# cmx-database-test

sqlx vs tokio-postgres 的 PostgreSQL 性能对比基准工程。

一个独立可执行程序（非 criterion 微基准）：跑完一次打印对比表格并写入 `RESULTS.md`。

## 测什么

对**完全相同的表结构、相同的数据、相同的连接**，只换驱动，对比：

1. **插入 10 万行 × 50 列**（数据来自模板文件 `bench_row.json`，同一行重复插入，仅主键递增）——三种写入策略：
   - 逐行 INSERT（事务内）
   - 批量多值 INSERT（`INSERT ... VALUES (..),(..)...`）
   - COPY（PG 最快批量导入路径，文本格式）
2. **查询 10万 / 50万 / 100万行**——两种读取方式：
   - `fetch_all`（一次性全量物化）
   - 流式逐行（`fetch` / `query_raw`，峰值内存 O(单行)）
3. **其他性能维度**：
   - 点查延迟分布（P50/P95/P99，按主键单行查询）
   - **Pipelining**（tokio-postgres 独有，串行 vs 管道化并发独立查询的加速比）

## 表结构（50 列宽表）

1 BIGINT 主键 + 15 整数 + 15 文本 + 8 NUMERIC + 5 TIMESTAMPTZ + 3 布尔 + 2 UUID + 1 JSONB。

DDL 与列顺序由 `src/schema.rs` 生成，两条驱动路径共用，确保公平。

## 运行

```bash
DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/cmx" \
  cargo run -p cmx-database-test --release
```

可选环境变量：

| 变量 | 默认 | 说明 |
|------|------|------|
| `INSERT_ROWS` | 100000 | 插入行数 |
| `QUERY_SIZES` | `100000,500000,1000000` | 查询规模（逗号分隔） |
| `BATCH` | 500 | 批量插入每批行数 |
| `LAT_SAMPLES` | 2000 | 点查延迟每轮采样数 |
| `PIPE_QUERIES` | 1000 | 管道化对比的查询数 |
| `ROUNDS_INSERT` | 3 | 插入场景迭代轮数（重且慢，默认少轮） |
| `ROUNDS_QUERY` | 5 | 查询场景迭代轮数 |
| `ROUNDS_LAT` | 3 | 点查延迟轮数（各轮样本合并算分位数） |
| `ROUNDS_PIPE` | 5 | 管道化对比轮数（取中位加速比） |

**多轮取中位数**：每个吞吐场景跑 N 轮，报告**中位吞吐 + 最快轮 + 变异系数 CV%**。CV 越小越稳定（个位数% 说明数字可信）；中位数抗离群点（冷缓存、GC、后台 IO 抖动）。点查延迟把多轮原始样本合并后统一算 P50/P95/P99。

小规模冒烟：

```bash
DATABASE_URL=... INSERT_ROWS=5000 QUERY_SIZES="5000,10000" \
  cargo run -p cmx-database-test
```

结果写入 `RESULTS.md`（含结论解读）。

## 模块

- `schema.rs` — 50 列表结构、DDL、列名、占位符
- `data.rs` — 模板行 JSON 文件生成/加载、COPY 文本格式化
- `report.rs` — 计时、延迟分位数、对比表格
- `bench_sqlx.rs` — sqlx 各场景实现
- `bench_tokio_pg.rs` — tokio-postgres 各场景 + pipelining
- `main.rs` — 编排、跑全部场景、出报告

## 说明

- 每个场景各跑一次（宏基准，单次即秒级，波动由数据规模摊薄）。绝对值随机器/PG 配置波动，重点看**同场景两驱动的相对比值**。
- 每种插入策略用独立表（`bench_wide_{sqlx,pg}_{row,batch,copy}`），避免残留干扰。
- 查询/点查用 COPY 装载的表；若 `QUERY_SIZES` 最大值 > `INSERT_ROWS`，先 COPY 扩表。
- 注意：本 crate 位于 `crates/tests/`，被仓库 `.gitignore`（第 18 行 `/crates/tests/`）忽略，默认不纳入 git。如需跟踪：`git add -f crates/tests/cmx-database-test`。
