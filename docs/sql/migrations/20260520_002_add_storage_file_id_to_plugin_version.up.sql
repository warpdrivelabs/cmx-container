ALTER TABLE cmx_marketplace_plugin_version
    ADD COLUMN IF NOT EXISTS storage_file_id VARCHAR (64);

COMMENT
ON COLUMN cmx_marketplace_plugin_version.storage_file_id IS 'cmx-storage 文件唯一标识，关联 cmx_file_detail.id';

CREATE TABLE IF NOT EXISTS cmx_file_detail
(
    id
    VARCHAR
(
    64
) NOT NULL CONSTRAINT pk_file_detail PRIMARY KEY,
    url VARCHAR
(
    512
) NOT NULL,
    size BIGINT,
    filename VARCHAR
(
    256
),
    original_filename VARCHAR
(
    256
),
    base_path VARCHAR
(
    256
),
    path VARCHAR
(
    256
),
    ext VARCHAR
(
    32
),
    content_type VARCHAR
(
    128
),
    platform VARCHAR
(
    32
),
    th_url VARCHAR
(
    512
),
    th_filename VARCHAR
(
    256
),
    th_size BIGINT,
    th_content_type VARCHAR
(
    128
),
    object_id VARCHAR
(
    64
),
    object_type VARCHAR
(
    32
),
    metadata TEXT,
    user_metadata TEXT,
    th_metadata TEXT,
    th_user_metadata TEXT,
    attr TEXT,
    file_acl VARCHAR
(
    32
),
    th_file_acl VARCHAR
(
    32
),
    hash_info TEXT,
    upload_id VARCHAR
(
    128
),
    upload_status INTEGER,
    archived INTEGER DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR
(
    100
),
    create_name VARCHAR
(
    100
),
    update_by VARCHAR
(
    100
),
    update_name VARCHAR
(
    100
)
    );

COMMENT
ON TABLE cmx_file_detail IS '文件详情表';
COMMENT
ON COLUMN cmx_file_detail.id IS '主键ID';
COMMENT
ON COLUMN cmx_file_detail.url IS '文件访问地址';
COMMENT
ON COLUMN cmx_file_detail.size IS '文件大小，单位字节';
COMMENT
ON COLUMN cmx_file_detail.filename IS '文件名称';
COMMENT
ON COLUMN cmx_file_detail.original_filename IS '原始文件名';
COMMENT
ON COLUMN cmx_file_detail.base_path IS '基础存储路径';
COMMENT
ON COLUMN cmx_file_detail.path IS '存储路径';
COMMENT
ON COLUMN cmx_file_detail.ext IS '文件扩展名';
COMMENT
ON COLUMN cmx_file_detail.content_type IS 'MIME类型';
COMMENT
ON COLUMN cmx_file_detail.platform IS '存储平台标识';
COMMENT
ON COLUMN cmx_file_detail.th_url IS '缩略图访问路径';
COMMENT
ON COLUMN cmx_file_detail.th_filename IS '缩略图名称';
COMMENT
ON COLUMN cmx_file_detail.th_size IS '缩略图大小，单位字节';
COMMENT
ON COLUMN cmx_file_detail.th_content_type IS '缩略图MIME类型';
COMMENT
ON COLUMN cmx_file_detail.object_id IS '文件所属对象ID';
COMMENT
ON COLUMN cmx_file_detail.object_type IS '文件所属对象类型';
COMMENT
ON COLUMN cmx_file_detail.metadata IS '文件元数据';
COMMENT
ON COLUMN cmx_file_detail.user_metadata IS '文件用户元数据';
COMMENT
ON COLUMN cmx_file_detail.th_metadata IS '缩略图元数据';
COMMENT
ON COLUMN cmx_file_detail.th_user_metadata IS '缩略图用户元数据';
COMMENT
ON COLUMN cmx_file_detail.attr IS '附加属性';
COMMENT
ON COLUMN cmx_file_detail.file_acl IS '文件ACL';
COMMENT
ON COLUMN cmx_file_detail.th_file_acl IS '缩略图文件ACL';
COMMENT
ON COLUMN cmx_file_detail.hash_info IS '哈希信息（JSON格式，含MD5等）';
COMMENT
ON COLUMN cmx_file_detail.upload_id IS '上传ID，仅在手动分片上传时使用';
COMMENT
ON COLUMN cmx_file_detail.upload_status IS '上传状态：0-普通上传，1-初始化完成，2-上传完成';
COMMENT
ON COLUMN cmx_file_detail.archived IS '是否归档：0-否，1-是';
COMMENT
ON COLUMN cmx_file_detail.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_file_detail.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_file_detail.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_file_detail.create_name IS '创建人姓名';
COMMENT
ON COLUMN cmx_file_detail.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_file_detail.update_name IS '更新人姓名';

CREATE INDEX IF NOT EXISTS idx_file_detail_platform ON cmx_file_detail (platform);
CREATE INDEX IF NOT EXISTS idx_file_detail_object_type ON cmx_file_detail (object_type);
CREATE INDEX IF NOT EXISTS idx_file_detail_upload_id ON cmx_file_detail (upload_id);

CREATE TABLE IF NOT EXISTS cmx_file_part_detail
(
    id
    VARCHAR
(
    64
) NOT NULL CONSTRAINT pk_file_part_detail PRIMARY KEY,
    platform VARCHAR
(
    32
),
    upload_id VARCHAR
(
    128
),
    e_tag VARCHAR
(
    255
),
    part_number INTEGER,
    part_size BIGINT,
    hash_info TEXT,
    archived INTEGER DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR
(
    100
),
    create_name VARCHAR
(
    100
),
    update_by VARCHAR
(
    100
),
    update_name VARCHAR
(
    100
)
    );

COMMENT
ON TABLE cmx_file_part_detail IS '文件分片信息表，仅在手动分片上传时使用';
COMMENT
ON COLUMN cmx_file_part_detail.id IS '主键ID';
COMMENT
ON COLUMN cmx_file_part_detail.platform IS '存储平台标识';
COMMENT
ON COLUMN cmx_file_part_detail.upload_id IS '上传ID';
COMMENT
ON COLUMN cmx_file_part_detail.e_tag IS '分片ETag';
COMMENT
ON COLUMN cmx_file_part_detail.part_number IS '分片号';
COMMENT
ON COLUMN cmx_file_part_detail.part_size IS '分片大小，单位字节';
COMMENT
ON COLUMN cmx_file_part_detail.hash_info IS '哈希信息';
COMMENT
ON COLUMN cmx_file_part_detail.archived IS '是否归档：0-否，1-是';
COMMENT
ON COLUMN cmx_file_part_detail.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_file_part_detail.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_file_part_detail.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_file_part_detail.create_name IS '创建人姓名';
COMMENT
ON COLUMN cmx_file_part_detail.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_file_part_detail.update_name IS '更新人姓名';

CREATE INDEX IF NOT EXISTS idx_file_part_detail_upload_id ON cmx_file_part_detail (upload_id);

ALTER TABLE cmx_plugin
    ADD COLUMN marketplace_source_id VARCHAR(64);
COMMENT
ON COLUMN cmx_plugin.marketplace_source_id IS '市场版本来源ID，关联 cmx_marketplace_plugin_version.id，非市场安装时为 NULL';

ALTER TABLE cmx_plugin_versions
    ADD COLUMN marketplace_source_id VARCHAR(64);
COMMENT
ON COLUMN cmx_plugin_versions.marketplace_source_id IS '市场版本来源ID，关联 cmx_marketplace_plugin_version.id';
