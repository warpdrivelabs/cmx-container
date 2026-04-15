-- 插件生命周期管理与版本控制系统 - 数据库表结构
-- 整理要求：
-- 1. 主键都改为 varchar(64)
-- 2. 表中每个字段都添加 COMMENT ON COLUMN 注释
-- 3. 每个表都有 COMMENT ON TABLE 注释
-- 4. 移除所有的 CONSTRAINT 约束
-- 4. 存储json的字段使用TEXT类型，字段注释标识下是json文本


-- =============================================
-- 3.3.1 插件注册表 (cmx_plugin)
-- 存储所有已安装插件的核心信息（位于默认数据库）
-- =============================================
CREATE TABLE cmx_plugin (
    id                  VARCHAR(64) NOT NULL primary key ,
    plugin_id           VARCHAR(255) NOT NULL,
    name                VARCHAR(500) NOT NULL,
    version             VARCHAR(50) NOT NULL,
    wasm_path           TEXT NOT NULL,
    install_path        TEXT NOT NULL,
    db_id               VARCHAR(100) ,
    status              VARCHAR(30)  DEFAULT 'installed',
    is_system           BOOLEAN  DEFAULT FALSE,
    is_locked           BOOLEAN  DEFAULT FALSE,
    domain_code         VARCHAR(64),
    application_code    VARCHAR(64),
    module_code         VARCHAR(64),
    vendor_name         VARCHAR(255),
    vendor_url          TEXT,
    vendor_contact      VARCHAR(255),
    metadata            Jsonb,
    signature_algorithm VARCHAR(50),
    signer_key_id       VARCHAR(255),
    zip_source_url      VARCHAR(500),
    zip_source_type     VARCHAR(30),
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4 NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100),
    plugin_type         VARCHAR(50),
    source_path         varchar(500),
    description         TEXT
);
COMMENT ON TABLE cmx_plugin IS '插件注册主表：存储所有已安装插件的核心信息基线版本';

COMMENT ON COLUMN cmx_plugin.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin.plugin_id IS '插件唯一标识 (如 "example_plugin")';
COMMENT ON COLUMN cmx_plugin.name IS '显示名称';
COMMENT ON COLUMN cmx_plugin.version IS '当前版本 (语义版本)';
COMMENT ON COLUMN cmx_plugin.wasm_path IS 'WASM 文件绝对路径';
COMMENT ON COLUMN cmx_plugin.install_path IS '安装根目录路径';
COMMENT ON COLUMN cmx_plugin.db_id IS '插件业务数据存储的数据库ID';
COMMENT ON COLUMN cmx_plugin.status IS '状态: installed(已安装), active(已激活), inactive(已停用), failed(失败)';
COMMENT ON COLUMN cmx_plugin.is_system IS '是否系统默认插件';
COMMENT ON COLUMN cmx_plugin.is_locked IS '是否被锁定 (防止卸载)';
COMMENT ON COLUMN cmx_plugin.domain_code IS '所属域编码 (如 "FIN")';
COMMENT ON COLUMN cmx_plugin.application_code IS '所属应用编码 (如 "GL_ACCT")';
COMMENT ON COLUMN cmx_plugin.module_code IS '所属模块编码 (如 "GL")';
COMMENT ON COLUMN cmx_plugin.vendor_name IS '开发商名称';
COMMENT ON COLUMN cmx_plugin.vendor_url IS '开发商URL';
COMMENT ON COLUMN cmx_plugin.vendor_contact IS '开发商联系方式';
COMMENT ON COLUMN cmx_plugin.metadata IS '扩展元数据';
COMMENT ON COLUMN cmx_plugin.signature_algorithm IS '签名算法';
COMMENT ON COLUMN cmx_plugin.signer_key_id IS '签名密钥ID';
COMMENT ON COLUMN cmx_plugin.zip_source_url IS '插件ZIP包来源地址';
COMMENT ON COLUMN cmx_plugin.zip_source_type IS '插件来源类型: local(本地), url(远程URL), registry(注册表)';
COMMENT ON COLUMN cmx_plugin.create_time IS '创建时间';
COMMENT ON COLUMN cmx_plugin.update_time IS '更新时间';
COMMENT ON COLUMN cmx_plugin.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_plugin.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_plugin.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_plugin.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_plugin.update_name IS '更新人名称';
COMMENT ON COLUMN cmx_plugin.plugin_type IS '插件类型: wasm/rhai';
COMMENT ON COLUMN cmx_plugin.source_path IS '源码路径';
COMMENT ON COLUMN cmx_plugin.description IS '插件描述信息';

CREATE INDEX idx_plugin_domain_app_module ON cmx_plugin(domain_code, application_code, module_code);
-- CREATE UNIQUE INDEX uk_cmx_plugin_plugin_id ON cmx_plugin(plugin_id);
-- cmx_plugin 表添加唯一约束
ALTER TABLE cmx_plugin ADD CONSTRAINT uk_cmx_plugin_plugin_id UNIQUE (plugin_id);

-- =============================================
-- 3.3.2 版本历史表 (cmx_plugin_versions)
-- 记录插件的版本历史
-- =============================================
CREATE TABLE cmx_plugin_versions (
    id                  VARCHAR(64) NOT NULL primary key,
    plugin_id           VARCHAR(64) NOT NULL,
    version             VARCHAR(50) NOT NULL,
    install_path        TEXT NOT NULL,
    wasm_path           TEXT NOT NULL,
    is_current          BOOLEAN NOT NULL DEFAULT FALSE,
    installed_at        TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    uninstalled_at      TIMESTAMP WITH TIME ZONE,
    zip_source_url      VARCHAR(500),
    zip_source_type     VARCHAR(30),
    build_type     VARCHAR(30),
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4 NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100),
    plugin_type         VARCHAR(50),
    source_path         varchar(500)


);

COMMENT ON TABLE cmx_plugin_versions IS '插件版本历史表：记录插件的版本历史';
COMMENT ON COLUMN cmx_plugin_versions.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_versions.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_versions.version IS '版本号';
COMMENT ON COLUMN cmx_plugin_versions.install_path IS '该版本的安装路径';
COMMENT ON COLUMN cmx_plugin_versions.wasm_path IS '该版本的 WASM 路径';
COMMENT ON COLUMN cmx_plugin_versions.is_current IS '是否当前版本';
COMMENT ON COLUMN cmx_plugin_versions.installed_at IS '安装时间';
COMMENT ON COLUMN cmx_plugin_versions.uninstalled_at IS '卸载时间';
COMMENT ON COLUMN cmx_plugin_versions.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_plugin_versions.zip_source_url IS '该版本插件ZIP包来源地址';
COMMENT ON COLUMN cmx_plugin_versions.zip_source_type IS '该版本插件来源类型: local(本地), url(远程URL), registry(注册表)';
COMMENT ON COLUMN cmx_plugin_versions.build_type IS '构建类型: debug/release';
COMMENT ON COLUMN cmx_plugin_versions.create_time IS '创建时间';
COMMENT ON COLUMN cmx_plugin_versions.update_time IS '更新时间';
COMMENT ON COLUMN cmx_plugin_versions.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_plugin_versions.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_plugin_versions.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_plugin_versions.update_name IS '更新人名称';
COMMENT ON COLUMN cmx_plugin_versions.plugin_type IS '插件类型: wasm/rhai';
COMMENT ON COLUMN cmx_plugin_versions.source_path IS '源码路径';

CREATE INDEX idx_version_plugin ON cmx_plugin_versions(plugin_id);
CREATE INDEX idx_version_current ON cmx_plugin_versions(plugin_id, is_current) WHERE is_current = TRUE;
-- CREATE UNIQUE INDEX uk_cmx_plugin_versions_plugin_version ON cmx_plugin_versions(plugin_id,version);
-- cmx_plugin_versions 表添加复合唯一约束
ALTER TABLE cmx_plugin_versions ADD CONSTRAINT uk_cmx_plugin_versions_plugin_version UNIQUE (plugin_id, version);

-- =============================================
-- 3.3.3 依赖关系表 (cmx_plugin_dependencies)
-- 记录插件之间的依赖关系
-- =============================================
CREATE TABLE cmx_plugin_dependencies (
    id                  VARCHAR(64) NOT NULL primary key,
    plugin_id           VARCHAR(64) NOT NULL,
    dependency_plugin_id VARCHAR(255) NOT NULL,
    dependency_name     VARCHAR(500),
    version_constraint  VARCHAR(100),
    min_version         VARCHAR(50),
    max_version         VARCHAR(50),
    is_optional         BOOLEAN NOT NULL DEFAULT FALSE,
    is_dev              BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_version    VARCHAR(50),
    resolution_status   VARCHAR(30),
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4 NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100)
);
COMMENT ON TABLE cmx_plugin_dependencies IS '插件依赖关系表：记录插件之间的依赖关系';
COMMENT ON COLUMN cmx_plugin_dependencies.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_dependencies.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_dependencies.dependency_plugin_id IS '依赖的插件ID (可能是未安装的)';
COMMENT ON COLUMN cmx_plugin_dependencies.dependency_name IS '依赖的插件名称';
COMMENT ON COLUMN cmx_plugin_dependencies.version_constraint IS '版本约束 (如 "^1.0.0", "~2.1.0", ">=1.0.0 <3.0.0")';
COMMENT ON COLUMN cmx_plugin_dependencies.min_version IS '最小版本';
COMMENT ON COLUMN cmx_plugin_dependencies.max_version IS '最大版本';
COMMENT ON COLUMN cmx_plugin_dependencies.is_optional IS '是否可选依赖';
COMMENT ON COLUMN cmx_plugin_dependencies.is_dev IS '是否开发依赖';
COMMENT ON COLUMN cmx_plugin_dependencies.resolved_version IS '已解析的版本';
COMMENT ON COLUMN cmx_plugin_dependencies.resolution_status IS '解析状态: resolved, conflict, missing';
COMMENT ON COLUMN cmx_plugin_dependencies.create_time IS '创建时间';
COMMENT ON COLUMN cmx_plugin_dependencies.update_time IS '更新时间';
COMMENT ON COLUMN cmx_plugin_dependencies.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_plugin_dependencies.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_plugin_dependencies.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_plugin_dependencies.update_name IS '更新人名称';

CREATE INDEX idx_dep_plugin ON cmx_plugin_dependencies(plugin_id);
CREATE INDEX idx_dep_resolved ON cmx_plugin_dependencies(plugin_id, resolution_status);


-- =============================================
-- 3.3.4 节点部署记录表 (cmx_plugin_deployments)
-- 记录在各个节点上的部署状态
-- =============================================
CREATE TABLE cmx_plugin_deployments (
    id                  VARCHAR(64) NOT NULL primary key,
    plugin_id           VARCHAR(64) NOT NULL,
    node_id             VARCHAR(100) NOT NULL,
    node_type           VARCHAR(50),
    version             VARCHAR(50) NOT NULL,
    status              VARCHAR(30) ,
    progress            INTEGER DEFAULT 0,
    error_message       TEXT,
    error_details       TEXT,
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4 NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100),
    plugin_type         VARCHAR(50),
    source_path         TEXT
);
COMMENT ON TABLE cmx_plugin_deployments IS '节点插件部署记录表：记录插件在各个节点上的部署状态';
COMMENT ON COLUMN cmx_plugin_deployments.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_deployments.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_deployments.node_id IS '节点标识';
COMMENT ON COLUMN cmx_plugin_deployments.node_type IS '节点类型: primary, replica, worker';
COMMENT ON COLUMN cmx_plugin_deployments.version IS '部署的版本';
COMMENT ON COLUMN cmx_plugin_deployments.status IS '部署状态';
COMMENT ON COLUMN cmx_plugin_deployments.progress IS '进度 (0-100)';
COMMENT ON COLUMN cmx_plugin_deployments.error_message IS '错误信息';
COMMENT ON COLUMN cmx_plugin_deployments.error_details IS '错误详情';
COMMENT ON COLUMN cmx_plugin_deployments.create_time IS '创建时间';
COMMENT ON COLUMN cmx_plugin_deployments.update_time IS '更新时间';
COMMENT ON COLUMN cmx_plugin_deployments.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_plugin_deployments.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_plugin_deployments.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_plugin_deployments.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_plugin_deployments.update_name IS '更新人名称';
COMMENT ON COLUMN cmx_plugin_deployments.plugin_type IS '插件类型: wasm/rhai';
COMMENT ON COLUMN cmx_plugin_deployments.source_path IS '源码路径';

CREATE INDEX idx_deploy_plugin ON cmx_plugin_deployments(plugin_id);
CREATE INDEX idx_deploy_node ON cmx_plugin_deployments(node_id);
CREATE INDEX idx_deploy_status ON cmx_plugin_deployments(status);
-- CREATE UNIQUE INDEX uk_cmx_plugin_deployments_plugin_node_version ON cmx_plugin_deployments(plugin_id,node_id,version);
-- cmx_plugin_deployments 表添加复合唯一约束
ALTER TABLE cmx_plugin_deployments ADD CONSTRAINT uk_cmx_plugin_deployments_plugin_node_version UNIQUE (plugin_id, node_id, version);


-- =============================================
-- 3.3.5 审计日志表 (cmx_plugin_audit_log)
-- 记录所有插件生命周期操作
-- =============================================
CREATE TABLE cmx_plugin_audit_log (
    id                  VARCHAR(64) NOT NULL,
    plugin_id           VARCHAR(64),
    node_id             VARCHAR(64),
    version          VARCHAR(64),
    deployment_id       VARCHAR(64),
    operation_type      VARCHAR(50) NOT NULL,
    operation_status    VARCHAR(30) NOT NULL,
    request_id          VARCHAR(100),
    details             TEXT,
    old_value           TEXT,
    new_value           TEXT,
    error_code          VARCHAR(50),
    error_message       TEXT,
    stack_trace         TEXT,
    started_at          TIMESTAMP WITH TIME ZONE,
    completed_at        TIMESTAMP WITH TIME ZONE,
    duration_ms         BIGINT,
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4 NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100)
);
COMMENT ON TABLE cmx_plugin_audit_log IS '审计日志表：记录插件操作日志';
COMMENT ON COLUMN cmx_plugin_audit_log.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_audit_log.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_audit_log.node_id IS '节点ID';
COMMENT ON COLUMN cmx_plugin_audit_log.version IS '插件版本';
COMMENT ON COLUMN cmx_plugin_audit_log.deployment_id IS '关联部署ID';
COMMENT ON COLUMN cmx_plugin_audit_log.operation_type IS '操作类型';
COMMENT ON COLUMN cmx_plugin_audit_log.operation_status IS '操作状态';

COMMENT ON COLUMN cmx_plugin_audit_log.request_id IS '请求 ID (用于链路追踪)';
COMMENT ON COLUMN cmx_plugin_audit_log.details IS '操作详情 (JSON)';
COMMENT ON COLUMN cmx_plugin_audit_log.old_value IS '旧值';
COMMENT ON COLUMN cmx_plugin_audit_log.new_value IS '新值';
COMMENT ON COLUMN cmx_plugin_audit_log.error_code IS '错误代码';
COMMENT ON COLUMN cmx_plugin_audit_log.error_message IS '错误消息';
COMMENT ON COLUMN cmx_plugin_audit_log.stack_trace IS '堆栈跟踪';
COMMENT ON COLUMN cmx_plugin_audit_log.started_at IS '操作开始时间';
COMMENT ON COLUMN cmx_plugin_audit_log.completed_at IS '操作完成时间';
COMMENT ON COLUMN cmx_plugin_audit_log.duration_ms IS '操作耗时 (毫秒)';
COMMENT ON COLUMN cmx_plugin_audit_log.create_time IS '创建时间';
COMMENT ON COLUMN cmx_plugin_audit_log.update_time IS '更新时间';
COMMENT ON COLUMN cmx_plugin_audit_log.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_plugin_audit_log.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_plugin_audit_log.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_plugin_audit_log.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_plugin_audit_log.update_name IS '更新人名称';

CREATE INDEX idx_audit_plugin ON cmx_plugin_audit_log(plugin_id);
CREATE INDEX idx_audit_node ON cmx_plugin_audit_log(node_id);
CREATE INDEX idx_audit_operation ON cmx_plugin_audit_log(operation_type);
CREATE INDEX idx_audit_timestamp ON cmx_plugin_audit_log(started_at);
CREATE INDEX idx_audit_request ON cmx_plugin_audit_log(request_id);


-- =============================================
-- 3.3.6 回滚记录表 (cmx_plugin_rollback)
-- 记录回滚点信息
-- =============================================

-- CREATE TABLE cmx_plugin_rollback (
--     id                  VARCHAR(64) NOT NULL,
--     plugin_id           VARCHAR(64) NOT NULL,
--     operation_id        VARCHAR(100) NOT NULL,
--     from_version        VARCHAR(50) NOT NULL,
--     to_version          VARCHAR(50) NOT NULL,
--     backup_path         TEXT NOT NULL,
--     backup_size         BIGINT,
--     backup_create_time  TIMESTAMP WITH TIME ZONE NOT NULL,
--     status              VARCHAR(30) NOT NULL,
--     completed_at        TIMESTAMP WITH TIME ZONE,
--     reason              TEXT,
--     create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     archived            INT4 NOT NULL DEFAULT 0,
--     create_by           VARCHAR(100),
--     create_name         VARCHAR(100),
--     update_by           VARCHAR(100),
--     update_name         VARCHAR(100)
-- );

--COMMENT ON TABLE cmx_plugin_rollback IS '插件回滚记录表：记录回滚信息';
-- COMMENT ON COLUMN cmx_plugin_rollback.id IS '主键ID';
-- COMMENT ON COLUMN cmx_plugin_rollback.plugin_id IS '关联插件ID';
-- COMMENT ON COLUMN cmx_plugin_rollback.operation_id IS '原始操作 ID';
-- COMMENT ON COLUMN cmx_plugin_rollback.from_version IS '回滚前版本';
-- COMMENT ON COLUMN cmx_plugin_rollback.to_version IS '回滚后版本';
-- COMMENT ON COLUMN cmx_plugin_rollback.backup_path IS '备份路径';
-- COMMENT ON COLUMN cmx_plugin_rollback.backup_size IS '备份大小 (字节)';
-- COMMENT ON COLUMN cmx_plugin_rollback.backup_create_time IS '备份创建时间';
-- COMMENT ON COLUMN cmx_plugin_rollback.status IS '状态';
-- COMMENT ON COLUMN cmx_plugin_rollback.completed_at IS '完成时间';
-- COMMENT ON COLUMN cmx_plugin_rollback.reason IS '回滚原因';
-- COMMENT ON COLUMN cmx_plugin_rollback.create_time IS '创建时间';
-- COMMENT ON COLUMN cmx_plugin_rollback.update_time IS '更新时间';
-- COMMENT ON COLUMN cmx_plugin_rollback.archived IS '归档标志：0-未归档，1-已归档';
-- COMMENT ON COLUMN cmx_plugin_rollback.create_by IS '创建人ID';
-- COMMENT ON COLUMN cmx_plugin_rollback.create_name IS '创建人名称';
-- COMMENT ON COLUMN cmx_plugin_rollback.update_by IS '更新人ID';
-- COMMENT ON COLUMN cmx_plugin_rollback.update_name IS '更新人名称';
-- CREATE INDEX idx_rollback_plugin ON cmx_plugin_rollback(plugin_id);
-- CREATE INDEX idx_rollback_operation ON cmx_plugin_rollback(operation_id);


-- =============================================
-- 3.3.7 系统默认插件配置表 (cmx_system_plugins)
-- 配置系统启动时需要自动安装的插件
-- =============================================
CREATE TABLE cmx_system_plugins (
    id                  VARCHAR(64) NOT NULL,
    plugin_id           VARCHAR(255) NOT NULL,
    name                VARCHAR(500) NOT NULL,
    version             VARCHAR(50) NOT NULL,
    install_order       INTEGER NOT NULL DEFAULT 0,
    is_optional         BOOLEAN NOT NULL DEFAULT FALSE,
    is_critical         BOOLEAN NOT NULL DEFAULT FALSE,
    retry_count         INTEGER NOT NULL DEFAULT 3,
    retry_delay_seconds INTEGER NOT NULL DEFAULT 10,
    wait_for_plugins    VARCHAR(255),
    source_type         VARCHAR(30) NOT NULL,
    source_path         TEXT,
    source_url          TEXT,
    status              VARCHAR(30) NOT NULL DEFAULT 'pending',
    last_installed_at   TIMESTAMP WITH TIME ZONE,
    install_attempts    INTEGER NOT NULL DEFAULT 0,
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4 NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100)
);

COMMENT ON TABLE cmx_system_plugins IS '系统默认插件配置表：配置系统启动时需要自动安装的插件';
COMMENT ON COLUMN cmx_system_plugins.id IS '主键ID';
COMMENT ON COLUMN cmx_system_plugins.plugin_id IS '插件唯一标识';
COMMENT ON COLUMN cmx_system_plugins.name IS '插件名称';
COMMENT ON COLUMN cmx_system_plugins.version IS '插件版本';
COMMENT ON COLUMN cmx_system_plugins.install_order IS '安装顺序 (数字越小越先安装)';
COMMENT ON COLUMN cmx_system_plugins.is_optional IS '是否可选 (可选则安装失败不阻止启动)';
COMMENT ON COLUMN cmx_system_plugins.is_critical IS '是否关键 (关键插件失败导致系统无法启动)';
COMMENT ON COLUMN cmx_system_plugins.retry_count IS '重试次数';
COMMENT ON COLUMN cmx_system_plugins.retry_delay_seconds IS '重试间隔 (秒)';
COMMENT ON COLUMN cmx_system_plugins.wait_for_plugins IS '需要等待完成的插件列表';
COMMENT ON COLUMN cmx_system_plugins.source_type IS '来源类型: bundled, url';
COMMENT ON COLUMN cmx_system_plugins.source_path IS '来源路径 (bundled 时为内置路径)';
COMMENT ON COLUMN cmx_system_plugins.source_url IS '来源 URL (url 类型时使用)';
COMMENT ON COLUMN cmx_system_plugins.status IS '状态';
COMMENT ON COLUMN cmx_system_plugins.create_time IS '创建时间';
COMMENT ON COLUMN cmx_system_plugins.update_time IS '更新时间';
COMMENT ON COLUMN cmx_system_plugins.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_system_plugins.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_system_plugins.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_system_plugins.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_system_plugins.update_name IS '更新人名称';

CREATE INDEX idx_system_plugin_order ON cmx_system_plugins(install_order);
CREATE INDEX idx_system_plugin_status ON cmx_system_plugins(status);


-- =============================================
-- 3.3.8 节点信息表 (cmx_plugin_nodes)
-- 记录集群中的节点信息
-- =============================================
CREATE TABLE cmx_plugin_nodes (
    node_id             VARCHAR(64) NOT NULL,
    node_name           VARCHAR(255) NOT NULL,
    node_type           VARCHAR(30) NOT NULL,
    status              VARCHAR(30) NOT NULL,
    is_active           BOOLEAN NOT NULL DEFAULT TRUE,
    host                VARCHAR(255) NOT NULL,
    port                INTEGER NOT NULL,
    protocol            VARCHAR(10) NOT NULL DEFAULT 'http',
    capabilities        TEXT,
    last_health_check   TIMESTAMP WITH TIME ZONE,
    health_check_interval INTEGER NOT NULL DEFAULT 30,
    plugin_manager_version VARCHAR(50),
    registered_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_seen_at        TIMESTAMP WITH TIME ZONE,
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4 NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100)
);

COMMENT ON TABLE cmx_plugin_nodes IS '节点信息表：记录集群中的节点信息';
COMMENT ON COLUMN cmx_plugin_nodes.node_id IS '节点ID';
COMMENT ON COLUMN cmx_plugin_nodes.node_name IS '节点名称';
COMMENT ON COLUMN cmx_plugin_nodes.node_type IS '节点类型: primary, replica, worker';
COMMENT ON COLUMN cmx_plugin_nodes.status IS '节点状态: online, offline, maintenance';
COMMENT ON COLUMN cmx_plugin_nodes.is_active IS '是否激活';
COMMENT ON COLUMN cmx_plugin_nodes.host IS '主机地址';
COMMENT ON COLUMN cmx_plugin_nodes.port IS '端口';
COMMENT ON COLUMN cmx_plugin_nodes.protocol IS '协议';
COMMENT ON COLUMN cmx_plugin_nodes.capabilities IS '节点能力';
COMMENT ON COLUMN cmx_plugin_nodes.last_health_check IS '最后健康检查时间';
COMMENT ON COLUMN cmx_plugin_nodes.health_check_interval IS '健康检查间隔 (秒)';
COMMENT ON COLUMN cmx_plugin_nodes.plugin_manager_version IS '插件管理器版本';
COMMENT ON COLUMN cmx_plugin_nodes.registered_at IS '注册时间';
COMMENT ON COLUMN cmx_plugin_nodes.last_seen_at IS '最后可见时间';
COMMENT ON COLUMN cmx_plugin_nodes.create_time IS '创建时间';
COMMENT ON COLUMN cmx_plugin_nodes.update_time IS '更新时间';
COMMENT ON COLUMN cmx_plugin_nodes.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_plugin_nodes.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_plugin_nodes.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_plugin_nodes.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_plugin_nodes.update_name IS '更新人名称';
CREATE INDEX idx_node_status ON cmx_plugin_nodes(status);
CREATE INDEX idx_node_type ON cmx_plugin_nodes(node_type);


-- =============================================
-- 3.3.9 插件功能表 (cmx_plugin_features)
-- 记录插件暴露的功能和api
-- =============================================
CREATE TABLE cmx_plugin_features (
                                     id                  VARCHAR(64) NOT NULL,
                                     plugin_id           VARCHAR(64) NOT NULL,
                                     plugin_version      VARCHAR(50) NOT NULL,
                                     feature_id          VARCHAR(255) NOT NULL,
                                     feature_name        VARCHAR(500) NOT NULL,
                                     feature_type        VARCHAR(50) NOT NULL,
                                     description         TEXT,
                                     config              JSONB,
                                     status              VARCHAR(30) NOT NULL DEFAULT 'active',
                                     create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                                     update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                                     archived            INT4 NOT NULL DEFAULT 0,
                                     create_by           VARCHAR(100),
                                     create_name         VARCHAR(100),
                                     update_by           VARCHAR(100),
                                     update_name         VARCHAR(100)
);
COMMENT ON TABLE cmx_plugin_features IS '插件功能表';
COMMENT ON COLUMN cmx_plugin_features.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_features.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_features.plugin_version IS '插件版本';
COMMENT ON COLUMN cmx_plugin_features.feature_id IS '功能唯一标识';
COMMENT ON COLUMN cmx_plugin_features.feature_name IS '功能名称';
COMMENT ON COLUMN cmx_plugin_features.feature_type IS '功能类型: service, event_handler, scheduler, api,function';
COMMENT ON COLUMN cmx_plugin_features.description IS '功能描述';
COMMENT ON COLUMN cmx_plugin_features.config IS '功能配置';
COMMENT ON COLUMN cmx_plugin_features.status IS '状态: active, inactive, error';
COMMENT ON COLUMN cmx_plugin_features.create_time IS '创建时间';
COMMENT ON COLUMN cmx_plugin_features.update_time IS '更新时间';
COMMENT ON COLUMN cmx_plugin_features.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_plugin_features.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_plugin_features.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_plugin_features.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_plugin_features.update_name IS '更新人名称';

-- -- =============================================
-- -- 3.3.9 插件事件表 (cmx_plugin_events)
-- -- =============================================
-- CREATE TABLE cmx_plugin_events (
--                                    id                  VARCHAR(64) NOT NULL,
--                                    plugin_id           VARCHAR(64) NOT NULL,
--                                    event_type          VARCHAR(100) NOT NULL,
--                                    event_data          JSONB,
--                                    processed           BOOLEAN NOT NULL DEFAULT FALSE,
--                                    processed_at        TIMESTAMP WITH TIME ZONE,
--                                    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--                                    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--                                    archived            INT4 NOT NULL DEFAULT 0,
--                                    create_by           VARCHAR(100),
--                                    create_name         VARCHAR(100),
--                                    update_by           VARCHAR(100),
--                                    update_name         VARCHAR(100)
-- );
-- COMMENT ON TABLE cmx_plugin_events IS '插件事件表';
-- COMMENT ON COLUMN cmx_plugin_events.id IS '主键ID';
-- COMMENT ON COLUMN cmx_plugin_events.plugin_id IS '关联插件ID';
-- COMMENT ON COLUMN cmx_plugin_events.event_type IS '事件类型';
-- COMMENT ON COLUMN cmx_plugin_events.event_data IS '事件数据';
-- COMMENT ON COLUMN cmx_plugin_events.processed IS '是否已处理';
-- COMMENT ON COLUMN cmx_plugin_events.processed_at IS '处理时间';




-- =============================================
-- 表定义元数据存储表 (cmx_meta_table_define)
-- =============================================
DROP TABLE IF EXISTS cmx_meta_table_define;
CREATE TABLE cmx_meta_table_define(
                                      id VARCHAR(64) NOT NULL,
                                      table_name VARCHAR(100),
                                      display_name VARCHAR(100),
                                      db_id VARCHAR(100),
                                      plugin_id VARCHAR(64),
                                      version VARCHAR(50),
                                      domain_code VARCHAR(100),
                                      application_code VARCHAR(100),
                                      module_code VARCHAR(100),
                                      archived int4 DEFAULT  0,
                                      create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                      update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                      create_by varchar(100),
                                      create_name varchar(100),
                                      update_by varchar(100),
                                      update_name varchar(100),
                                      PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_meta_table_define IS '表定义元数据';
COMMENT ON COLUMN cmx_meta_table_define.id IS '主键';
COMMENT ON COLUMN cmx_meta_table_define.table_name IS '表名';
COMMENT ON COLUMN cmx_meta_table_define.display_name IS '显示名称';
COMMENT ON COLUMN cmx_meta_table_define.db_id IS '所属数据库id';
COMMENT ON COLUMN cmx_meta_table_define.plugin_id IS '插件id';
COMMENT ON COLUMN cmx_meta_table_define.version IS '当前使用的元数据插件版本';
COMMENT ON COLUMN cmx_meta_table_define.domain_code IS '域编码';
COMMENT ON COLUMN cmx_meta_table_define.application_code IS '应用编码';
COMMENT ON COLUMN cmx_meta_table_define.module_code IS '模块编码';
COMMENT ON COLUMN cmx_meta_table_define.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_meta_table_define.create_time IS '创建时间';
COMMENT ON COLUMN cmx_meta_table_define.update_time IS '更新时间';
COMMENT ON COLUMN cmx_meta_table_define.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_meta_table_define.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_meta_table_define.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_meta_table_define.update_name IS '更新人名称';

-- =============================================
-- 表定义元数据版本表 (cmx_meta_table_define_version)
-- =============================================
DROP TABLE IF EXISTS cmx_meta_table_define_version;
CREATE TABLE cmx_meta_table_define_version(
                                              id VARCHAR(64) NOT NULL,
                                              table_name VARCHAR(100),
                                              display_name VARCHAR(100),
                                              db_id VARCHAR(100),
                                              plugin_id VARCHAR(64),
                                              version VARCHAR(50),
                                              domain_code VARCHAR(100),
                                              application_code VARCHAR(100),
                                              module_code VARCHAR(100),
                                              metadata jsonb,
                                              archived int4 DEFAULT  0,
                                              create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                              update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                              create_by varchar(100),
                                              create_name varchar(100),
                                              update_by varchar(100),
                                              update_name varchar(100),
                                              PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_meta_table_define_version IS '表元数据版本表';
COMMENT ON COLUMN cmx_meta_table_define_version.id IS '主键';
COMMENT ON COLUMN cmx_meta_table_define_version.table_name IS '表名';
COMMENT ON COLUMN cmx_meta_table_define_version.display_name IS '显示名称';
COMMENT ON COLUMN cmx_meta_table_define_version.db_id IS '所属数据库id';
COMMENT ON COLUMN cmx_meta_table_define_version.plugin_id IS '插件id';
COMMENT ON COLUMN cmx_meta_table_define_version.version IS '插件版本';
COMMENT ON COLUMN cmx_meta_table_define_version.domain_code IS '域编码';
COMMENT ON COLUMN cmx_meta_table_define_version.application_code IS '应用编码';
COMMENT ON COLUMN cmx_meta_table_define_version.module_code IS '模块编码';
COMMENT ON COLUMN cmx_meta_table_define_version.metadata IS '元数据json';
COMMENT ON COLUMN cmx_meta_table_define_version.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_meta_table_define_version.create_time IS '创建时间';
COMMENT ON COLUMN cmx_meta_table_define_version.update_time IS '更新时间';
COMMENT ON COLUMN cmx_meta_table_define_version.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_meta_table_define_version.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_meta_table_define_version.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_meta_table_define_version.update_name IS '更新人名称';





