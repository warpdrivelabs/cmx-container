//! 事务核心模块（tokio-postgres 版）
//!
//! 与 cmx-database（sqlx 版）的 transaction/core.rs 对齐，但事务不再用 sqlx 的
//! `Transaction<'static>`（tokio-postgres 的 `Transaction<'a>` 借用 `&mut Client` 无法
//! `'static`）。改为：
//!
//! - `DbTransaction` 持有一条 deadpool `Object`（`'static` 独占物理连接，Drop 归还池），
//!   事务边界用**文本命令** `BEGIN` / `COMMIT` / `ROLLBACK` 手动驱动。同一 `Object` 上
//!   的所有 execute/query 天然属于同一事务。
//! - `TxnHolder` / 引用计数 / 两个全局注册表 / `Dbx` 的 begin/commit/rollback 逻辑
//!   与 sqlx 版逐行等价，仅把 `pool.begin()` 换成「get_conn + BEGIN」、`txn.commit()`
//!   换成 COMMIT 文本命令。
//!
//! 行为差异（相对 sqlx 版）：
//! 1. 事务期独占一条 deadpool 连接；`max_size` 须 ≥ 并发事务数，否则第 N+1 个 begin
//!    在 `get_conn()` 处等 `wait_timeout` 返回 `Error::Pool`（对应旧 `PoolExhausted` 语义）。
//! 2. 后端错误类型 `Error::Postgres` / `Error::Pool`（旧为 `Error::Sqlx`）。
//! 3. `NullTyped` 必须给对 `None::<T>`（tokio-postgres NULL 类型推断更严格）。
//! 4. 仅 `Propagation::Required` 真实现（与旧版一致）；SAVEPOINT 第一版不做，
//!    `savepoint_depth` 预留字段。

use crate::connection::DbPool;
use crate::error::{Error, Result};
use crate::executor::{PgResultConverter, as_param_refs, bind_data_values_pg, sea_values_to_tosql};
use crate::transaction::metadata::{TransactionStatus, register_txn};
use crate::transaction::registry::get_txn_holder_registry;
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid;

/// 数据库访问对象，支持事务管理（tokio-postgres 版，与 sqlx 版 API 一致）
#[derive(Clone)]
pub struct Dbx {
    /// 数据库连接池
    db_pool: DbPool,
    /// 事务持有器，跨 await 持有的当前事务槽位
    txn_holder: Arc<Mutex<Option<TxnHolder>>>,
    /// 是否启用事务
    with_txn: bool,
}

impl std::fmt::Debug for Dbx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dbx")
            .field("with_txn", &self.with_txn)
            .finish_non_exhaustive()
    }
}

impl Dbx {
    /// 创建新的数据库访问对象
    pub fn new(db_pool: DbPool, with_txn: bool) -> Result<Self> {
        Ok(Dbx {
            db_pool,
            txn_holder: Arc::default(),
            with_txn,
        })
    }

    /// 开始事务
    pub async fn begin_txn(&self, db_id: &str, propagation: Propagation) -> Result<String> {
        if !self.with_txn {
            return Err(Error::CannotBeginTxnWithTxnFalse);
        }
        match propagation {
            Propagation::Required => self.do_required(db_id).await,
            _ => Err(Error::TransactionNotAllowed),
        }
    }

    /// Required: 如果存在事务则加入，否则创建新事务
    async fn do_required(&self, db_id: &str) -> Result<String> {
        let mut txh_g = self.txn_holder.lock().await;
        if txh_g.is_some() {
            if let Some(txh) = txh_g.as_mut() {
                txh.inc();
                Ok(txh.txn_id().to_string())
            } else {
                Err(Error::NoTxn)
            }
        } else {
            drop(txh_g);
            self.create_new_txn(db_id).await
        }
    }

    /// 开始事务（默认传播行为：Required）
    pub async fn begin_txn_default(&self, db_id: &str) -> Result<String> {
        self.begin_txn(db_id, Propagation::Required).await
    }

    /// 创建新事务：从池取独占连接 + 发送 BEGIN，然后双注册表登记。
    async fn create_new_txn(&self, db_id: &str) -> Result<String> {
        // 取独占连接并开启事务
        let conn = self.db_pool.get_conn().await?;
        conn.batch_execute("BEGIN").await?;
        let txn = DbTransaction::new(conn);

        // 创建事务持有器
        let txh = TxnHolder::new(txn, db_id);
        let txn_id = txh.txn_id().to_string();

        // 放入自身事务槽
        let mut txh_g = self.txn_holder.lock().await;
        let _ = txh_g.insert(txh);

        // 注册元数据
        register_txn(txn_id.clone(), db_id.to_string()).await;

        // 注册 TxnHolder 到全局注册表（共享同一 Arc）
        get_txn_holder_registry()
            .write()
            .await
            .insert(txn_id.clone(), self.txn_holder.clone());

        Ok(txn_id)
    }

    /// 回滚事务
    pub async fn rollback_txn(&self) -> Result<()> {
        if !self.with_txn {
            return Err(Error::CannotBeginTxnWithTxnFalse);
        }

        let mut txn_id: Option<String> = None;
        let mut should_rollback = false;
        let mut txn_to_rollback = None;

        let result = {
            let mut txh_g = self.txn_holder.lock().await;
            if let Some(mut txn_holder) = txh_g.take() {
                txn_id = Some(txn_holder.txn_id().to_string());
                if txn_holder.counter > 1 {
                    txn_holder.counter -= 1;
                    let _ = txh_g.replace(txn_holder);
                } else {
                    txn_to_rollback = Some(txn_holder);
                    should_rollback = true;
                }
                Ok(())
            } else {
                Err(Error::NoTxn)
            }
        };
        result?;

        if should_rollback && let (Some(txn), Some(txn_id)) = (txn_to_rollback, txn_id) {
            txn.rollback().await?;
            crate::transaction::metadata::update_txn_status(&txn_id, TransactionStatus::RolledBack)
                .await;
            get_txn_holder_registry().write().await.remove(&txn_id);
        }

        Ok(())
    }

    /// 提交事务
    pub async fn commit_txn(&self) -> Result<()> {
        if !self.with_txn {
            return Err(Error::CannotCommitTxnWithTxnFalse);
        }

        let mut txn_id: Option<String> = None;
        let mut should_commit = false;
        let mut txn_to_commit = None;

        let result = {
            let mut txh_g = self.txn_holder.lock().await;
            if let Some(txh) = txh_g.as_mut() {
                txn_id = Some(txh.txn_id().to_string());
                let counter = txh.dec();
                if counter == 0 {
                    txn_to_commit = txh_g.take();
                    should_commit = true;
                }
                Ok(())
            } else {
                Err(Error::TxnCantCommitNoOpenTxn)
            }
        };
        result?;

        if should_commit && let (Some(txn), Some(txn_id)) = (txn_to_commit, txn_id) {
            txn.commit().await?;
            crate::transaction::metadata::update_txn_status(&txn_id, TransactionStatus::Committed)
                .await;
            get_txn_holder_registry().write().await.remove(&txn_id);
        }

        Ok(())
    }

    /// 获取数据库连接池
    pub fn db(&self) -> &DbPool {
        &self.db_pool
    }

    /// 获取当前事务ID
    pub async fn get_txn_id(&self) -> Option<String> {
        let txh_g = self.txn_holder.lock().await;
        txh_g.as_ref().map(|txh| txh.txn_id().to_string())
    }

    /// 检查事务是否超时
    pub async fn is_txn_timeout(&self, timeout: std::time::Duration) -> bool {
        let txh_g = self.txn_holder.lock().await;
        txh_g
            .as_ref()
            .map(|txh| txh.elapsed() > timeout)
            .unwrap_or(false)
    }

    /// 创建一个支持事务的 Dbx 实例（新空事务槽，不复用旧槽）
    pub fn with_transaction(&self) -> Result<Self> {
        Ok(Dbx {
            db_pool: self.db_pool.clone(),
            txn_holder: Arc::default(),
            with_txn: true,
        })
    }
}

/// 数据库事务（tokio-postgres 版）。
///
/// 持有一条独占的 deadpool 连接（`'static`）；事务边界通过文本命令驱动。
/// 保留 `DbTransaction` 命名以便 registry.rs / api.rs 与 sqlx 版对齐。
pub struct DbTransaction {
    /// 独占物理连接（deadpool Object，Drop 时归还池）
    conn: deadpool_postgres::Object,
    /// SAVEPOINT 嵌套深度（预留，第一版不使用）
    #[allow(dead_code)]
    savepoint_depth: u32,
}

impl std::fmt::Debug for DbTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbTransaction").finish_non_exhaustive()
    }
}

impl DbTransaction {
    /// 用一条已 BEGIN 的独占连接构造事务对象。
    fn new(conn: deadpool_postgres::Object) -> Self {
        DbTransaction {
            conn,
            savepoint_depth: 0,
        }
    }

    /// 执行无参 SQL，返回受影响行数。
    pub async fn execute(&mut self, sql: &str) -> Result<u64> {
        let n = self.conn.execute(sql, &[]).await?;
        Ok(n)
    }

    /// 执行带 `DataValue` 参数的 SQL。
    pub async fn execute_with_datavalues(
        &mut self,
        sql: &str,
        params: &[DataValue],
    ) -> Result<u64> {
        if params.is_empty() {
            return self.execute(sql).await;
        }
        let boxed = bind_data_values_pg(params);
        let refs = as_param_refs(&boxed);
        let n = self.conn.execute(sql, &refs).await?;
        Ok(n)
    }

    /// 执行带 JSON 参数的 SQL。
    pub async fn execute_with_json(
        &mut self,
        sql: &str,
        params: serde_json::Value,
    ) -> Result<u64> {
        let values: Vec<DataValue> =
            serde_json::from_value(params).map_err(|e| Error::InvalidParams(e.to_string()))?;
        self.execute_with_datavalues(sql, &values).await
    }

    /// 执行无参查询，返回 DataSet。
    pub async fn query(&mut self, sql: &str, dataset_id: &str) -> Result<DataSet> {
        let rows = self.conn.query(sql, &[]).await?;
        Ok(PgResultConverter::convert_rows(rows, dataset_id))
    }

    /// 执行带 `DataValue` 参数的查询，返回 DataSet。
    pub async fn query_with_datavalues(
        &mut self,
        sql: &str,
        params: &[DataValue],
        dataset_id: &str,
    ) -> Result<DataSet> {
        if params.is_empty() {
            return self.query(sql, dataset_id).await;
        }
        let boxed = bind_data_values_pg(params);
        let refs = as_param_refs(&boxed);
        let rows = self.conn.query(sql, &refs).await?;
        Ok(PgResultConverter::convert_rows(rows, dataset_id))
    }

    /// 执行带 JSON 参数的查询，返回 DataSet。
    pub async fn query_with_json(
        &mut self,
        sql: &str,
        params: serde_json::Value,
        dataset_id: &str,
    ) -> Result<DataSet> {
        let values: Vec<DataValue> =
            serde_json::from_value(params).map_err(|e| Error::InvalidParams(e.to_string()))?;
        self.query_with_datavalues(sql, &values, dataset_id).await
    }

    /// 执行 sea-query 生成的语句（参数 `sea_query::Values`）。
    pub async fn execute_with_seavalues(
        &mut self,
        sql: &str,
        params: sea_query::Values,
    ) -> Result<u64> {
        let boxed = sea_values_to_tosql(params)?;
        let refs = as_param_refs(&boxed);
        let n = self.conn.execute(sql, &refs).await?;
        Ok(n)
    }

    /// 查询 sea-query 生成的语句，返回 DataSet。
    pub async fn query_with_seavalues(
        &mut self,
        sql: &str,
        params: sea_query::Values,
        dataset_id: &str,
    ) -> Result<DataSet> {
        let boxed = sea_values_to_tosql(params)?;
        let refs = as_param_refs(&boxed);
        let rows = self.conn.query(sql, &refs).await?;
        Ok(PgResultConverter::convert_rows(rows, dataset_id))
    }

    /// 提交事务（COMMIT 文本命令；随后 Object Drop 归还池）。
    async fn commit(self) -> Result<()> {
        self.conn.batch_execute("COMMIT").await?;
        Ok(())
    }

    /// 回滚事务（ROLLBACK 文本命令；随后 Object Drop 归还池）。
    async fn rollback(self) -> Result<()> {
        self.conn.batch_execute("ROLLBACK").await?;
        Ok(())
    }
}

/// 事务持有器，管理事务和引用计数（与 sqlx 版一致）
#[derive(Debug)]
pub struct TxnHolder {
    /// 数据库事务实例
    pub txn: DbTransaction,
    /// 引用计数器
    pub counter: i32,
    /// 事务ID，全局唯一
    txn_id: String,
    /// 数据库ID
    db_id: String,
    /// 创建时间
    create_time: std::time::Instant,
}

impl TxnHolder {
    /// 构造事务持有者（引用计数初始为 1，分配新 txn_id）。
    pub fn new(txn: DbTransaction, db_id: &str) -> Self {
        TxnHolder {
            txn,
            counter: 1,
            txn_id: uuid::Uuid::new_v4().to_string(),
            db_id: db_id.to_string(),
            create_time: std::time::Instant::now(),
        }
    }

    /// 递增引用计数。
    pub fn inc(&mut self) {
        self.counter += 1;
    }

    /// 递减引用计数，返回递减后的值。
    pub fn dec(&mut self) -> i32 {
        self.counter -= 1;
        self.counter
    }

    /// 回滚事务（消费 self）
    pub async fn rollback(self) -> Result<()> {
        self.txn.rollback().await
    }

    /// 提交事务（消费 self）
    pub async fn commit(self) -> Result<()> {
        self.txn.commit().await
    }

    /// 返回事务 ID。
    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    /// 返回数据库 ID。
    pub fn db_id(&self) -> &str {
        &self.db_id
    }

    /// 返回事务已存活时长。
    pub fn elapsed(&self) -> std::time::Duration {
        self.create_time.elapsed()
    }
}

impl Deref for TxnHolder {
    type Target = DbTransaction;

    fn deref(&self) -> &Self::Target {
        &self.txn
    }
}

impl DerefMut for TxnHolder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.txn
    }
}

/// 事务传播行为（与 sqlx 版一致；仅 `Required` 真实现）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Propagation {
    /// REQUIRED (默认)：如果当前存在事务，则加入该事务；否则创建一个新事务
    Required,
    /// REQUIRES_NEW：创建一个新事务，如果当前存在事务，则将当前事务挂起
    RequiresNew,
    /// SUPPORTS：如果当前存在事务，则加入该事务；否则以非事务方式执行
    Supports,
    /// NOT_SUPPORTED：以非事务方式执行，如果当前存在事务，则将当前事务挂起
    NotSupported,
    /// MANDATORY：必须在事务中执行，如果当前不存在事务，则抛出异常
    Mandatory,
    /// NEVER：必须以非事务方式执行，如果当前存在事务，则抛出异常
    Never,
}
