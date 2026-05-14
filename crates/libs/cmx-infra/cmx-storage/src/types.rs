//! 公共类型定义模块
//!
//! 定义存储操作相关的公共数据结构，包括文件信息、上传/下载请求、
//! 分片上传、预签名、列举和查询等核心类型。

use std::collections::HashMap;

use bytes::Bytes;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 文件信息
///
/// 从数据库查询后返回给上层使用的完整文件信息，
/// 包含文件的基本属性、存储路径、缩略图信息和上传状态等。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileInfo {
    /// 文件唯一标识（UUID）
    pub id: String,
    /// 文件访问 URL
    pub url: String,
    /// 文件大小（字节）
    pub size: i64,
    /// 存储文件名（UUID 生成）
    pub filename: String,
    /// 原始文件名
    pub original_filename: Option<String>,
    /// 存储基础路径前缀
    pub base_path: Option<String>,
    /// 文件完整存储路径
    pub path: Option<String>,
    /// 文件扩展名
    pub ext: Option<String>,
    /// 文件 MIME 类型
    pub content_type: Option<String>,
    /// 存储平台标识
    pub platform: String,
    /// 缩略图访问 URL
    pub th_url: Option<String>,
    /// 缩略图存储路径
    pub th_path: Option<String>,
    /// 缩略图文件名
    pub th_filename: Option<String>,
    /// 缩略图文件大小（字节）
    pub th_size: Option<i64>,
    /// 缩略图 MIME 类型
    pub th_content_type: Option<String>,
    /// 关联对象 ID
    pub object_id: Option<String>,
    /// 关联对象类型
    pub object_type: Option<String>,
    /// 用户自定义元数据（JSON 字符串）
    pub user_metadata: Option<String>,
    /// 文件哈希信息（JSON）
    pub hash_info: Option<String>,
    /// 分片上传会话 ID
    pub upload_id: Option<String>,
    /// 上传状态：0-普通上传，1-初始化完成，2-上传完成
    pub upload_status: Option<i32>,
    /// 创建时间
    pub create_time: Option<NaiveDateTime>,
}

/// 文件下载结果
///
/// 包含下载的文件数据和相关的响应头信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDownload {
    /// 文件二进制数据
    pub data: Bytes,
    /// 文件详细信息
    pub file_info: FileInfo,
    /// 响应 Content-Type 头
    pub content_type: String,
    /// 响应 Content-Disposition 头
    pub content_disposition: String,
    /// 响应 Content-Length 头
    pub content_length: u64,
}

/// 上传请求
///
/// 上层调用存储服务上传文件时使用的请求结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadRequest {
    /// 文件二进制数据
    pub data: Bytes,
    /// 原始文件名
    pub original_filename: Option<String>,
    /// 文件 MIME 类型（若不指定则自动检测）
    pub content_type: Option<String>,
    /// 关联对象 ID
    pub object_id: Option<String>,
    /// 关联对象类型
    pub object_type: Option<String>,
    /// 存储平台标识（若不指定则使用默认平台）
    pub platform: Option<String>,
    /// 用户自定义元数据
    pub user_metadata: Option<HashMap<String, String>>,
    /// 访问控制列表
    pub acl: Option<String>,
}

/// 写入选项
///
/// 底层 StorageBackend 执行写入操作时使用的配置选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteOptions {
    /// 文件 MIME 类型
    pub content_type: Option<String>,
    /// 响应 Content-Disposition 头（RFC 5987）
    pub content_disposition: Option<String>,
    /// 缓存控制策略
    pub cache_control: Option<String>,
    /// 用户自定义元数据
    pub user_metadata: Option<HashMap<String, String>>,
    /// 访问控制列表
    pub acl: Option<String>,
}

/// 写入结果
///
/// 底层存储执行写入操作后返回的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    /// 对象 ETag 标识
    pub etag: Option<String>,
    /// 写入的数据长度（字节）
    pub content_length: u64,
}

/// 对象元数据
///
/// 底层 StorageBackend 执行 stat 操作后返回的对象元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    /// 对象存储路径
    pub path: String,
    /// 对象内容长度（字节）
    pub content_length: u64,
    /// 对象 MIME 类型
    pub content_type: Option<String>,
    /// 对象 ETag 标识
    pub etag: Option<String>,
    /// 最后修改时间
    pub last_modified: Option<NaiveDateTime>,
    /// 用户自定义元数据
    pub user_metadata: Option<HashMap<String, String>>,
}

/// 列举选项
///
/// 控制列举操作的返回数量和递归行为。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOptions {
    /// 最大返回条目数
    pub limit: Option<usize>,
    /// 是否递归列举子目录
    pub recursive: bool,
}

/// 列举条目
///
/// 列举操作返回的单个条目信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEntry {
    /// 对象路径
    pub path: String,
    /// 是否为目录
    pub is_dir: bool,
    /// 对象内容长度（字节）
    pub content_length: u64,
    /// 对象 MIME 类型
    pub content_type: Option<String>,
    /// 对象 ETag 标识
    pub etag: Option<String>,
    /// 最后修改时间
    pub last_modified: Option<NaiveDateTime>,
}

/// 存储能力
///
/// 描述存储后端支持的操作集合，用于运行时查询后端特性。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageCapabilities {
    /// 是否支持读取
    pub read: bool,
    /// 是否支持写入
    pub write: bool,
    /// 是否支持删除
    pub delete: bool,
    /// 是否支持列举
    pub list: bool,
    /// 是否支持复制
    pub copy: bool,
    /// 是否支持预签名（读取或写入）
    pub presign: bool,
    /// 是否支持预签名读取
    pub presign_read: bool,
    /// 是否支持预签名写入
    pub presign_write: bool,
    /// 是否支持创建目录
    pub create_dir: bool,
    /// 是否支持重命名
    pub rename: bool,
    /// 是否支持分片上传
    pub multipart: bool,
}

/// 文件查询条件
///
/// 用于构建文件列表查询的过滤和分页参数。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileQuery {
    /// 关联对象类型过滤
    pub object_type: Option<String>,
    /// 关联对象 ID 过滤
    pub object_id: Option<String>,
    /// 存储平台标识过滤
    pub platform: Option<String>,
    /// 当前页码（从 1 开始）
    pub page: Option<u32>,
    /// 每页条目数
    pub page_size: Option<u32>,
    /// 原始文件名过滤（模糊匹配）
    pub original_filename: Option<String>,
}

/// 分页结果
///
/// 包含分页查询的总数、页码信息和当前页数据列表。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilePage {
    /// 总记录数
    pub total: u64,
    /// 当前页码
    pub page: u32,
    /// 每页条目数
    pub page_size: u32,
    /// 当前页数据列表
    pub items: Vec<FileInfo>,
}

/// 分片上传会话
///
/// 初始化分片上传后返回的会话信息，包含所有分片的预签名 URL。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MultipartSession {
    /// 分片上传会话 ID
    pub upload_id: String,
    /// 关联的文件 ID
    pub file_id: String,
    /// 各分片的预签名 URL 列表
    pub presigned_urls: Vec<PresignedPartUrl>,
    /// 总分片数
    pub total_parts: u32,
}

/// 预签名分片 URL
///
/// 单个分片上传使用的预签名 URL 及其分片编号。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignedPartUrl {
    /// 分片编号（从 1 开始）
    pub part_number: u32,
    /// 分片上传预签名 URL
    pub upload_url: String,
}

/// 分片上传初始化请求
///
/// 发起分片上传时使用的请求参数。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MultipartInitRequest {
    /// 文件名
    pub filename: String,
    /// 总分片数
    pub total_parts: u32,
    /// 文件 MIME 类型
    pub content_type: Option<String>,
    /// 关联对象类型
    pub object_type: Option<String>,
    /// 关联对象 ID
    pub object_id: Option<String>,
    /// 存储平台标识
    pub platform: Option<String>,
}

/// 分片数据
///
/// 单个分片上传完成后的结果数据，用于完成分片上传时提交。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PartData {
    /// 分片上传会话 ID
    pub upload_id: String,
    /// 分片编号
    pub part_number: u32,
    /// 分片 ETag 标识
    pub e_tag: String,
    /// 分片数据大小（字节）
    pub part_size: i64,
}

/// 分片信息
///
/// 记录单个分片的上传状态和元数据。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PartInfo {
    /// 分片编号
    pub part_number: u32,
    /// 分片 ETag 标识
    pub e_tag: String,
    /// 分片数据大小（字节）
    pub part_size: i64,
}

/// 缩略图数据
///
/// 包含缩略图的二进制数据和对应的 MIME 类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailData {
    /// 缩略图二进制数据
    pub data: Bytes,
    /// 缩略图 MIME 类型
    pub content_type: String,
}

/// 预签名上传请求
///
/// 请求生成预签名上传 URL 时使用的参数。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignUploadRequest {
    /// 文件名
    pub filename: String,
    /// 文件 MIME 类型
    pub content_type: Option<String>,
    /// 存储平台标识
    pub platform: Option<String>,
}

/// 预签名上传结果
///
/// 包含预签名上传 URL 和创建的文件记录 ID。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PresignUploadResult {
    /// 预签名上传 URL
    pub url: String,
    /// 创建的文件记录 ID
    pub file_id: String,
}

// ==================== 数据库模型 ====================

/// 文件详情数据库模型
///
/// 对应 `file_detail` 表，用于与数据库交互。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDetail {
    /// 主键 ID
    pub id: String,
    /// 文件访问地址
    pub url: String,
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
    /// 文件所属对象 ID
    pub object_id: Option<String>,
    /// 文件所属对象类型
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
    /// 归档状态：0-正常，1-已删除
    pub archived: Option<i32>,
    /// 创建时间
    pub create_time: Option<NaiveDateTime>,
    /// 更新时间
    pub update_time: Option<NaiveDateTime>,
    /// 创建人 ID
    pub create_by: Option<String>,
    /// 创建人姓名
    pub create_name: Option<String>,
    /// 更新人 ID
    pub update_by: Option<String>,
    /// 更新人姓名
    pub update_name: Option<String>,
}

impl FileDetail {
    /// 将数据库模型转换为 FileInfo
    ///
    /// # Returns
    ///
    /// 转换后的业务层文件信息对象。
    pub fn to_file_info(&self) -> FileInfo {
        FileInfo {
            id: self.id.clone(),
            url: self.url.clone(),
            size: self.size.unwrap_or(0),
            filename: self.filename.clone().unwrap_or_default(),
            original_filename: self.original_filename.clone(),
            base_path: self.base_path.clone(),
            path: self.path.clone(),
            ext: self.ext.clone(),
            content_type: self.content_type.clone(),
            platform: self.platform.clone().unwrap_or_default(),
            th_url: self.th_url.clone(),
            th_path: self.th_path.clone(),
            th_filename: self.th_filename.clone(),
            th_size: self.th_size,
            th_content_type: self.th_content_type.clone(),
            object_id: self.object_id.clone(),
            object_type: self.object_type.clone(),
            user_metadata: self.user_metadata.clone(),
            hash_info: self.hash_info.clone(),
            upload_id: self.upload_id.clone(),
            upload_status: self.upload_status,
            create_time: self.create_time,
        }
    }
}

/// 文件分片信息数据库模型
///
/// 对应 `file_part_detail` 表，用于记录分片上传的每个分片信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePartDetail {
    /// 主键 ID
    pub id: String,
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
    /// 哈希信息（JSON）
    pub hash_info: Option<String>,
    /// 归档状态
    pub archived: Option<i32>,
    /// 创建时间
    pub create_time: Option<NaiveDateTime>,
    /// 更新时间
    pub update_time: Option<NaiveDateTime>,
    /// 创建人 ID
    pub create_by: Option<String>,
    /// 创建人姓名
    pub create_name: Option<String>,
    /// 更新人 ID
    pub update_by: Option<String>,
    /// 更新人姓名
    pub update_name: Option<String>,
}
