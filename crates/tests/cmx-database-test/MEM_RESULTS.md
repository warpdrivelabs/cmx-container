# ZmcDataSet(零拷贝) vs DataSet(传统) 五方内存对标

> 表:50 列宽表 · 行数:100000 · 真实 PG · 计数分配器测活跃堆内存(alloc−dealloc)
> 五路径:sqlx/DataSet(老) · sqlx/Zmc全量 · sqlx/Zmc流式 · tokio/Zmc全量 · tokio/Zmc流式

## 各阶段活跃内存(MB,相对取数前基线)

| 阶段 | sqlx/DataSet | sqlx/Zmc全量 | sqlx/Zmc流式 | tokio/Zmc全量 | tokio/Zmc流式 |
|------|--------------|---------------|---------------|----------------|----------------|
| A 取数(原始行集) | 221.3 MB | 220.8 MB | — | 254.3 MB | — |
| B 结构就绪 | 229.7 MB | 220.8 MB | — | 254.3 MB | — |
| C 结构+输出同时活跃 | 485.7 MB | 348.8 MB | 243.0 MB | 382.3 MB | 243.0 MB |
| 峰值水位 | 613.7 MB | 412.8 MB | 243.0 MB | 446.3 MB | 243.0 MB |

> 峰值(相对 sqlx/DataSet 614 MB):sqlx/Zmc全量 **413 MB**(省 33%) · sqlx/Zmc流式 **243 MB**(省 60%) · tokio/Zmc全量 **446 MB**(省 27%) · tokio/Zmc流式 **243 MB**(省 60%)

## 输出体积(序列化结果)

| | sqlx/DataSet(JSON) | ZmcDataSet(msgpack,两驱动同) | 比值 |
|---|---|---|---|
| 输出字节 | 152.77 MB | 104.97 MB | JSON 是 msgpack 的 1.46x |

## 解读(直面「sqlx 能不能吃到 ZmcDataSet 红利」)

- **sqlx + ZmcDataSet 完全成立**:同一套驱动无关编码器(cmx-rowsource),sqlx 的 PgRow 与 tokio 的 Row 底层同为引用计数 Bytes,零拷贝能力等同。全量峰值 sqlx/Zmc **413 MB** vs tokio/Zmc **446 MB**;流式 sqlx **243 MB** vs tokio **243 MB** —— **驱动差异很小,收益来自 ZmcDataSet 的设计(不产 DataValue 副本 + msgpack + 流式),不来自换驱动**。
- **「持有原始行」的代价随驱动而异(意外发现)**:Zmc 全量攥着 10 万行原始 Row,sqlx 版占 221 MB、tokio 版占 254 MB,而 DataSet 的 DataValue 副本占 230 MB。**tokio 的 Row 每行结构更重**(列偏移用 usize、元数据引用等),持有全量时反超 DataValue 副本;**sqlx 的 PgRow 更紧凑**(列偏移 u32),持有全量甚至略省于 DataValue 副本。Bytes retention 的代价真实存在,但大小取决于驱动的行结构开销。
- **流式在两个驱动上同样是杀手锏**:峰值 sqlx流式 243 MB / tokio流式 243 MB,vs 老链路 614 MB —— 不囤行、边编边弃,这是老 DataSet 结构上做不到的。

### 一句话结论

**老代码不必为内存红利换驱动**:留在 sqlx,把出口从「DataSet+JSON」换成「ZmcDataSet+msgpack(大结果集用流式)」,即可拿到与 tokio-postgres 几乎相同的内存收益。tokio-postgres 的价值在别处(pipelining、低点查延迟),与本报告的内存维度无关。

---
注:计数分配器统计 Rust 堆分配(含网络缓冲/驱动缓存);单次测量,数值随数据/机器波动,重点看相对差。
