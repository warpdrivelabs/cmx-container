//! 数据库表元信息和实体定义
//!
//! 定义 `cmx_file_detail` 和 `cmx_file_part_detail` 表的 `DbBmc` 实现，
//! 以及对应的创建/更新/过滤实体类型。

use cmx_database::crud::DbBmc;
use modql::field::Fields;
use modql::filter::{FilterNodes, OpValInt64, OpValsInt32, OpValsInt64, OpValsString};
use serde::{Deserialize, Serialize};

/// 文件详情表的数据库操作元信息
///
/// 提供 `cmx_file_detail` 表的表名和主键列名。
pub struct FileDetailBmc;

impl DbBmc for FileDetailBmc {
    const TABLE: &'static str = "cmx_file_detail";
    const PK_COLUMN: &'static str = "id";
}

/// 文件分片信息表的数据库操作元信息
///
/// 提供 `cmx_file_part_detail` 表的表名和主键列名。
pub struct FilePartDetailBmc;

impl DbBmc for FilePartDetailBmc {
    const TABLE: &'static str = "cmx_file_part_detail";
    const PK_COLUMN: &'static str = "id";
}

/// 文件详情创建实体
///
/// 用于向 `cmx_file_detail` 表插入新记录。
/// 所有字段均为 `Option` 类型，可根据需要选择性设置。
#[derive(Debug, Clone, Serialize, Deserialize, Fields, Default)]
pub struct FileDetailForCreate {
    /// 文件唯一标识（UUID）
    pub id: Option<String>,
    /// 文件访问 URL
    pub url: Option<String>,
    /// 文件大小（字节）
    pub size: Option<i64>,
    /// 存储文件名（UUID 生成）
    pub filename: Option<String>,
    /// 原始文件名
    pub original_filename: Option<String>,
    /// 基础存储路径前缀
    pub base_path: Option<String>,
    /// 文件存储完整路径
    pub path: Option<String>,
    /// 文件扩展名
    pub ext: Option<String>,
    /// MIME 类型
    pub content_type: Option<String>,
    /// 存储平台标识
    pub platform: Option<String>,
    /// 缩略图访问 URL
    pub th_url: Option<String>,
    /// 缩略图存储路径
    pub th_path: Option<String>,
    /// 缩略图文件名
    pub th_filename: Option<String>,
    /// 缩略图大小（字节）
    pub th_size: Option<i64>,
    /// 缩略图 MIME 类型
    pub th_content_type: Option<String>,
    /// 关联对象 ID
    pub object_id: Option<String>,
    /// 关联对象类型
    pub object_type: Option<String>,
    /// 文件元数据（JSON）
    pub metadata: Option<String>,
    /// 用户自定义元数据（JSON）
    pub user_metadata: Option<String>,
    /// 缩略图元数据（JSON）
    pub th_metadata: Option<String>,
    /// 缩略图用户元数据（JSON）
    pub th_user_metadata: Option<String>,
    /// 附加属性（JSON）
    pub attr: Option<String>,
    /// 文件访问控制列表
    pub file_acl: Option<String>,
    /// 缩略图访问控制列表
    pub th_file_acl: Option<String>,
    /// 文件哈希信息（JSON）
    pub hash_info: Option<String>,
    /// 分片上传会话 ID
    pub upload_id: Option<String>,
    /// 上传状态：0-普通上传，1-初始化完成，2-上传完成
    pub upload_status: Option<i32>,
}

/// 文件详情更新实体
///
/// 用于更新 `cmx_file_detail` 表的现有记录。
/// 只包含可更新字段，未指定的字段不会被修改。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct FileDetailForUpdate {
    /// 文件访问 URL
    pub url: Option<String>,
    /// 文件大小（字节）
    pub size: Option<i64>,
    /// 存储文件名
    pub filename: Option<String>,
    /// 上传状态
    pub upload_status: Option<i32>,
    /// 缩略图访问 URL
    pub th_url: Option<String>,
    /// 缩略图存储路径
    pub th_path: Option<String>,
    /// 缩略图文件名
    pub th_filename: Option<String>,
    /// 缩略图大小（字节）
    pub th_size: Option<i64>,
    /// 缩略图 MIME 类型
    pub th_content_type: Option<String>,
    /// 归档状态：0-正常，1-已删除（归档）
    pub archived: Option<i32>,
}

/// 文件分片创建实体
///
/// 用于向 `cmx_file_part_detail` 表插入新分片记录。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct FilePartDetailForCreate {
    /// 分片唯一标识（UUID）
    pub id: Option<String>,
    /// 存储平台标识
    pub platform: Option<String>,
    /// 分片上传会话 ID
    pub upload_id: Option<String>,
    /// 分片 ETag
    pub e_tag: Option<String>,
    /// 分片编号（从 1 开始）
    pub part_number: Option<i32>,
    /// 分片大小（字节）
    pub part_size: Option<i64>,
    /// 分片哈希信息（JSON）
    pub hash_info: Option<String>,
}

/// 文件详情过滤器
///
/// 用于构建 `cmx_file_detail` 表的查询条件。
#[derive(Debug, Clone, Deserialize, FilterNodes, Default)]
pub struct FileDetailFilter {
    /// 文件 ID 过滤
    pub id: Option<OpValsString>,
    /// 存储平台标识过滤
    pub platform: Option<OpValsString>,
    /// 关联对象类型过滤
    pub object_type: Option<OpValsString>,
    /// 关联对象 ID 过滤
    pub object_id: Option<OpValsString>,
    /// 原始文件名过滤（支持模糊匹配）
    pub original_filename: Option<OpValsString>,
    /// 分片上传会话 ID 过滤
    pub upload_id: Option<OpValsString>,
    /// 文件哈希信息过滤
    pub hash_info: Option<OpValsString>,
    /// 归档状态过滤
    pub archived: Option<OpValsInt64>,
}

impl FileDetailFilter {
    /// 添加排除已删除的条件
    ///
    /// 设置 `archived = 0`，仅返回未归档的记录。
    pub fn with_active_only(&mut self) {
        self.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
    }
}

/// 文件分片过滤器
///
/// 用于构建 `cmx_file_part_detail` 表的查询条件。
#[derive(Debug, Clone, Deserialize, FilterNodes, Default)]
pub struct FilePartDetailFilter {
    /// 分片 ID 过滤
    pub id: Option<OpValsString>,
    /// 存储平台标识过滤
    pub platform: Option<OpValsString>,
    /// 分片上传会话 ID 过滤
    pub upload_id: Option<OpValsString>,
    /// 分片 ETag 过滤
    pub e_tag: Option<OpValsString>,
    /// 分片编号过滤
    pub part_number: Option<OpValsInt32>,
    /// 分片大小过滤
    pub part_size: Option<OpValsInt64>,
    /// 分片哈希信息过滤
    pub hash_info: Option<OpValsString>,
}
