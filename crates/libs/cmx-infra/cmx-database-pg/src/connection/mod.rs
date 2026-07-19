//! 连接层（tokio-postgres + deadpool 版）
//!
//! `PgDbPool` 封装 deadpool-postgres 的 `Pool`（PG-only，单一变体），替代 sqlx 版的
//! `DbPool` 三库枚举。6 个执行方法签名与 sqlx 版对齐，唯一差异：`*_with_sqlxvalues`
//! 改为 `*_with_seavalues`（参数 `sea_query::Values`）。
//!
//! `DbRegistry` / `DatabasePoolImpl` / `DatabasePool` / 全局注册表逻辑与 sqlx 版一致
//! （独立静态量，与旧 crate 隔离）。

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::RwLock;

use deadpool_postgres::{Hook, HookError, Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use tracing::info;

use crate::config::{DbConfig, DbType};
use crate::executor::{PgResultConverter, as_param_refs, bind_data_values_pg, sea_values_to_tosql};
use crate::transaction::Dbx;
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;

/// 数据库连接池（tokio-postgres，PG-only）。
///
/// `Clone` 廉价（deadpool `Pool` 内部 Arc）。
#[derive(Clone)]
pub struct DbPool {
    pool: Pool,
}

impl std::fmt::Debug for DbPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbPool").finish_non_exhaustive()
    }
}

impl DbPool {
    /// 在连接池上执行无参数 SQL，返回受影响行数。
    pub async fn execute(&self, sql: &str) -> crate::Result<u64> {
        let client = self.pool.get().await?;
        let n = client.execute(sql, &[]).await?;
        Ok(n)
    }

    /// 在连接池上执行无参数查询，返回 DataSet。
    pub async fn query(&self, sql: &str, dataset_id: &str) -> crate::Result<DataSet> {
        let client = self.pool.get().await?;
        let rows = client.query(sql, &[]).await?;
        Ok(PgResultConverter::convert_rows(rows, dataset_id))
    }

    /// 带 `DataValue` 参数执行（`$1,$2` 占位）。
    pub async fn execute_with_datavalues(
        &self,
        sql: &str,
        params: &[DataValue],
    ) -> crate::Result<u64> {
        let client = self.pool.get().await?;
        let boxed = bind_data_values_pg(params);
        let refs = as_param_refs(&boxed);
        let n = client.execute(sql, &refs).await?;
        Ok(n)
    }

    /// 带 `DataValue` 参数查询，返回 DataSet。
    pub async fn query_with_datavalues(
        &self,
        sql: &str,
        params: &[DataValue],
        dataset_id: &str,
    ) -> crate::Result<DataSet> {
        let client = self.pool.get().await?;
        let boxed = bind_data_values_pg(params);
        let refs = as_param_refs(&boxed);
        let rows = client.query(sql, &refs).await?;
        Ok(PgResultConverter::convert_rows(rows, dataset_id))
    }

    /// 执行 sea-query 生成的语句（参数为 `sea_query::Values`）。
    pub async fn execute_with_seavalues(
        &self,
        sql: &str,
        params: sea_query::Values,
    ) -> crate::Result<u64> {
        let client = self.pool.get().await?;
        let boxed = sea_values_to_tosql(params)?;
        let refs = as_param_refs(&boxed);
        let n = client.execute(sql, &refs).await?;
        Ok(n)
    }

    /// 查询 sea-query 生成的语句，返回 DataSet。
    pub async fn query_with_seavalues(
        &self,
        sql: &str,
        params: sea_query::Values,
        dataset_id: &str,
    ) -> crate::Result<DataSet> {
        let client = self.pool.get().await?;
        let boxed = sea_values_to_tosql(params)?;
        let refs = as_param_refs(&boxed);
        let rows = client.query(sql, &refs).await?;
        Ok(PgResultConverter::convert_rows(rows, dataset_id))
    }

    /// 从池中取出一条独占连接（供事务层持有，跨 await 手动驱动 BEGIN/COMMIT）。
    pub async fn get_conn(&self) -> crate::Result<deadpool_postgres::Object> {
        Ok(self.pool.get().await?)
    }

    /// 无参查询，返回零拷贝 [`ZmcDataSet`]（持有原始 Row，惰性二进制编码）。
    ///
    /// `client.query()` 已返回 `Vec<Row>`（每行内含引用计数 Bytes），直接包成 ZmcDataSet，
    /// 不产 DataValue 枚举、不产 DataSet。
    pub async fn query_zmc(
        &self,
        sql: &str,
        dataset_id: &str,
    ) -> crate::Result<crate::zmcdataset::ZmcDataSet> {
        let client = self.pool.get().await?;
        let rows: Vec<crate::zmcdataset::TokioPgRowSource> = client
            .query(sql, &[])
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(crate::zmcdataset::ZmcDataSet::new(
            dataset_id.to_string(),
            rows,
        ))
    }

    /// 带 `DataValue` 参数查询，返回零拷贝 [`ZmcDataSet`]。
    pub async fn query_zmc_with_datavalues(
        &self,
        sql: &str,
        params: &[DataValue],
        dataset_id: &str,
    ) -> crate::Result<crate::zmcdataset::ZmcDataSet> {
        let client = self.pool.get().await?;
        let boxed = bind_data_values_pg(params);
        let refs = as_param_refs(&boxed);
        let rows: Vec<crate::zmcdataset::TokioPgRowSource> = client
            .query(sql, &refs)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(crate::zmcdataset::ZmcDataSet::new(
            dataset_id.to_string(),
            rows,
        ))
    }

    /// 无参查询，**流式**编码成列式二进制包写入 `out`，返回行数。
    ///
    /// 峰值内存 O(单行 + 输出)，不囤全部 Row —— 大结果集的零内存路径。
    /// 逐行调 `cmx_rowsource::encode_row_into`，每行编完即 drop。
    pub async fn query_zmc_streaming(
        &self,
        sql: &str,
        dataset_id: &str,
        out: &mut Vec<u8>,
    ) -> crate::Result<u64> {
        use crate::zmcdataset::TokioPgRowSource;
        use cmx_rowsource::{ZmcSchema, encode_row_into, encode_stream_close, encode_stream_open};
        use futures::TryStreamExt;

        let client = self.pool.get().await?;
        let params: Vec<i32> = Vec::new();
        let stream = client.query_raw(sql, params).await?;
        futures::pin_mut!(stream);

        // 首行确定 schema(包成 newtype)
        let first = stream.try_next().await?.map(TokioPgRowSource::from);
        let schema = match &first {
            Some(r) => ZmcSchema::from_row(r),
            None => ZmcSchema::from_parts(vec![], vec![]),
        };

        // 单缓冲:先写头 + 预留 rows 数组长度,再把各行**直接编进 out**(免 rows_body 双缓冲)
        let marker = encode_stream_open(out, dataset_id, &schema);
        let mut count: u64 = 0;
        if let Some(r) = &first {
            encode_row_into(out, r, &schema);
            count += 1;
        }
        drop(first);
        while let Some(r) = stream.try_next().await? {
            let r = TokioPgRowSource::from(r);
            encode_row_into(out, &r, &schema);
            count += 1;
        }

        // 回填 rows 数组长度
        encode_stream_close(out, marker, count as u32);
        Ok(count)
    }

    /// **真·分帧流式**：带 `DataValue` 参数查询，逐行编成长度分帧发到 `chunk_tx`，峰值内存 O(单行)。
    ///
    /// 协议见 `cmx_rowsource::encode_frame_*`：header 帧 + N 个 row 帧 + end 帧。
    /// 每行编完即发、随即 drop，不囤全部 Row/rows_body —— 超大扁平结果的零内存网络流式路径。
    /// 每积攒到 ~16KB 或每行即刷一次到 channel（背压由 channel 容量控制）。返回行数。
    ///
    /// `col_names`：来自定义 schema 的列名（header 帧只带列名，不带类型），**先于访问结果流**发出，
    /// 保证即使空结果（tokio-postgres `query_raw` 对空结果偶发 "connection closed"）也能收尾。
    pub async fn query_zmc_stream_chunks(
        &self,
        sql: &str,
        params: &[DataValue],
        dataset_id: &str,
        col_names: &[String],
        chunk_tx: tokio::sync::mpsc::Sender<bytes::Bytes>,
    ) -> crate::Result<u64> {
        use crate::zmcdataset::TokioPgRowSource;
        use crate::zmcdataset::{encode_frame_end, encode_frame_header, encode_frame_row};
        use cmx_rowsource::{ZmcColType, ZmcSchema};
        use futures::TryStreamExt;

        const FLUSH_AT: usize = 16 * 1024; // 攒够 16KB 刷一次，减少 channel 往返

        // header 帧：只带列名，来自定义 schema，**不依赖结果流首行** → 空结果也能正常收尾。
        // 类型用占位（Unknown）——header 不含类型；真正逐行编码时用首行的真实类型 schema。
        let header_schema = ZmcSchema::from_parts(
            col_names.to_vec(),
            vec![ZmcColType::Unknown; col_names.len()],
        );
        let mut buf: Vec<u8> = Vec::with_capacity(FLUSH_AT + 1024);
        encode_frame_header(&mut buf, dataset_id, &header_schema);
        // 立即把 header 发出（哪怕后续查询失败，客户端也拿到合法头 + 后面的 end）。
        if chunk_tx
            .send(bytes::Bytes::copy_from_slice(&buf))
            .await
            .is_err()
        {
            return Ok(0);
        }
        buf.clear();

        let client = self.pool.get().await?;
        let boxed = bind_data_values_pg(params);
        let refs = as_param_refs(&boxed);
        let stream = client.query_raw(sql, refs).await?;
        futures::pin_mut!(stream);

        let mut count: u64 = 0;
        let mut row_schema: Option<ZmcSchema> = None;
        loop {
            // 空结果时 query_raw 偶发以 Err("connection closed") 结束首次拉取；容错当作流结束。
            let next = match stream.try_next().await {
                Ok(v) => v,
                Err(e) => {
                    if count == 0 {
                        tracing::debug!("流式空结果/首拉终止({dataset_id}): {e}");
                    } else {
                        tracing::warn!("流式中断({dataset_id}, 已发 {count} 行): {e}");
                    }
                    break;
                }
            };
            let Some(row) = next else { break };
            let r = TokioPgRowSource::from(row);
            let schema = row_schema.get_or_insert_with(|| ZmcSchema::from_row(&r));
            encode_frame_row(&mut buf, &r, schema);
            count += 1;
            if buf.len() >= FLUSH_AT {
                let chunk = bytes::Bytes::copy_from_slice(&buf);
                buf.clear();
                if chunk_tx.send(chunk).await.is_err() {
                    return Ok(count); // 客户端断开
                }
            }
        }

        encode_frame_end(&mut buf);
        let _ = chunk_tx.send(bytes::Bytes::copy_from_slice(&buf)).await;
        Ok(count)
    }
}

/// 数据库连接池 trait
pub trait DatabasePool: Send + Sync {
    /// 获取数据库访问对象
    fn get_dbx(&self) -> Dbx;
    /// 获取数据库配置
    fn get_config(&self) -> DbConfig;
}

/// 数据库连接池实现（活跃计数 / 优雅关闭，与 sqlx 版一致）
#[derive(Clone)]
pub struct DatabasePoolImpl {
    dbx: Dbx,
    config: DbConfig,
    active_connections: Arc<std::sync::atomic::AtomicUsize>,
    is_closing: Arc<std::sync::atomic::AtomicBool>,
}

impl DatabasePoolImpl {
    /// 按配置创建连接池实现（建立底层 tokio-postgres 连接）。
    pub async fn new(config: DbConfig) -> crate::Result<Self> {
        let dbx = create_dbx(&config).await?;
        Ok(Self {
            dbx,
            config,
            active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            is_closing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// 获取一个连接（克隆 Dbx）并递增活跃计数。
    pub fn acquire(&self) -> Dbx {
        self.active_connections
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.dbx.clone()
    }

    /// 归还连接，递减活跃计数。
    pub fn release(&self) {
        self.active_connections
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// 是否已标记为关闭中。
    pub fn is_closing(&self) -> bool {
        self.is_closing.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 标记为关闭中。
    pub fn mark_closing(&self) {
        self.is_closing
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 当前活跃连接数。
    pub fn active_count(&self) -> usize {
        self.active_connections
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 等待所有活跃连接归还（超时返回 false）。
    pub async fn wait_for_idle(&self, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while self.active_count() > 0 {
            if start.elapsed() > timeout {
                return false;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        true
    }
}

impl DatabasePool for DatabasePoolImpl {
    fn get_dbx(&self) -> Dbx {
        self.dbx.clone()
    }

    fn get_config(&self) -> DbConfig {
        self.config.clone()
    }
}

/// 全局注册器
pub(crate) struct DbRegistry {
    pools: RwLock<HashMap<String, DatabasePoolImpl>>,
}

impl DbRegistry {
    /// 创建空注册表。
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }

    /// 注册新数据源（按 db_id 建池并存入注册表）。
    pub async fn register(&self, config: DbConfig) -> crate::Result<()> {
        let db_key = config.db_id.clone();
        let pool = DatabasePoolImpl::new(config).await?;
        let mut pools = self.pools.write().await;
        pools.insert(db_key, pool);
        Ok(())
    }

    /// 热更新数据源：标记旧池关闭、等待空闲后用新配置重建。
    #[allow(dead_code)]
    pub async fn update(&self, config: DbConfig) -> crate::Result<()> {
        let key = config.db_id.clone();
        // 先标记旧池关闭中
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(&key) {
                pool.mark_closing();
            }
        }
        // 等待旧池活跃连接归还
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(&key) {
                let timeout = std::time::Duration::from_secs(30);
                if !pool.wait_for_idle(timeout).await {
                    tracing::warn!(
                        "等待旧连接池关闭超时，仍有 {} 个活跃连接",
                        pool.active_count()
                    );
                }
            }
        }
        // 用新配置重建
        let pool = DatabasePoolImpl::new(config).await?;
        let mut pools = self.pools.write().await;
        pools.insert(key, pool);
        Ok(())
    }

    /// 注销数据源（标记关闭并移出注册表）。
    pub async fn unregister(&self, key: &str) -> Option<DatabasePoolImpl> {
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(key) {
                pool.mark_closing();
            }
        }
        let mut pools = self.pools.write().await;
        pools.remove(key)
    }

    /// 列出所有已注册数据源的 db_id。
    pub async fn list(&self) -> Vec<String> {
        let pools = self.pools.read().await;
        pools.keys().cloned().collect()
    }

    /// 按 db_id 获取访问对象 + 配置。
    pub async fn get(&self, key: &str) -> Option<(Dbx, DbConfig)> {
        let pools = self.pools.read().await;
        pools
            .get(key)
            .map(|pool| (pool.get_dbx(), pool.get_config()))
    }

    /// 按 db_id 仅获取访问对象。
    pub async fn get_db_access(&self, key: &str) -> Option<Dbx> {
        self.get(key).await.map(|(dbx, _)| dbx)
    }

    /// 按 db_id 仅获取配置。
    pub async fn get_db_config(&self, key: &str) -> Option<DbConfig> {
        self.get(key).await.map(|(_, config)| config)
    }

    /// 列出所有已注册数据源的配置。
    pub async fn list_configs(&self) -> Vec<DbConfig> {
        let pools = self.pools.read().await;
        pools.values().map(|p| p.get_config()).collect()
    }
}

static GLOBAL_REGISTRY: OnceLock<Arc<DbRegistry>> = OnceLock::new();

/// 获取全局数据源注册表单例（惰性初始化）。
pub(crate) fn get_global_registry() -> &'static Arc<DbRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| Arc::new(DbRegistry::new()))
}

/// 创建数据库访问对象
async fn create_dbx(config: &DbConfig) -> crate::Result<Dbx> {
    let db_pool = new_db_pool(config).await?;
    let dbx = Dbx::new(db_pool, false)?;
    Ok(dbx)
}

/// 创建新的 PostgreSQL 连接池（deadpool）。
///
/// - MySQL/SQLite 一律拒绝（本 crate PG-only）。
/// - search_path 注入：deadpool `post_create` hook，每条新物理连接执行
///   `SET search_path TO <schema>, public`（`RecyclingMethod::Fast` 下会跨复用保留）。
async fn new_db_pool(config: &DbConfig) -> crate::Result<DbPool> {
    if config.db_type != DbType::Postgres {
        return Err(crate::Error::UnsupportedDbType);
    }

    let pool_config = &config.pool_config;
    info!(
        "创建 PostgreSQL 连接池(tokio-postgres)，连接池配置：{:?}",
        pool_config
    );

    // 解析连接串为 tokio_postgres::Config
    let pg_config = tokio_postgres::Config::from_str(&config.db_url).map_err(crate::Error::from)?;

    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr = Manager::from_config(pg_config, NoTls, mgr_config);

    let schema = config
        .db_schema
        .clone()
        .unwrap_or_else(|| "public".to_string());
    let set_search_path = format!("SET search_path TO {}, public", schema);

    let pool = Pool::builder(mgr)
        .max_size(pool_config.max_connections)
        // 配置了超时，deadpool 要求显式指定 Runtime（否则 build() 报
        // "Timeouts require a runtime"）。
        .runtime(Runtime::Tokio1)
        .create_timeout(Some(std::time::Duration::from_secs(
            pool_config.acquire_timeout,
        )))
        .wait_timeout(Some(std::time::Duration::from_secs(
            pool_config.acquire_timeout,
        )))
        .recycle_timeout(Some(std::time::Duration::from_secs(
            pool_config.idle_timeout,
        )))
        .post_create(Hook::async_fn(move |client, _metrics| {
            let sql = set_search_path.clone();
            Box::pin(async move {
                client
                    .simple_query(&sql)
                    .await
                    .map_err(HookError::Backend)?;
                Ok(())
            })
        }))
        .build()
        .map_err(|e| crate::Error::CantCreateModelManagerProvider(e.to_string()))?;

    Ok(DbPool { pool })
}
