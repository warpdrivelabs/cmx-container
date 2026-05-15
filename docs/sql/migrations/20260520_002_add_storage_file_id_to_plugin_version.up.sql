ALTER TABLE cmx_marketplace_plugin_version
    ADD COLUMN IF NOT EXISTS storage_file_id VARCHAR (64);

COMMENT
ON COLUMN cmx_marketplace_plugin_version.storage_file_id
IS 'cmx-storage 文件唯一标识，关联 cmx_file_detail.id';


CREATE TABLE IF NOT EXISTS cmx_file_detail
(
    id
    varchar
(
    64
) not null
    constraint pk_file_detail
    primary key,
    url varchar
(
    512
) not null,
    size bigint,
    filename varchar
(
    256
),
    original_filename varchar
(
    256
),
    base_path varchar
(
    256
),
    path varchar
(
    256
),
    ext varchar
(
    32
),
    content_type varchar
(
    128
),
    platform varchar
(
    32
),
    th_url varchar
(
    512
),
    th_filename varchar
(
    256
),
    th_size bigint,
    th_content_type varchar
(
    128
),
    object_id varchar
(
    64
),
    object_type varchar
(
    32
),
    metadata text,
    user_metadata text,
    th_metadata text,
    th_user_metadata text,
    attr text,
    file_acl varchar
(
    32
),
    th_file_acl varchar
(
    32
),
    hash_info text,
    upload_id varchar
(
    128
),
    upload_status integer,
    archived integer default 0,
    create_time timestamp default CURRENT_TIMESTAMP,
    update_time timestamp default CURRENT_TIMESTAMP,
    create_by varchar
(
    100
),
    create_name varchar
(
    100
),
    update_by varchar
(
    100
),
    update_name varchar
(
    100
)
    );

comment
on table cmx_file_detail is '文件详情表';

comment
on column cmx_file_detail.id is '主键ID';
comment
on column cmx_file_detail.url is '文件访问地址';
comment
on column cmx_file_detail.size is '文件大小，单位字节';
comment
on column cmx_file_detail.filename is '文件名称';
comment
on column cmx_file_detail.original_filename is '原始文件名';
comment
on column cmx_file_detail.base_path is '基础存储路径';
comment
on column cmx_file_detail.path is '存储路径';
comment
on column cmx_file_detail.ext is '文件扩展名';
comment
on column cmx_file_detail.content_type is 'MIME类型';
comment
on column cmx_file_detail.platform is '存储平台标识';
comment
on column cmx_file_detail.th_url is '缩略图访问路径';
comment
on column cmx_file_detail.th_filename is '缩略图名称';
comment
on column cmx_file_detail.th_size is '缩略图大小，单位字节';
comment
on column cmx_file_detail.th_content_type is '缩略图MIME类型';
comment
on column cmx_file_detail.object_id is '文件所属对象ID';
comment
on column cmx_file_detail.object_type is '文件所属对象类型';
comment
on column cmx_file_detail.metadata is '文件元数据';
comment
on column cmx_file_detail.user_metadata is '文件用户元数据';
comment
on column cmx_file_detail.th_metadata is '缩略图元数据';
comment
on column cmx_file_detail.th_user_metadata is '缩略图用户元数据';
comment
on column cmx_file_detail.attr is '附加属性';
comment
on column cmx_file_detail.file_acl is '文件ACL';
comment
on column cmx_file_detail.th_file_acl is '缩略图文件ACL';
comment
on column cmx_file_detail.hash_info is '哈希信息（JSON格式，含MD5等）';
comment
on column cmx_file_detail.upload_id is '上传ID，仅在手动分片上传时使用';
comment
on column cmx_file_detail.upload_status is '上传状态：0-普通上传，1-初始化完成，2-上传完成';
comment
on column cmx_file_detail.archived is '是否归档：0-否，1-是';
comment
on column cmx_file_detail.create_time is '创建时间';
comment
on column cmx_file_detail.update_time is '更新时间';
comment
on column cmx_file_detail.create_by is '创建人ID';
comment
on column cmx_file_detail.create_name is '创建人姓名';
comment
on column cmx_file_detail.update_by is '更新人ID';
comment
on column cmx_file_detail.update_name is '更新人姓名';

CREATE INDEX IF NOT EXISTS idx_file_detail_platform
    on cmx_file_detail (platform);

CREATE INDEX IF NOT EXISTS idx_file_detail_object_type
    on cmx_file_detail (object_type);

CREATE INDEX IF NOT EXISTS idx_file_detail_upload_id
    on cmx_file_detail (upload_id);

CREATE TABLE IF NOT EXISTS cmx_file_part_detail
(
    id
    varchar
(
    64
) not null
    constraint pk_file_part_detail
    primary key,
    platform varchar
(
    32
),
    upload_id varchar
(
    128
),
    e_tag varchar
(
    255
),
    part_number integer,
    part_size bigint,
    hash_info text,
    archived integer default 0,
    create_time timestamp default CURRENT_TIMESTAMP,
    update_time timestamp default CURRENT_TIMESTAMP,
    create_by varchar
(
    100
),
    create_name varchar
(
    100
),
    update_by varchar
(
    100
),
    update_name varchar
(
    100
)
    );

comment
on table cmx_file_part_detail is '文件分片信息表，仅在手动分片上传时使用';

comment
on column cmx_file_part_detail.id is '主键ID';
comment
on column cmx_file_part_detail.platform is '存储平台标识';
comment
on column cmx_file_part_detail.upload_id is '上传ID';
comment
on column cmx_file_part_detail.e_tag is '分片ETag';
comment
on column cmx_file_part_detail.part_number is '分片号';
comment
on column cmx_file_part_detail.part_size is '分片大小，单位字节';
comment
on column cmx_file_part_detail.hash_info is '哈希信息';
comment
on column cmx_file_part_detail.archived is '是否归档：0-否，1-是';
comment
on column cmx_file_part_detail.create_time is '创建时间';
comment
on column cmx_file_part_detail.update_time is '更新时间';
comment
on column cmx_file_part_detail.create_by is '创建人ID';
comment
on column cmx_file_part_detail.create_name is '创建人姓名';
comment
on column cmx_file_part_detail.update_by is '更新人ID';
comment
on column cmx_file_part_detail.update_name is '更新人姓名';

CREATE INDEX IF NOT EXISTS idx_file_part_detail_upload_id
    on cmx_file_part_detail (upload_id);

        