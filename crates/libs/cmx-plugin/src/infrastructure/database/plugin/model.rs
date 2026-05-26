//! 插件数据模型
//!
//! 定义与 `cmx_plugin` 表对应的结构体：
//! - `PluginRecord`: 查询结果记录（包含 JOIN 补充字段）
//! - `PluginCreateParams`: 创建/写入参数（仅数据库列字段）
//! - `PluginUpdateParams`: 更新参数（所有字段可选）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件查询结果记录（数据库行映射 + JOIN 补充字段）
///
/// 用于 Repository 查询操作的返回类型。
/// 前 30 个字段与 `cmx_plugin` 表列一一对应，
/// 后 3 个字段（domain_name/application_name/module_name）
/// 来自 LEFT JOIN cmx_domain/application/module 的补充查询。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRecord {
    // === cmx_plugin 表字段 (32列) ===
    /// 主键ID (对应列: id)
    pub id: String,
    /// 应用ID (对应列: app_id)
    pub app_id: String,
    /// 插件唯一标识 (对应列: plugin_id)
    pub plugin_id: String,
    /// 显示名称 (对应列: name)
    pub name: String,
    /// 插件描述 (对应列: description)
    pub description: Option<String>,
    /// 当前版本 (对应列: version)
    pub version: String,
    /// WASM 文件绝对路径 (对应列: wasm_path)
    pub wasm_path: String,
    /// 安装根目录路径 (对应列: install_path)
    pub install_path: String,
    /// 插件业务数据存储的数据库ID (对应列: db_id)
    pub db_id: String,
    /// 状态: installed/activated/deactivated/error (对应列: status)
    pub status: String,
    /// 是否系统默认插件 (对应列: is_system)
    pub is_system: bool,
    /// 是否被锁定防止卸载 (对应列: is_locked)
    pub is_locked: bool,
    /// 所属域编码 (对应列: domain_code)
    pub domain_code: Option<String>,
    /// 所属应用编码 (对应列: application_code)
    pub application_code: Option<String>,
    /// 所属模块编码 (对应列: module_code)
    pub module_code: Option<String>,
    /// 开发商名称 (对应列: vendor_name)
    pub vendor_name: Option<String>,
    /// 开发商URL (对应列: vendor_url)
    pub vendor_url: Option<String>,
    /// 开发商联系方式 (对应列: vendor_contact)
    pub vendor_contact: Option<String>,
    /// 扩展元数据 (对应列: metadata, JSONB)
    pub metadata: Option<serde_json::Value>,
    /// 签名算法 (对应列: signature_algorithm)
    pub signature_algorithm: Option<String>,
    /// 签名密钥ID (对应列: signer_key_id)
    pub signer_key_id: Option<String>,
    /// 插件ZIP包来源地址 (对应列: zip_source_url)
    pub zip_source_url: Option<String>,
    /// 插件来源类型: local/url/registry (对应列: zip_source_type)
    pub zip_source_type: Option<String>,
    /// 插件类型: wasm/rhai (对应列: plugin_type)
    pub plugin_type: Option<String>,
    /// 源码路径 (对应列: source_path)
    pub source_path: Option<String>,
    /// 市场版本来源 ID，关联 `cmx_marketplace_plugin_version.id`。
    pub marketplace_source_id: Option<String>,
    /// 存储键 (对应列: storage_key)
    pub storage_key: Option<String>,
    /// 存储校验和 (对应列: storage_checksum)
    pub storage_checksum: Option<String>,
    /// 创建时间 (对应列: create_time)
    pub create_time: DateTime<Utc>,
    /// 更新时间 (对应列: update_time)
    pub update_time: DateTime<Utc>,
    /// 归档标志: 0-未归档，1-已归档 (对应列: archived)
    pub archived: i32,
    /// 创建人ID (对应列: create_by)
    pub create_by: Option<String>,
    /// 创建人名称 (对应列: create_name)
    pub create_name: Option<String>,
    /// 更新人ID (对应列: update_by)
    pub update_by: Option<String>,
    /// 更新人名称 (对应列: update_name)
    pub update_name: Option<String>,

    // === JOIN 补充字段 (非数据库列) ===
    /// 域名称 (JOIN cmx_domain.name)
    pub domain_name: Option<String>,
    /// 应用名称 (JOIN cmx_application.name)
    pub application_name: Option<String>,
    /// 模块名称 (JOIN cmx_module.name)
    pub module_name: Option<String>,
}

/// 插件创建参数（用于 INSERT / UPSERT 操作）
///
/// 仅包含写入数据库时需要的 33 个字段，
/// 不包含 JOIN 补充字段（domain_name/application_name/module_name）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCreateParams {
    /// 主键ID
    pub id: String,
    /// 应用ID
    pub app_id: String,
    /// 插件唯一标识
    pub plugin_id: String,
    /// 显示名称
    pub name: String,
    /// 插件描述
    pub description: Option<String>,
    /// 当前版本
    pub version: String,
    /// WASM 文件绝对路径
    pub wasm_path: String,
    /// 安装根目录路径
    pub install_path: String,
    /// 数据库ID
    pub db_id: String,
    /// 状态
    pub status: String,
    /// 是否系统插件
    pub is_system: bool,
    /// 是否锁定
    pub is_locked: bool,
    /// 所属域编码
    pub domain_code: Option<String>,
    /// 所属应用编码
    pub application_code: Option<String>,
    /// 所属模块编码
    pub module_code: Option<String>,
    /// 开发商名称
    pub vendor_name: Option<String>,
    /// 开发商URL
    pub vendor_url: Option<String>,
    /// 开发商联系方式
    pub vendor_contact: Option<String>,
    /// 扩展元数据
    pub metadata: Option<serde_json::Value>,
    /// 签名算法
    pub signature_algorithm: Option<String>,
    /// 签名密钥ID
    pub signer_key_id: Option<String>,
    /// 插件ZIP包来源地址
    pub zip_source_url: Option<String>,
    /// 插件来源类型
    pub zip_source_type: Option<String>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,
    /// 市场版本来源 ID。
    pub marketplace_source_id: Option<String>,
    /// 存储键
    pub storage_key: Option<String>,
    /// 存储校验和
    pub storage_checksum: Option<String>,
    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 归档标志
    pub archived: i32,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

impl PluginCreateParams {
    /// 转换为 PluginRecord（查询结果类型）
    ///
    /// JOIN 补充字段设为 None
    pub fn to_record(&self) -> PluginRecord {
        PluginRecord {
            id: self.id.clone(),
            app_id: self.app_id.clone(),
            plugin_id: self.plugin_id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            version: self.version.clone(),
            wasm_path: self.wasm_path.clone(),
            install_path: self.install_path.clone(),
            db_id: self.db_id.clone(),
            status: self.status.clone(),
            is_system: self.is_system,
            is_locked: self.is_locked,
            domain_code: self.domain_code.clone(),
            application_code: self.application_code.clone(),
            module_code: self.module_code.clone(),
            vendor_name: self.vendor_name.clone(),
            vendor_url: self.vendor_url.clone(),
            vendor_contact: self.vendor_contact.clone(),
            metadata: self.metadata.clone(),
            signature_algorithm: self.signature_algorithm.clone(),
            signer_key_id: self.signer_key_id.clone(),
            zip_source_url: self.zip_source_url.clone(),
            zip_source_type: self.zip_source_type.clone(),
            plugin_type: self.plugin_type.clone(),
            source_path: self.source_path.clone(),
            marketplace_source_id: self.marketplace_source_id.clone(),
            storage_key: self.storage_key.clone(),
            storage_checksum: self.storage_checksum.clone(),
            create_time: self.create_time,
            update_time: self.update_time,
            archived: self.archived,
            create_by: self.create_by.clone(),
            create_name: self.create_name.clone(),
            update_by: self.update_by.clone(),
            update_name: self.update_name.clone(),
            domain_name: None,
            application_name: None,
            module_name: None,
        }
    }
}

/// 插件更新参数（用于 UPDATE 操作，所有字段可选）
///
/// 仅包含需要更新的字段，未设置（None）的字段保持原值不变。
/// 更新条件为 WHERE plugin_id = ?。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginUpdateParams {
    /// 显示名称
    pub name: Option<String>,
    /// 插件描述
    pub description: Option<String>,
    /// 当前版本
    pub version: Option<String>,
    /// WASM 文件绝对路径
    pub wasm_path: Option<String>,
    /// 安装根目录路径
    pub install_path: Option<String>,
    /// 数据库ID
    pub db_id: Option<String>,
    /// 状态
    pub status: Option<String>,
    /// 是否系统插件
    pub is_system: Option<bool>,
    /// 是否锁定
    pub is_locked: Option<bool>,
    /// 域编码
    pub domain_code: Option<String>,
    /// 应用编码
    pub application_code: Option<String>,
    /// 模块编码
    pub module_code: Option<String>,
    /// 开发商名称
    pub vendor_name: Option<String>,
    /// 开发商URL
    pub vendor_url: Option<String>,
    /// 开发商联系方式
    pub vendor_contact: Option<String>,
    /// 扩展元数据
    pub metadata: Option<serde_json::Value>,
    /// 签名算法
    pub signature_algorithm: Option<String>,
    /// 签名密钥ID
    pub signer_key_id: Option<String>,
    /// 插件ZIP包来源地址
    pub zip_source_url: Option<String>,
    /// 插件来源类型
    pub zip_source_type: Option<String>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,
    /// 市场版本来源 ID。
    pub marketplace_source_id: Option<String>,
    /// 应用ID
    pub app_id: Option<String>,
    /// 存储键
    pub storage_key: Option<String>,
    /// 存储校验和
    pub storage_checksum: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}
