//! 可换服务插口 —— [`HierService`]。
//!
//! **依赖反转的核心**：trait 在这个中立 crate 里定义，**实现在服务侧**（`cmx-doc-store-pg`、
//! `cmx-dct-store-pg`，乃至任何支持层级结构的后端）。谁 `impl HierService`，谁就能当
//! [`CmxMasterSlave`](crate::CmxMasterSlave) 的数据服务——正如前端换 `cmx-doc-source.js` /
//! `cmx-dct-source.js`。**是服务依赖协调器，不是协调器依赖服务。**
//!
//! 加载态用 [`ZmcDataSet`](cmx_rowsource::ZmcDataSet)（零拷贝）；写入态用 [`ChangeSet`]。
//! 协调器（[`CmxMasterSlave::load_via`](crate::CmxMasterSlave::load_via) 等）泛型 over
//! `S: HierService`，故可对着 mock 单测，也可在运行时随时更换实现。

use async_trait::async_trait;
use cmx_rowsource::{ZmcDataSet, ZmcRowSource};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::changeset::{ChangeSet, SaveOutcome};
use crate::schema::HierSchema;

/// 装载查询（对齐前端 DocQuery 的中立子集）。服务侧按自己的定义/坐标翻译。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadQuery {
    /// 根层过滤（列名 → 值），如按 org / period。
    #[serde(default)]
    pub root_filter: Map<String, Value>,
    /// 根层分页 limit。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    /// 根层分页 offset。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    /// 装载深度（None = 全部）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// 是否要根层总数（分页）。
    #[serde(default)]
    pub count_total: bool,
}

/// 层级数据服务契约。服务侧 `impl` 它，把协调器接到自己现成的加载/保存上。
///
/// 关联类型 `Row: ZmcRowSource` 让服务返回自己驱动的零拷贝行（tokio-postgres 的
/// `TokioPgRowSource` / sqlx 的行），协调器不绑定具体驱动。
#[async_trait]
pub trait HierService: Send + Sync {
    /// 驱动的零拷贝行类型（如 `cmx_database_pg::TokioPgRowSource`）。
    type Row: ZmcRowSource;

    /// 装载一棵完整层级树，返回根层 [`ZmcDataSet`]（含 childRows）。
    async fn load(
        &self,
        schema: &HierSchema,
        query: &LoadQuery,
    ) -> Result<ZmcDataSet<Self::Row>, String>;

    /// 懒加载某层在给定父 id 下的子树（大树下钻）。
    async fn expand(
        &self,
        schema: &HierSchema,
        layer_path: &str,
        parent_ids: &[String],
    ) -> Result<ZmcDataSet<Self::Row>, String>;

    /// 保存一个变更集（服务侧现成 saver：校验 + 铸号 + 落库 + 乐观锁 + 派生列重算）。
    ///
    /// 注意：**写时上卷已由协调器在调用前完成**（见
    /// [`CmxMasterSlave::save_via`](crate::CmxMasterSlave::save_via)），故传入的 `changes`
    /// 里承接字段已是权威值，服务只管落库。
    async fn save(
        &self,
        schema: &HierSchema,
        changes: &ChangeSet,
    ) -> Result<SaveOutcome, String>;
}
