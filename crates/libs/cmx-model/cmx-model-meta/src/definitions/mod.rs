//! 定义中心（DCT 数据字典 / DOC 业务单据 / BASE 字段集模板）设计期元数据读写。
//!
//! 复刻 Node `lib/definitionsStore.js`：
//! 路径 `meta/definitions/<domain>/<app>/<module>/<file>.json`；`base` 域特例为 `meta/definitions/base/<file>.json`。
//! - list：递归扫描，summarize 抽列表项（按 metaKind 区分 DCT/DOC/BASE）。
//! - get / save（补 updatedAt）/ delete（base 不可删）。
//! - batch：批量读 + 一次性附带各主元数据引用的 base 字段集文件（去重）。

pub mod store;

// 业务编码 → 定义文件解析（DOC/DCT 共享，供 cmx-api/cmx-doc-api/cmx-dct-api 三方共用，避免环）。
pub mod resolve;

// DAM 坐标全局反查与定义树代数（DAM 可选化基础设施，供 cmx-dct/cmx-doc 咽喉点共用）。
pub mod coord;
