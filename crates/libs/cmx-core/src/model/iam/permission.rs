//! 权限基础数据模型（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 权限基础数据模型（WASM 可见）。
///
/// 权限描述系统中的一个可执行操作（如菜单、按钮、API 资源），
/// 支持树形结构（通过 `parent_id` 关联）和资源类型分类。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Permission {
    /// 权限唯一标识（主键）。
    pub id: String,

    /// 权限编码（如 `system:user:add`），业务唯一，用于鉴权匹配。
    pub code: String,

    /// 权限名称，用于展示。
    pub name: String,

    /// 资源类型（如 `menu` / `button` / `api`），可空。
    #[serde(default)]
    pub resource_type: Option<String>,

    /// 父权限 ID，根权限为 `None`，可空。
    #[serde(default)]
    pub parent_id: Option<String>,

    /// 排序号，升序排列，可空。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 状态（如 1 启用 / 0 禁用），可空。
    #[serde(default)]
    pub status: Option<i64>,

    /// 权限描述/备注，可空。
    #[serde(default)]
    pub description: Option<String>,

    /// 所属域编码（如 `platform`、`tenant`），可空。
    #[serde(default)]
    pub domain_code: Option<String>,

    /// 所属应用编码（如 `user-center`、`billing`），可空。
    #[serde(default)]
    pub app_code: Option<String>,

    /// 所属模块编码（如 `user`、`order`），可空。
    #[serde(default)]
    pub module_code: Option<String>,

    /// 扩展配置（用户自定义 JSON 文本），可空。
    #[serde(default)]
    pub extension: Option<String>,

    /// 父权限编码（根为 `None`），冗余字段便于直接读取。
    #[serde(default)]
    pub parent_code: Option<String>,

    /// code 全路径（如 `/user:list/user:delete`），用于 LIKE 查询子树。
    #[serde(default)]
    pub full_code_path: Option<String>,

    /// 是否叶子节点（1 叶子 / 0 非叶子）。
    #[serde(default)]
    pub is_leaf: Option<i64>,

    /// 层级深度（根 = 1，子 = 父 + 1）。
    #[serde(default)]
    pub level: Option<i64>,

    /// 归档标记（如 1 已归档 / 0 正常），可空。
    #[serde(default)]
    pub archived: Option<i64>,

    /// 创建时间（ISO8601 字符串），可空。
    #[serde(default)]
    pub create_time: Option<String>,

    /// 更新时间（ISO8601 字符串），可空。
    #[serde(default)]
    pub update_time: Option<String>,

    /// 创建人 ID，可空。
    #[serde(default)]
    pub create_by: Option<String>,

    /// 创建人姓名，可空。
    #[serde(default)]
    pub create_name: Option<String>,

    /// 更新人 ID，可空。
    #[serde(default)]
    pub update_by: Option<String>,

    /// 更新人姓名，可空。
    #[serde(default)]
    pub update_name: Option<String>,
}

// ===== 权限导入/导出契约（PermissionFile / PermissionDefinition）=====
//
// 这两个结构是模块包 permissions/*.json 的序列化契约,
// 同时被 cmx-iam(插件权限导入) 和 cmx-plugin(模块导入/导出) 使用。
// 定义在 cmx-core 中避免三处副本(导出 ad-hoc json!、导入私有 inline struct、cmx-iam 规范)漂移。

/// 权限定义（对应权限文件中的单条条目，导入/导出契约）。
///
/// 用于从 `permdata/*.json` / 模块包 `permissions/*.json` 反序列化,
/// 与入库的 `Permission` 实体解耦。`parent_code` 在第二阶段被解析为 `parent_id`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionDefinition {
    /// 权限编码（必须含 `:` 分隔符，如 `user:list`）。
    pub code: String,
    /// 权限名称。
    pub name: String,
    /// 资源类型（`api`/`menu`/`button`，未指定默认 `api`）。
    #[serde(default)]
    pub resource_type: Option<String>,
    /// 父权限编码（用 code 引用，接收端解析为 parent_id）。
    #[serde(default)]
    pub parent_code: Option<String>,
    /// 排序序号（默认 0）。
    #[serde(default)]
    pub sort_order: Option<i64>,
    /// 权限描述。
    #[serde(default)]
    pub description: Option<String>,
    /// 扩展配置（JSON 字符串）。
    #[serde(default)]
    pub extension: Option<String>,
    /// 状态（1-启用，0-禁用，默认 1）。
    #[serde(default)]
    pub status: Option<i64>,
}

/// 权限文件（对应 `permdata/` 目录下的单个 JSON 文件）。
///
/// `name`/`version`/`description` 为元数据，不入库；
/// `permissions` 为实际权限定义列表，合并后统一处理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionFile {
    /// 文件描述名称（元数据，不入库）。
    #[serde(default)]
    pub name: String,
    /// 文件版本（元数据，不入库）。
    #[serde(default)]
    pub version: String,
    /// 文件描述（元数据，不入库）。
    #[serde(default)]
    pub description: String,
    /// 权限定义列表。
    pub permissions: Vec<PermissionDefinition>,
}
