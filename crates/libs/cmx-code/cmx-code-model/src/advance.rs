//! Advance trait：DB 操作抽象（由 cmx-code-api 实现，依赖注入传入）。
//!
//! model 层不直接依赖任何 DB crate——所有 DB 操作（反查 max、取断号、UNIQUE 重试插入）
//! 抽象为 [`Advance`] trait，由 `cmx-code-api` 的 `serial_pg.rs` / `random_pg.rs` 实现。
//! 钩子层（cmx-dct-store-pg / cmx-doc-store-pg）通过依赖注入拿到 `&dyn Advance`。

use async_trait::async_trait;

use crate::error::Result;
use crate::spec::Target;

/// DB 操作抽象（反查 max / 取断号 / 插入重试）。
///
/// 实现方：`cmx-code-api::store::serial_pg::PgAdvance`。
/// 测试用：[`StubAdvance`]（query_max 返回 0、take_gap 返回 None、try_insert 恒 Ok）。
#[async_trait]
pub trait Advance: Send + Sync {
    /// 反查业务表 code 列的当前前缀下最大流水号。
    ///
    /// `minted_buffer` 非空时，把 buffer 里的号也 union 进候选集（同事务多行铸号推进 max）。
    /// 返回 0 表示无历史号（首次铸号）。
    async fn query_max_serial(
        &self,
        target: &Target,
        prefix: &str,
        width: usize,
        minted_buffer: &[String],
    ) -> Result<i64>;

    /// 取一个断号（断号补偿，enable_gap=true 时优先调）。
    ///
    /// 返回 None 表示无断号或未启用断号补偿（走反查 max）。
    async fn take_gap(&self, prefix: &str, width: usize) -> Result<Option<i64>>;

    /// 尝试插入候选号（UNIQUE 冲突时返回 `Err` 触发 `evaluate_segments` 重试）。
    ///
    /// ## 责任边界（重要）
    ///
    /// 当前实现（`PgAdvance`）返回 `Ok(())` —— 铸号阶段**不做真实 INSERT**，这是设计决策：
    /// DCT/DOC saver 的铸号发生在 apply_merge 之前（钩子算号写回 changeset），真正的 INSERT
    /// 由 saver 的 apply_merge / write 完成，业务表的 UNIQUE 约束在那里兜底。
    ///
    /// 因此 `evaluate_segments` 的重试循环在铸号阶段恒不触发，UNIQUE 冲突重试责任上移到
    /// saver 层（saver 落库冲突 → 重新调 mint 取下一号）。这是有意为之 —— 铸号函数只算号不落库。
    ///
    /// 如果未来需要在铸号阶段预检（如 `SELECT EXISTS` 查重），在此实现并返回任意 `Err`
    /// 即可触发 `evaluate_segments` 的重试循环。
    async fn try_insert(&self, target: &Target, code: &str) -> Result<()>;
}

/// 测试用 stub：query_max 返回 0、take_gap 返回 None、try_insert 恒 Ok。
///
/// 配合 `evaluate_segments` 做纯逻辑单测，不碰 DB。
pub struct StubAdvance;

#[async_trait]
impl Advance for StubAdvance {
    async fn query_max_serial(
        &self,
        _target: &Target,
        _prefix: &str,
        _width: usize,
        _minted_buffer: &[String],
    ) -> Result<i64> {
        Ok(0)
    }

    async fn take_gap(&self, _prefix: &str, _width: usize) -> Result<Option<i64>> {
        Ok(None)
    }

    async fn try_insert(&self, _target: &Target, _code: &str) -> Result<()> {
        Ok(())
    }
}
