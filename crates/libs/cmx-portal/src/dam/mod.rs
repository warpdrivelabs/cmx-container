//! DAM 注册表（domain / application / module 主数据）。
//!
//! 复刻 Node `lib/damRegistryStore.js`：normalize / load / save、三级 upsert（含改名级联 +
//! 11 个 DAM 树根目录搬移）、删除（带引用完整性校验）、`ensureDamTreeDirs`。
//!
//! 注册表文件：`dam-registry/registry.json`，结构 `{ version, domains[], applications[], modules[] }`。

pub mod store;
