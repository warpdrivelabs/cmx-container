-- ============================================
-- 插件市场 - 插件主表
-- ============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_plugin (
    id VARCHAR(64) NOT NULL,
    plugin_id VARCHAR(128) NOT NULL,
    name VARCHAR(256),
    description TEXT,
    short_description VARCHAR(512),
    icon_url VARCHAR(512),
    category VARCHAR(64),
    tags JSONB,
    vendor_name VARCHAR(128),
    vendor_url VARCHAR(512),
    vendor_contact VARCHAR(256),
    license_type VARCHAR(32),
    homepage_url VARCHAR(512),
    documentation_url VARCHAR(512),
    repository_url VARCHAR(512),
    status VARCHAR(32),
    is_featured INT2,
    is_official INT2,
    avg_rating DECIMAL(3,2),
    rating_count INT4,
    download_count INT8,
    install_count INT8,
    domain_code VARCHAR(64),
    application_code VARCHAR(64),
    module_code VARCHAR(64),
    plugin_type VARCHAR(32),
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),
    CONSTRAINT pk_marketplace_plugin PRIMARY KEY (id),
    CONSTRAINT uk_marketplace_plugin_id UNIQUE (plugin_id)
);

COMMENT ON TABLE cmx_marketplace_plugin IS '插件市场-插件主表';
COMMENT ON COLUMN cmx_marketplace_plugin.id IS '主键';
COMMENT ON COLUMN cmx_marketplace_plugin.plugin_id IS '插件唯一标识';
COMMENT ON COLUMN cmx_marketplace_plugin.name IS '插件名称';
COMMENT ON COLUMN cmx_marketplace_plugin.description IS '插件详细描述';
COMMENT ON COLUMN cmx_marketplace_plugin.short_description IS '简短描述';
COMMENT ON COLUMN cmx_marketplace_plugin.icon_url IS '图标URL';
COMMENT ON COLUMN cmx_marketplace_plugin.category IS '分类（如：数据集成、业务逻辑、工具类）';
COMMENT ON COLUMN cmx_marketplace_plugin.tags IS '标签列表（JSON数组）';
COMMENT ON COLUMN cmx_marketplace_plugin.vendor_name IS '供应商名称';
COMMENT ON COLUMN cmx_marketplace_plugin.vendor_url IS '供应商主页';
COMMENT ON COLUMN cmx_marketplace_plugin.vendor_contact IS '联系方式';
COMMENT ON COLUMN cmx_marketplace_plugin.license_type IS '许可证类型（MIT/Apache/Commercial/Free）';
COMMENT ON COLUMN cmx_marketplace_plugin.homepage_url IS '插件主页';
COMMENT ON COLUMN cmx_marketplace_plugin.documentation_url IS '文档地址';
COMMENT ON COLUMN cmx_marketplace_plugin.repository_url IS '代码仓库地址';
COMMENT ON COLUMN cmx_marketplace_plugin.status IS '状态（draft/published/deprecated/archived）';
COMMENT ON COLUMN cmx_marketplace_plugin.is_featured IS '是否推荐（1是/0否）';
COMMENT ON COLUMN cmx_marketplace_plugin.is_official IS '是否官方插件（1是/0否）';
COMMENT ON COLUMN cmx_marketplace_plugin.avg_rating IS '平均评分（1.00-5.00）';
COMMENT ON COLUMN cmx_marketplace_plugin.rating_count IS '评分数量';
COMMENT ON COLUMN cmx_marketplace_plugin.download_count IS '总下载量';
COMMENT ON COLUMN cmx_marketplace_plugin.install_count IS '总安装量';
COMMENT ON COLUMN cmx_marketplace_plugin.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_marketplace_plugin.application_code IS '所属应用编码';
COMMENT ON COLUMN cmx_marketplace_plugin.module_code IS '所属模块编码';
COMMENT ON COLUMN cmx_marketplace_plugin.plugin_type IS '插件类型';
COMMENT ON COLUMN cmx_marketplace_plugin.archived IS '归档标记（0未归档/1已归档）';
COMMENT ON COLUMN cmx_marketplace_plugin.create_time IS '创建时间';
COMMENT ON COLUMN cmx_marketplace_plugin.update_time IS '更新时间';
COMMENT ON COLUMN cmx_marketplace_plugin.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_marketplace_plugin.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_marketplace_plugin.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_marketplace_plugin.update_name IS '更新人姓名';

-- ============================================
-- 插件市场 - 版本表
-- ============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_plugin_version (
    id VARCHAR(64) NOT NULL,
    plugin_id VARCHAR(128) NOT NULL,
    version VARCHAR(64) NOT NULL,
    version_rank INT4,
    changelog TEXT,
    release_notes TEXT,
    download_url VARCHAR(512),
    package_size INT8,
    checksum VARCHAR(128),
    min_platform_version VARCHAR(32),
    max_platform_version VARCHAR(32),
    dependencies JSONB,
    compatibility JSONB,
    status VARCHAR(32),
    is_latest INT2,
    is_stable INT2,
    download_count INT8,
    published_at TIMESTAMP,
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),
    CONSTRAINT pk_marketplace_plugin_version PRIMARY KEY (id),
    CONSTRAINT uk_marketplace_plugin_version UNIQUE (plugin_id, version),
    CONSTRAINT fk_mpversion_plugin FOREIGN KEY (plugin_id)
        REFERENCES cmx_marketplace_plugin(plugin_id) ON DELETE CASCADE
);

COMMENT ON TABLE cmx_marketplace_plugin_version IS '插件市场-版本表';
COMMENT ON COLUMN cmx_marketplace_plugin_version.id IS '主键';
COMMENT ON COLUMN cmx_marketplace_plugin_version.plugin_id IS '插件ID';
COMMENT ON COLUMN cmx_marketplace_plugin_version.version IS '版本号（语义化版本）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.version_rank IS '版本排序值（用于版本比较）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.changelog IS '变更日志';
COMMENT ON COLUMN cmx_marketplace_plugin_version.release_notes IS '发布说明';
COMMENT ON COLUMN cmx_marketplace_plugin_version.download_url IS '下载地址';
COMMENT ON COLUMN cmx_marketplace_plugin_version.package_size IS '包大小（字节）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.checksum IS '校验和（SHA256）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.min_platform_version IS '最低平台版本要求';
COMMENT ON COLUMN cmx_marketplace_plugin_version.max_platform_version IS '最高平台版本要求';
COMMENT ON COLUMN cmx_marketplace_plugin_version.dependencies IS '依赖列表（JSON数组）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.compatibility IS '兼容性信息（JSON对象）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.status IS '状态（draft/published/deprecated）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.is_latest IS '是否最新版本（1是/0否）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.is_stable IS '是否稳定版（1是/0否）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.download_count IS '版本下载量';
COMMENT ON COLUMN cmx_marketplace_plugin_version.published_at IS '发布时间';
COMMENT ON COLUMN cmx_marketplace_plugin_version.archived IS '归档标记（0未归档/1已归档）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.create_time IS '创建时间';
COMMENT ON COLUMN cmx_marketplace_plugin_version.update_time IS '更新时间';
COMMENT ON COLUMN cmx_marketplace_plugin_version.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_marketplace_plugin_version.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_marketplace_plugin_version.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_marketplace_plugin_version.update_name IS '更新人姓名';

-- ============================================
-- 插件市场 - 下载统计表
-- ============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_download_stats (
    id VARCHAR(64) NOT NULL,
    plugin_id VARCHAR(128) NOT NULL,
    version VARCHAR(64),
    download_date DATE,
    download_count INT4,
    install_count INT4,
    source_type VARCHAR(32),
    region VARCHAR(32),
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),
    CONSTRAINT pk_marketplace_download_stats PRIMARY KEY (id),
    CONSTRAINT uk_download_stats UNIQUE (plugin_id, version, download_date, source_type),
    CONSTRAINT fk_dstats_plugin FOREIGN KEY (plugin_id)
        REFERENCES cmx_marketplace_plugin(plugin_id) ON DELETE CASCADE
);

COMMENT ON TABLE cmx_marketplace_download_stats IS '插件市场-下载统计表';
COMMENT ON COLUMN cmx_marketplace_download_stats.id IS '主键';
COMMENT ON COLUMN cmx_marketplace_download_stats.plugin_id IS '插件ID';
COMMENT ON COLUMN cmx_marketplace_download_stats.version IS '版本号';
COMMENT ON COLUMN cmx_marketplace_download_stats.download_date IS '下载日期';
COMMENT ON COLUMN cmx_marketplace_download_stats.download_count IS '当日下载量';
COMMENT ON COLUMN cmx_marketplace_download_stats.install_count IS '当日安装量';
COMMENT ON COLUMN cmx_marketplace_download_stats.source_type IS '来源类型（api/cli/marketplace）';
COMMENT ON COLUMN cmx_marketplace_download_stats.region IS '地区';
COMMENT ON COLUMN cmx_marketplace_download_stats.archived IS '归档标记（0未归档/1已归档）';
COMMENT ON COLUMN cmx_marketplace_download_stats.create_time IS '创建时间';
COMMENT ON COLUMN cmx_marketplace_download_stats.update_time IS '更新时间';
COMMENT ON COLUMN cmx_marketplace_download_stats.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_marketplace_download_stats.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_marketplace_download_stats.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_marketplace_download_stats.update_name IS '更新人姓名';

-- ============================================
-- 插件市场 - 评分表
-- ============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_rating (
    id VARCHAR(64) NOT NULL,
    plugin_id VARCHAR(128) NOT NULL,
    user_id VARCHAR(128) NOT NULL,
    rating INT4,
    review TEXT,
    status VARCHAR(32),
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),
    CONSTRAINT pk_marketplace_rating PRIMARY KEY (id),
    CONSTRAINT uk_rating_plugin_user UNIQUE (plugin_id, user_id),
    CONSTRAINT fk_rating_plugin FOREIGN KEY (plugin_id)
        REFERENCES cmx_marketplace_plugin(plugin_id) ON DELETE CASCADE
);

COMMENT ON TABLE cmx_marketplace_rating IS '插件市场-评分表';
COMMENT ON COLUMN cmx_marketplace_rating.id IS '主键';
COMMENT ON COLUMN cmx_marketplace_rating.plugin_id IS '插件ID';
COMMENT ON COLUMN cmx_marketplace_rating.user_id IS '用户ID';
COMMENT ON COLUMN cmx_marketplace_rating.rating IS '评分（1-5）';
COMMENT ON COLUMN cmx_marketplace_rating.review IS '评论内容';
COMMENT ON COLUMN cmx_marketplace_rating.status IS '状态（pending/approved/rejected）';
COMMENT ON COLUMN cmx_marketplace_rating.archived IS '归档标记（0未归档/1已归档）';
COMMENT ON COLUMN cmx_marketplace_rating.create_time IS '创建时间';
COMMENT ON COLUMN cmx_marketplace_rating.update_time IS '更新时间';
COMMENT ON COLUMN cmx_marketplace_rating.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_marketplace_rating.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_marketplace_rating.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_marketplace_rating.update_name IS '更新人姓名';

-- ============================================
-- 索引
-- ============================================
CREATE INDEX idx_mp_category ON cmx_marketplace_plugin(category);
CREATE INDEX idx_mp_status ON cmx_marketplace_plugin(status);
CREATE INDEX idx_mp_featured ON cmx_marketplace_plugin(is_featured) WHERE is_featured = 1;
CREATE INDEX idx_mp_download_count ON cmx_marketplace_plugin(download_count DESC);
CREATE INDEX idx_mp_rating ON cmx_marketplace_plugin(avg_rating DESC);
CREATE INDEX idx_mpv_plugin_id ON cmx_marketplace_plugin_version(plugin_id);
CREATE INDEX idx_mpv_latest ON cmx_marketplace_plugin_version(plugin_id, is_latest) WHERE is_latest = 1;
CREATE INDEX idx_dstats_date ON cmx_marketplace_download_stats(download_date);
CREATE INDEX idx_rating_plugin ON cmx_marketplace_rating(plugin_id);
