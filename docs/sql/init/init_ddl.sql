-- =============================================
-- cmx-container 数据库定义 (DDL)
-- 包含：域/应用/模块/数据源表、插件表、服务表
-- =============================================

-- =============================================
-- 1. 域表 (cmx_domain)
-- =============================================
DROP TABLE IF EXISTS cmx_domain;
CREATE TABLE cmx_domain
(
    id          VARCHAR(64)  NOT NULL,
    code        VARCHAR(64)  NOT NULL,
    name        VARCHAR(200) NOT NULL,
    description TEXT,
    type        VARCHAR(50),
    tags        TEXT,
    sort_order  INT4      DEFAULT 0,
    status      INT4      DEFAULT 1,
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by   VARCHAR(100),
    create_name VARCHAR(100),
    update_by   VARCHAR(100),
    update_name VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_domain IS '域表';
COMMENT
ON COLUMN cmx_domain.id IS 'ID';
COMMENT
ON COLUMN cmx_domain.code IS '域编码，全局唯一，如: FIN, HR, SCM';
COMMENT
ON COLUMN cmx_domain.name IS '域名称，如: 财务域, 人力资源域';
COMMENT
ON COLUMN cmx_domain.description IS '域描述';
COMMENT
ON COLUMN cmx_domain.type IS '类型: business(业务域), technical(技术域), product_line(产品线)';
COMMENT
ON COLUMN cmx_domain.tags IS '多标签，JSON数组字符串，如 ["财务","核心","S4HANA"]';
COMMENT
ON COLUMN cmx_domain.sort_order IS '排序字段，数值小的靠前';
COMMENT
ON COLUMN cmx_domain.status IS '状态：0-禁用，1-启用';
COMMENT
ON COLUMN cmx_domain.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_domain.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_domain.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_domain.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_domain.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_domain.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_domain.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_domain_code ON cmx_domain (code);

-- =============================================
-- 2. 应用表 (cmx_application)
-- =============================================
DROP TABLE IF EXISTS cmx_application;
CREATE TABLE cmx_application
(
    id          VARCHAR(64)  NOT NULL,
    code        VARCHAR(64)  NOT NULL,
    domain_code VARCHAR(64)  NOT NULL,
    name        VARCHAR(200) NOT NULL,
    description TEXT,
    type        VARCHAR(50),
    tags        TEXT,
    sort_order  INT4      DEFAULT 0,
    status      INT4      DEFAULT 1,
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by   VARCHAR(100),
    create_name VARCHAR(100),
    update_by   VARCHAR(100),
    update_name VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_application IS '应用表';
COMMENT
ON COLUMN cmx_application.id IS 'ID';
COMMENT
ON COLUMN cmx_application.code IS '应用编码，全局唯一，如: FI, CO, MM';
COMMENT
ON COLUMN cmx_application.domain_code IS '所属域编码，逻辑关联到cmx_domain.code';
COMMENT
ON COLUMN cmx_application.name IS '应用名称，如: 财务会计, 管理会计';
COMMENT
ON COLUMN cmx_application.description IS '应用描述';
COMMENT
ON COLUMN cmx_application.type IS '类型: product(产品应用), platform(平台应用), integration(集成应用)';
COMMENT
ON COLUMN cmx_application.tags IS '多标签，JSON数组字符串，如 ["财务核心","SAP_FI"]';
COMMENT
ON COLUMN cmx_application.sort_order IS '排序字段，数值小的靠前';
COMMENT
ON COLUMN cmx_application.status IS '状态：0-禁用，1-启用';
COMMENT
ON COLUMN cmx_application.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_application.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_application.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_application.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_application.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_application.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_application.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_application_code ON cmx_application (code);

-- =============================================
-- 3. 模块表 (cmx_module)
-- =============================================
DROP TABLE IF EXISTS cmx_module;
CREATE TABLE cmx_module
(
    id               VARCHAR(64)  NOT NULL,
    code             VARCHAR(64)  NOT NULL,
    domain_code      VARCHAR(64)  NOT NULL,
    application_code VARCHAR(64)  NOT NULL,
    name             VARCHAR(200) NOT NULL,
    description      TEXT,
    type             VARCHAR(50),
    tags             TEXT,
    sort_order       INT4      DEFAULT 0,
    status           INT4      DEFAULT 1,
    archived         INT4      DEFAULT 0,
    create_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by        VARCHAR(100),
    create_name      VARCHAR(100),
    update_by        VARCHAR(100),
    update_name      VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_module IS '模块表';
COMMENT
ON COLUMN cmx_module.id IS 'ID';
COMMENT
ON COLUMN cmx_module.code IS '模块编码，全局唯一，如: GL, AR, AP';
COMMENT
ON COLUMN cmx_module.domain_code IS '所属域编码';
COMMENT
ON COLUMN cmx_module.application_code IS '所属应用编码，逻辑关联到cmx_application.code';
COMMENT
ON COLUMN cmx_module.name IS '模块名称，如: 总账模块, 应收模块';
COMMENT
ON COLUMN cmx_module.description IS '模块描述';
COMMENT
ON COLUMN cmx_module.type IS '类型: business(业务模块), extension(扩展点), integration(集成点)';
COMMENT
ON COLUMN cmx_module.tags IS '多标签，JSON数组字符串，如 ["总账","核心","FI-GL"]';
COMMENT
ON COLUMN cmx_module.sort_order IS '排序字段，数值小的靠前';
COMMENT
ON COLUMN cmx_module.status IS '状态：0-禁用，1-启用';
COMMENT
ON COLUMN cmx_module.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_module.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_module.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_module.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_module.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_module.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_module.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_module_code ON cmx_module (code);

-- =============================================
-- 4. 数据源表 (cmx_sys_datasource)
-- =============================================
DROP TABLE IF EXISTS cmx_sys_datasource;
CREATE TABLE cmx_sys_datasource
(
    id                    VARCHAR(64)  NOT NULL,
    db_id                 VARCHAR(64),
    db_schema             VARCHAR(64),
    description           VARCHAR(255),
    db_type               VARCHAR(255) NOT NULL,
    db_url                VARCHAR(255),
    max_connections       INTEGER,
    min_connections       INTEGER,
    connect_timeout       INTEGER,
    idle_timeout          INTEGER,
    max_lifetime          INTEGER,
    health_check_interval INTEGER,
    health_check_timeout  INTEGER,
    default_flag          INTEGER,
    source                VARCHAR(20),
    status                INT4      DEFAULT 1,
    archived              INT4      DEFAULT 0,
    create_time           TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time           TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by             VARCHAR(100),
    create_name           VARCHAR(100),
    update_by             VARCHAR(100),
    update_name           VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_sys_datasource IS 'cmx数据源管理';
COMMENT
ON COLUMN cmx_sys_datasource.id IS '主键';
COMMENT
ON COLUMN cmx_sys_datasource.db_id IS '数据源标识';
COMMENT
ON COLUMN cmx_sys_datasource.db_schema IS '数据库模式';
COMMENT
ON COLUMN cmx_sys_datasource.description IS '数据源描述';
COMMENT
ON COLUMN cmx_sys_datasource.db_type IS '数据库类型(postgres;mysql)';
COMMENT
ON COLUMN cmx_sys_datasource.db_url IS '数据库连接 URL';
COMMENT
ON COLUMN cmx_sys_datasource.max_connections IS '最大连接数';
COMMENT
ON COLUMN cmx_sys_datasource.min_connections IS '最小空闲连接数';
COMMENT
ON COLUMN cmx_sys_datasource.connect_timeout IS '连接超时时间（秒）';
COMMENT
ON COLUMN cmx_sys_datasource.idle_timeout IS '空闲连接超时时间（秒）';
COMMENT
ON COLUMN cmx_sys_datasource.max_lifetime IS '最大生命周期（秒）';
COMMENT
ON COLUMN cmx_sys_datasource.health_check_interval IS '健康检查间隔（秒）';
COMMENT
ON COLUMN cmx_sys_datasource.health_check_timeout IS '健康检查超时（秒）';
COMMENT
ON COLUMN cmx_sys_datasource.default_flag IS '是否默认;0否1是';
COMMENT
ON COLUMN cmx_sys_datasource.source IS '数据源来源：config-配置文件, manual-手动维护';
COMMENT
ON COLUMN cmx_sys_datasource.status IS '状态：0-禁用，1-启用';
COMMENT
ON COLUMN cmx_sys_datasource.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_sys_datasource.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_sys_datasource.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_sys_datasource.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_sys_datasource.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_sys_datasource.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_sys_datasource.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_datasource_db_id ON cmx_sys_datasource (db_id);

-- =============================================
-- 5. 插件注册表 (cmx_plugin)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin;
CREATE TABLE cmx_plugin
(
    id                  VARCHAR(64)              NOT NULL,
    plugin_id           VARCHAR(255)             NOT NULL,
    name                VARCHAR(500)             NOT NULL,
    version             VARCHAR(50)              NOT NULL,
    wasm_path           TEXT                     NOT NULL,
    install_path        TEXT                     NOT NULL,
    db_id               VARCHAR(100),
    status              VARCHAR(30)                       DEFAULT 'installed',
    is_system           BOOLEAN                           DEFAULT FALSE,
    is_locked           BOOLEAN                           DEFAULT FALSE,
    domain_code         VARCHAR(64),
    application_code    VARCHAR(64),
    module_code         VARCHAR(64),
    vendor_name         VARCHAR(255),
    vendor_url          TEXT,
    vendor_contact      VARCHAR(255),
    metadata            JSONB,
    signature_algorithm VARCHAR(50),
    signer_key_id       VARCHAR(255),
    zip_source_url      VARCHAR(500),
    zip_source_type     VARCHAR(30),
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4                     NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100),
    plugin_type         VARCHAR(50),
    source_path         VARCHAR(500),
    description         TEXT,
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_plugin IS '插件注册主表：存储所有已安装插件的核心信息基线版本';
COMMENT
ON COLUMN cmx_plugin.id IS '主键ID';
COMMENT
ON COLUMN cmx_plugin.plugin_id IS '插件唯一标识 (如 "example_plugin")';
COMMENT
ON COLUMN cmx_plugin.name IS '显示名称';
COMMENT
ON COLUMN cmx_plugin.version IS '当前版本 (语义版本)';
COMMENT
ON COLUMN cmx_plugin.wasm_path IS 'WASM 文件绝对路径';
COMMENT
ON COLUMN cmx_plugin.install_path IS '安装根目录路径';
COMMENT
ON COLUMN cmx_plugin.db_id IS '插件业务数据存储的数据库ID';
COMMENT
ON COLUMN cmx_plugin.status IS '状态: installed(已安装), active(已激活), inactive(已停用), failed(失败)';
COMMENT
ON COLUMN cmx_plugin.is_system IS '是否系统默认插件';
COMMENT
ON COLUMN cmx_plugin.is_locked IS '是否被锁定 (防止卸载)';
COMMENT
ON COLUMN cmx_plugin.domain_code IS '所属域编码 (如 "FIN")';
COMMENT
ON COLUMN cmx_plugin.application_code IS '所属应用编码 (如 "GL_ACCT")';
COMMENT
ON COLUMN cmx_plugin.module_code IS '所属模块编码 (如 "GL")';
COMMENT
ON COLUMN cmx_plugin.vendor_name IS '开发商名称';
COMMENT
ON COLUMN cmx_plugin.vendor_url IS '开发商URL';
COMMENT
ON COLUMN cmx_plugin.vendor_contact IS '开发商联系方式';
COMMENT
ON COLUMN cmx_plugin.metadata IS '扩展元数据';
COMMENT
ON COLUMN cmx_plugin.signature_algorithm IS '签名算法';
COMMENT
ON COLUMN cmx_plugin.signer_key_id IS '签名密钥ID';
COMMENT
ON COLUMN cmx_plugin.zip_source_url IS '插件ZIP包来源地址';
COMMENT
ON COLUMN cmx_plugin.zip_source_type IS '插件来源类型: local(本地), url(远程URL), registry(注册表)';
COMMENT
ON COLUMN cmx_plugin.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_plugin.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_plugin.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_plugin.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_plugin.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_plugin.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_plugin.update_name IS '更新人名称';
COMMENT
ON COLUMN cmx_plugin.plugin_type IS '插件类型: wasm/rhai';
COMMENT
ON COLUMN cmx_plugin.source_path IS '源码路径';
COMMENT
ON COLUMN cmx_plugin.description IS '插件描述信息';

CREATE INDEX idx_plugin_domain_app_module ON cmx_plugin (domain_code, application_code, module_code);
ALTER TABLE cmx_plugin
    ADD CONSTRAINT uk_cmx_plugin_plugin_id UNIQUE (plugin_id);

-- =============================================
-- 6. 版本历史表 (cmx_plugin_versions)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_versions;
CREATE TABLE cmx_plugin_versions
(
    id              VARCHAR(64)              NOT NULL,
    plugin_id       VARCHAR(64)              NOT NULL,
    version         VARCHAR(50)              NOT NULL,
    install_path    TEXT                     NOT NULL,
    wasm_path       TEXT                     NOT NULL,
    is_current      BOOLEAN                  NOT NULL DEFAULT FALSE,
    installed_at    TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    uninstalled_at  TIMESTAMP WITH TIME ZONE,
    zip_source_url  VARCHAR(500),
    zip_source_type VARCHAR(30),
    build_type      VARCHAR(30),
    create_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived        INT4                     NOT NULL DEFAULT 0,
    create_by       VARCHAR(100),
    create_name     VARCHAR(100),
    update_by       VARCHAR(100),
    update_name     VARCHAR(100),
    plugin_type     VARCHAR(50),
    source_path     VARCHAR(500),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_plugin_versions IS '插件版本历史表：记录插件的版本历史';
COMMENT
ON COLUMN cmx_plugin_versions.id IS '主键ID';
COMMENT
ON COLUMN cmx_plugin_versions.plugin_id IS '关联插件ID';
COMMENT
ON COLUMN cmx_plugin_versions.version IS '版本号';
COMMENT
ON COLUMN cmx_plugin_versions.install_path IS '该版本的安装路径';
COMMENT
ON COLUMN cmx_plugin_versions.wasm_path IS '该版本的 WASM 路径';
COMMENT
ON COLUMN cmx_plugin_versions.is_current IS '是否当前版本';
COMMENT
ON COLUMN cmx_plugin_versions.installed_at IS '安装时间';
COMMENT
ON COLUMN cmx_plugin_versions.uninstalled_at IS '卸载时间';
COMMENT
ON COLUMN cmx_plugin_versions.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_plugin_versions.zip_source_url IS '该版本插件ZIP包来源地址';
COMMENT
ON COLUMN cmx_plugin_versions.zip_source_type IS '该版本插件来源类型: local(本地), url(远程URL), registry(注册表)';
COMMENT
ON COLUMN cmx_plugin_versions.build_type IS '构建类型: debug/release';
COMMENT
ON COLUMN cmx_plugin_versions.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_plugin_versions.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_plugin_versions.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_plugin_versions.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_plugin_versions.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_plugin_versions.update_name IS '更新人名称';
COMMENT
ON COLUMN cmx_plugin_versions.plugin_type IS '插件类型: wasm/rhai';
COMMENT
ON COLUMN cmx_plugin_versions.source_path IS '源码路径';

CREATE INDEX idx_version_plugin ON cmx_plugin_versions (plugin_id);
CREATE INDEX idx_version_current ON cmx_plugin_versions (plugin_id, is_current) WHERE is_current = TRUE;
ALTER TABLE cmx_plugin_versions
    ADD CONSTRAINT uk_cmx_plugin_versions_plugin_version UNIQUE (plugin_id, version);

-- =============================================
-- 7. 依赖关系表 (cmx_plugin_dependencies)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_dependencies;
CREATE TABLE cmx_plugin_dependencies
(
    id                   VARCHAR(64)              NOT NULL,
    plugin_id            VARCHAR(64)              NOT NULL,
    dependency_plugin_id VARCHAR(255)             NOT NULL,
    dependency_name      VARCHAR(500),
    version_constraint   VARCHAR(100),
    min_version          VARCHAR(50),
    max_version          VARCHAR(50),
    is_optional          BOOLEAN                  NOT NULL DEFAULT FALSE,
    is_dev               BOOLEAN                  NOT NULL DEFAULT FALSE,
    resolved_version     VARCHAR(50),
    resolution_status    VARCHAR(30),
    create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived             INT4                     NOT NULL DEFAULT 0,
    create_by            VARCHAR(100),
    create_name          VARCHAR(100),
    update_by            VARCHAR(100),
    update_name          VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_plugin_dependencies IS '插件依赖关系表：记录插件之间的依赖关系';
COMMENT
ON COLUMN cmx_plugin_dependencies.id IS '主键ID';
COMMENT
ON COLUMN cmx_plugin_dependencies.plugin_id IS '关联插件ID';
COMMENT
ON COLUMN cmx_plugin_dependencies.dependency_plugin_id IS '依赖的插件ID (可能是未安装的)';
COMMENT
ON COLUMN cmx_plugin_dependencies.dependency_name IS '依赖的插件名称';
COMMENT
ON COLUMN cmx_plugin_dependencies.version_constraint IS '版本约束 (如 "^1.0.0", "~2.1.0", ">=1.0.0 <3.0.0")';
COMMENT
ON COLUMN cmx_plugin_dependencies.min_version IS '最小版本';
COMMENT
ON COLUMN cmx_plugin_dependencies.max_version IS '最大版本';
COMMENT
ON COLUMN cmx_plugin_dependencies.is_optional IS '是否可选依赖';
COMMENT
ON COLUMN cmx_plugin_dependencies.is_dev IS '是否开发依赖';
COMMENT
ON COLUMN cmx_plugin_dependencies.resolved_version IS '已解析的版本';
COMMENT
ON COLUMN cmx_plugin_dependencies.resolution_status IS '解析状态: resolved, conflict, missing';
COMMENT
ON COLUMN cmx_plugin_dependencies.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_plugin_dependencies.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_plugin_dependencies.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_plugin_dependencies.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_plugin_dependencies.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_plugin_dependencies.update_name IS '更新人名称';

CREATE INDEX idx_dep_plugin ON cmx_plugin_dependencies (plugin_id);
CREATE INDEX idx_dep_resolved ON cmx_plugin_dependencies (plugin_id, resolution_status);

-- =============================================
-- 8. 节点部署记录表 (cmx_plugin_deployments)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_deployments;
CREATE TABLE cmx_plugin_deployments
(
    id            VARCHAR(64)              NOT NULL,
    plugin_id     VARCHAR(64)              NOT NULL,
    node_id       VARCHAR(100)             NOT NULL,
    node_type     VARCHAR(50),
    version       VARCHAR(50)              NOT NULL,
    status        VARCHAR(30),
    progress      INTEGER                           DEFAULT 0,
    error_message TEXT,
    error_details TEXT,
    create_time   TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time   TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived      INT4                     NOT NULL DEFAULT 0,
    create_by     VARCHAR(100),
    create_name   VARCHAR(100),
    update_by     VARCHAR(100),
    update_name   VARCHAR(100),
    plugin_type   VARCHAR(50),
    source_path   TEXT,
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_plugin_deployments IS '节点插件部署记录表：记录插件在各个节点上的部署状态';
COMMENT
ON COLUMN cmx_plugin_deployments.id IS '主键ID';
COMMENT
ON COLUMN cmx_plugin_deployments.plugin_id IS '关联插件ID';
COMMENT
ON COLUMN cmx_plugin_deployments.node_id IS '节点标识';
COMMENT
ON COLUMN cmx_plugin_deployments.node_type IS '节点类型: primary, replica, worker';
COMMENT
ON COLUMN cmx_plugin_deployments.version IS '部署的版本';
COMMENT
ON COLUMN cmx_plugin_deployments.status IS '部署状态';
COMMENT
ON COLUMN cmx_plugin_deployments.progress IS '进度 (0-100)';
COMMENT
ON COLUMN cmx_plugin_deployments.error_message IS '错误信息';
COMMENT
ON COLUMN cmx_plugin_deployments.error_details IS '错误详情';
COMMENT
ON COLUMN cmx_plugin_deployments.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_plugin_deployments.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_plugin_deployments.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_plugin_deployments.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_plugin_deployments.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_plugin_deployments.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_plugin_deployments.update_name IS '更新人名称';
COMMENT
ON COLUMN cmx_plugin_deployments.plugin_type IS '插件类型: wasm/rhai';
COMMENT
ON COLUMN cmx_plugin_deployments.source_path IS '源码路径';

CREATE INDEX idx_deploy_plugin ON cmx_plugin_deployments (plugin_id);
CREATE INDEX idx_deploy_node ON cmx_plugin_deployments (node_id);
CREATE INDEX idx_deploy_status ON cmx_plugin_deployments (status);
ALTER TABLE cmx_plugin_deployments
    ADD CONSTRAINT uk_cmx_plugin_deployments_plugin_node_version UNIQUE (plugin_id, node_id, version);

-- =============================================
-- 9. 审计日志表 (cmx_plugin_audit_log)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_audit_log;
CREATE TABLE cmx_plugin_audit_log
(
    id               VARCHAR(64)              NOT NULL,
    plugin_id        VARCHAR(64),
    node_id          VARCHAR(64),
    version          VARCHAR(64),
    deployment_id    VARCHAR(64),
    operation_type   VARCHAR(50)              NOT NULL,
    operation_status VARCHAR(30)              NOT NULL,
    request_id       VARCHAR(100),
    details          TEXT,
    old_value        TEXT,
    new_value        TEXT,
    error_code       VARCHAR(50),
    error_message    TEXT,
    stack_trace      TEXT,
    started_at       TIMESTAMP WITH TIME ZONE,
    completed_at     TIMESTAMP WITH TIME ZONE,
    duration_ms      BIGINT,
    create_time      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived         INT4                     NOT NULL DEFAULT 0,
    create_by        VARCHAR(100),
    create_name      VARCHAR(100),
    update_by        VARCHAR(100),
    update_name      VARCHAR(100)
);

COMMENT
ON TABLE cmx_plugin_audit_log IS '审计日志表：记录插件操作日志';
COMMENT
ON COLUMN cmx_plugin_audit_log.id IS '主键ID';
COMMENT
ON COLUMN cmx_plugin_audit_log.plugin_id IS '关联插件ID';
COMMENT
ON COLUMN cmx_plugin_audit_log.node_id IS '节点ID';
COMMENT
ON COLUMN cmx_plugin_audit_log.version IS '插件版本';
COMMENT
ON COLUMN cmx_plugin_audit_log.deployment_id IS '关联部署ID';
COMMENT
ON COLUMN cmx_plugin_audit_log.operation_type IS '操作类型';
COMMENT
ON COLUMN cmx_plugin_audit_log.operation_status IS '操作状态';
COMMENT
ON COLUMN cmx_plugin_audit_log.request_id IS '请求 ID (用于链路追踪)';
COMMENT
ON COLUMN cmx_plugin_audit_log.details IS '操作详情 (JSON)';
COMMENT
ON COLUMN cmx_plugin_audit_log.old_value IS '旧值';
COMMENT
ON COLUMN cmx_plugin_audit_log.new_value IS '新值';
COMMENT
ON COLUMN cmx_plugin_audit_log.error_code IS '错误代码';
COMMENT
ON COLUMN cmx_plugin_audit_log.error_message IS '错误消息';
COMMENT
ON COLUMN cmx_plugin_audit_log.stack_trace IS '堆栈跟踪';
COMMENT
ON COLUMN cmx_plugin_audit_log.started_at IS '操作开始时间';
COMMENT
ON COLUMN cmx_plugin_audit_log.completed_at IS '操作完成时间';
COMMENT
ON COLUMN cmx_plugin_audit_log.duration_ms IS '操作耗时 (毫秒)';
COMMENT
ON COLUMN cmx_plugin_audit_log.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_plugin_audit_log.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_plugin_audit_log.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_plugin_audit_log.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_plugin_audit_log.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_plugin_audit_log.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_plugin_audit_log.update_name IS '更新人名称';

CREATE INDEX idx_audit_plugin ON cmx_plugin_audit_log (plugin_id);
CREATE INDEX idx_audit_node ON cmx_plugin_audit_log (node_id);
CREATE INDEX idx_audit_operation ON cmx_plugin_audit_log (operation_type);
CREATE INDEX idx_audit_timestamp ON cmx_plugin_audit_log (started_at);
CREATE INDEX idx_audit_request ON cmx_plugin_audit_log (request_id);

-- =============================================
-- 10. 系统默认插件配置表 (cmx_system_plugins)
-- =============================================
DROP TABLE IF EXISTS cmx_system_plugins;
CREATE TABLE cmx_system_plugins
(
    id                  VARCHAR(64)              NOT NULL,
    plugin_id           VARCHAR(255)             NOT NULL,
    name                VARCHAR(500)             NOT NULL,
    version             VARCHAR(50)              NOT NULL,
    install_order       INTEGER                  NOT NULL DEFAULT 0,
    is_optional         BOOLEAN                  NOT NULL DEFAULT FALSE,
    is_critical         BOOLEAN                  NOT NULL DEFAULT FALSE,
    retry_count         INTEGER                  NOT NULL DEFAULT 3,
    retry_delay_seconds INTEGER                  NOT NULL DEFAULT 10,
    wait_for_plugins    VARCHAR(255),
    source_type         VARCHAR(30)              NOT NULL,
    source_path         TEXT,
    source_url          TEXT,
    status              VARCHAR(30)              NOT NULL DEFAULT 'pending',
    last_installed_at   TIMESTAMP WITH TIME ZONE,
    install_attempts    INTEGER                  NOT NULL DEFAULT 0,
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived            INT4                     NOT NULL DEFAULT 0,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100)
);

COMMENT
ON TABLE cmx_system_plugins IS '系统默认插件配置表：配置系统启动时需要自动安装的插件';
COMMENT
ON COLUMN cmx_system_plugins.id IS '主键ID';
COMMENT
ON COLUMN cmx_system_plugins.plugin_id IS '插件唯一标识';
COMMENT
ON COLUMN cmx_system_plugins.name IS '插件名称';
COMMENT
ON COLUMN cmx_system_plugins.version IS '插件版本';
COMMENT
ON COLUMN cmx_system_plugins.install_order IS '安装顺序 (数字越小越先安装)';
COMMENT
ON COLUMN cmx_system_plugins.is_optional IS '是否可选 (可选则安装失败不阻止启动)';
COMMENT
ON COLUMN cmx_system_plugins.is_critical IS '是否关键 (关键插件失败导致系统无法启动)';
COMMENT
ON COLUMN cmx_system_plugins.retry_count IS '重试次数';
COMMENT
ON COLUMN cmx_system_plugins.retry_delay_seconds IS '重试间隔 (秒)';
COMMENT
ON COLUMN cmx_system_plugins.wait_for_plugins IS '需要等待完成的插件列表';
COMMENT
ON COLUMN cmx_system_plugins.source_type IS '来源类型: bundled, url';
COMMENT
ON COLUMN cmx_system_plugins.source_path IS '来源路径 (bundled 时为内置路径)';
COMMENT
ON COLUMN cmx_system_plugins.source_url IS '来源 URL (url 类型时使用)';
COMMENT
ON COLUMN cmx_system_plugins.status IS '状态';
COMMENT
ON COLUMN cmx_system_plugins.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_system_plugins.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_system_plugins.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_system_plugins.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_system_plugins.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_system_plugins.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_system_plugins.update_name IS '更新人名称';

CREATE INDEX idx_system_plugin_order ON cmx_system_plugins (install_order);
CREATE INDEX idx_system_plugin_status ON cmx_system_plugins (status);

-- =============================================
-- 11. 节点信息表 (cmx_plugin_nodes)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_nodes;
CREATE TABLE cmx_plugin_nodes
(
    node_id                VARCHAR(64)              NOT NULL,
    node_name              VARCHAR(255)             NOT NULL,
    node_type              VARCHAR(30)              NOT NULL,
    status                 VARCHAR(30)              NOT NULL,
    is_active              BOOLEAN                  NOT NULL DEFAULT TRUE,
    host                   VARCHAR(255)             NOT NULL,
    port                   INTEGER                  NOT NULL,
    protocol               VARCHAR(10)              NOT NULL DEFAULT 'http',
    capabilities           TEXT,
    last_health_check      TIMESTAMP WITH TIME ZONE,
    health_check_interval  INTEGER                  NOT NULL DEFAULT 30,
    plugin_manager_version VARCHAR(50),
    registered_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_seen_at           TIMESTAMP WITH TIME ZONE,
    create_time            TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time            TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived               INT4                     NOT NULL DEFAULT 0,
    create_by              VARCHAR(100),
    create_name            VARCHAR(100),
    update_by              VARCHAR(100),
    update_name            VARCHAR(100)
);

COMMENT
ON TABLE cmx_plugin_nodes IS '节点信息表：记录集群中的节点信息';
COMMENT
ON COLUMN cmx_plugin_nodes.node_id IS '节点ID';
COMMENT
ON COLUMN cmx_plugin_nodes.node_name IS '节点名称';
COMMENT
ON COLUMN cmx_plugin_nodes.node_type IS '节点类型: primary, replica, worker';
COMMENT
ON COLUMN cmx_plugin_nodes.status IS '节点状态: online, offline, maintenance';
COMMENT
ON COLUMN cmx_plugin_nodes.is_active IS '是否激活';
COMMENT
ON COLUMN cmx_plugin_nodes.host IS '主机地址';
COMMENT
ON COLUMN cmx_plugin_nodes.port IS '端口';
COMMENT
ON COLUMN cmx_plugin_nodes.protocol IS '协议';
COMMENT
ON COLUMN cmx_plugin_nodes.capabilities IS '节点能力';
COMMENT
ON COLUMN cmx_plugin_nodes.last_health_check IS '最后健康检查时间';
COMMENT
ON COLUMN cmx_plugin_nodes.health_check_interval IS '健康检查间隔 (秒)';
COMMENT
ON COLUMN cmx_plugin_nodes.plugin_manager_version IS '插件管理器版本';
COMMENT
ON COLUMN cmx_plugin_nodes.registered_at IS '注册时间';
COMMENT
ON COLUMN cmx_plugin_nodes.last_seen_at IS '最后可见时间';
COMMENT
ON COLUMN cmx_plugin_nodes.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_plugin_nodes.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_plugin_nodes.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_plugin_nodes.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_plugin_nodes.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_plugin_nodes.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_plugin_nodes.update_name IS '更新人名称';

CREATE INDEX idx_node_status ON cmx_plugin_nodes (status);
CREATE INDEX idx_node_type ON cmx_plugin_nodes (node_type);

-- =============================================
-- 12. 插件功能表 (cmx_plugin_features)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_features;
CREATE TABLE cmx_plugin_features
(
    id             VARCHAR(64)              NOT NULL,
    plugin_id      VARCHAR(64)              NOT NULL,
    plugin_version VARCHAR(50)              NOT NULL,
    feature_id     VARCHAR(255)             NOT NULL,
    feature_name   VARCHAR(500)             NOT NULL,
    feature_type   VARCHAR(50)              NOT NULL,
    description    TEXT,
    config         JSONB,
    status         VARCHAR(30)              NOT NULL DEFAULT 'active',
    create_time    TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time    TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived       INT4                     NOT NULL DEFAULT 0,
    create_by      VARCHAR(100),
    create_name    VARCHAR(100),
    update_by      VARCHAR(100),
    update_name    VARCHAR(100)
);

COMMENT
ON TABLE cmx_plugin_features IS '插件功能表';
COMMENT
ON COLUMN cmx_plugin_features.id IS '主键ID';
COMMENT
ON COLUMN cmx_plugin_features.plugin_id IS '关联插件ID';
COMMENT
ON COLUMN cmx_plugin_features.plugin_version IS '插件版本';
COMMENT
ON COLUMN cmx_plugin_features.feature_id IS '功能唯一标识';
COMMENT
ON COLUMN cmx_plugin_features.feature_name IS '功能名称';
COMMENT
ON COLUMN cmx_plugin_features.feature_type IS '功能类型: service, event_handler, scheduler, api,function';
COMMENT
ON COLUMN cmx_plugin_features.description IS '功能描述';
COMMENT
ON COLUMN cmx_plugin_features.config IS '功能配置';
COMMENT
ON COLUMN cmx_plugin_features.status IS '状态: active, inactive, error';
COMMENT
ON COLUMN cmx_plugin_features.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_plugin_features.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_plugin_features.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_plugin_features.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_plugin_features.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_plugin_features.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_plugin_features.update_name IS '更新人名称';

-- =============================================
-- 13. 表定义元数据表 (cmx_meta_table_define)
-- =============================================
DROP TABLE IF EXISTS cmx_meta_table_define;
CREATE TABLE cmx_meta_table_define
(
    id               VARCHAR(64) NOT NULL,
    table_name       VARCHAR(100),
    display_name     VARCHAR(100),
    db_id            VARCHAR(100),
    plugin_id        VARCHAR(64),
    version          VARCHAR(50),
    domain_code      VARCHAR(100),
    application_code VARCHAR(100),
    module_code      VARCHAR(100),
    archived         INT4      DEFAULT 0,
    create_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by        VARCHAR(100),
    create_name      VARCHAR(100),
    update_by        VARCHAR(100),
    update_name      VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_meta_table_define IS '表定义元数据';
COMMENT
ON COLUMN cmx_meta_table_define.id IS '主键';
COMMENT
ON COLUMN cmx_meta_table_define.table_name IS '表名';
COMMENT
ON COLUMN cmx_meta_table_define.display_name IS '显示名称';
COMMENT
ON COLUMN cmx_meta_table_define.db_id IS '所属数据库id';
COMMENT
ON COLUMN cmx_meta_table_define.plugin_id IS '插件id';
COMMENT
ON COLUMN cmx_meta_table_define.version IS '当前使用的元数据插件版本';
COMMENT
ON COLUMN cmx_meta_table_define.domain_code IS '域编码';
COMMENT
ON COLUMN cmx_meta_table_define.application_code IS '应用编码';
COMMENT
ON COLUMN cmx_meta_table_define.module_code IS '模块编码';
COMMENT
ON COLUMN cmx_meta_table_define.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_meta_table_define.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_meta_table_define.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_meta_table_define.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_meta_table_define.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_meta_table_define.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_meta_table_define.update_name IS '更新人名称';

-- =============================================
-- 14. 表定义元数据版本表 (cmx_meta_table_define_version)
-- =============================================
DROP TABLE IF EXISTS cmx_meta_table_define_version;
CREATE TABLE cmx_meta_table_define_version
(
    id               VARCHAR(64) NOT NULL,
    table_name       VARCHAR(100),
    display_name     VARCHAR(100),
    db_id            VARCHAR(100),
    plugin_id        VARCHAR(64),
    version          VARCHAR(50),
    domain_code      VARCHAR(100),
    application_code VARCHAR(100),
    module_code      VARCHAR(100),
    metadata         JSONB,
    archived         INT4      DEFAULT 0,
    create_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by        VARCHAR(100),
    create_name      VARCHAR(100),
    update_by        VARCHAR(100),
    update_name      VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_meta_table_define_version IS '表元数据版本表';
COMMENT
ON COLUMN cmx_meta_table_define_version.id IS '主键';
COMMENT
ON COLUMN cmx_meta_table_define_version.table_name IS '表名';
COMMENT
ON COLUMN cmx_meta_table_define_version.display_name IS '显示名称';
COMMENT
ON COLUMN cmx_meta_table_define_version.db_id IS '所属数据库id';
COMMENT
ON COLUMN cmx_meta_table_define_version.plugin_id IS '插件id';
COMMENT
ON COLUMN cmx_meta_table_define_version.version IS '插件版本';
COMMENT
ON COLUMN cmx_meta_table_define_version.domain_code IS '域编码';
COMMENT
ON COLUMN cmx_meta_table_define_version.application_code IS '应用编码';
COMMENT
ON COLUMN cmx_meta_table_define_version.module_code IS '模块编码';
COMMENT
ON COLUMN cmx_meta_table_define_version.metadata IS '元数据json';
COMMENT
ON COLUMN cmx_meta_table_define_version.archived IS '归档标志：0-未归档，1-已归档';
COMMENT
ON COLUMN cmx_meta_table_define_version.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_meta_table_define_version.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_meta_table_define_version.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_meta_table_define_version.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_meta_table_define_version.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_meta_table_define_version.update_name IS '更新人名称';

-- =============================================
-- 15. 服务定义表 (cmx_service_define)
-- =============================================
DROP TABLE IF EXISTS cmx_service_define;
CREATE TABLE cmx_service_define
(
    id               VARCHAR(64)  NOT NULL,
    service_key      VARCHAR(100) NOT NULL,
    service_name     VARCHAR(100),
    description      VARCHAR(255),
    plugin_id        VARCHAR(64),
    domain_code      VARCHAR(64),
    application_code VARCHAR(64),
    module_code      VARCHAR(64),
    status           INT4      DEFAULT 1,
    version          VARCHAR(50),
    create_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by        VARCHAR(100),
    create_name      VARCHAR(100),
    update_by        VARCHAR(100),
    update_name      VARCHAR(100),
    archived         INT4      DEFAULT 0,
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_service_define IS '服务定义表';
COMMENT
ON COLUMN cmx_service_define.id IS '主键';
COMMENT
ON COLUMN cmx_service_define.service_key IS '服务key';
COMMENT
ON COLUMN cmx_service_define.service_name IS '服务名称';
COMMENT
ON COLUMN cmx_service_define.description IS '服务描述';
COMMENT
ON COLUMN cmx_service_define.plugin_id IS '所属插件id';
COMMENT
ON COLUMN cmx_service_define.domain_code IS '所属域';
COMMENT
ON COLUMN cmx_service_define.application_code IS '所属应用';
COMMENT
ON COLUMN cmx_service_define.module_code IS '所属模块';
COMMENT
ON COLUMN cmx_service_define.status IS '状态：0-禁用，1-启用';
COMMENT
ON COLUMN cmx_service_define.version IS '服务版本';
COMMENT
ON COLUMN cmx_service_define.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_service_define.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_service_define.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_service_define.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_service_define.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_service_define.update_name IS '更新人名称';
COMMENT
ON COLUMN cmx_service_define.archived IS '归档标志：0-未归档，1-已归档';

CREATE UNIQUE INDEX uk_cmx_service_define_key ON cmx_service_define (service_key);

-- =============================================
-- 16. 服务定义版本表 (cmx_service_define_version)
-- =============================================
DROP TABLE IF EXISTS cmx_service_define_version;
CREATE TABLE cmx_service_define_version
(
    id             VARCHAR(64) NOT NULL,
    service_key    VARCHAR(100),
    version        VARCHAR(50),
    plugin_id      VARCHAR(64),
    plugin_version VARCHAR(50),
    config         TEXT,
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by      VARCHAR(100),
    create_name    VARCHAR(100),
    update_by      VARCHAR(100),
    update_name    VARCHAR(100),
    archived       INT4      DEFAULT 0,
    PRIMARY KEY (id)
);

COMMENT
ON TABLE cmx_service_define_version IS '服务定义版本表';
COMMENT
ON COLUMN cmx_service_define_version.id IS '主键';
COMMENT
ON COLUMN cmx_service_define_version.service_key IS '服务key';
COMMENT
ON COLUMN cmx_service_define_version.version IS '服务版本';
COMMENT
ON COLUMN cmx_service_define_version.plugin_id IS '服务所属插件';
COMMENT
ON COLUMN cmx_service_define_version.plugin_version IS '所属插件版本';
COMMENT
ON COLUMN cmx_service_define_version.config IS '服务编排配置';
COMMENT
ON COLUMN cmx_service_define_version.create_time IS '创建时间';
COMMENT
ON COLUMN cmx_service_define_version.update_time IS '更新时间';
COMMENT
ON COLUMN cmx_service_define_version.create_by IS '创建人ID';
COMMENT
ON COLUMN cmx_service_define_version.create_name IS '创建人名称';
COMMENT
ON COLUMN cmx_service_define_version.update_by IS '更新人ID';
COMMENT
ON COLUMN cmx_service_define_version.update_name IS '更新人名称';
COMMENT
ON COLUMN cmx_service_define_version.archived IS '归档标志：0-未归档，1-已归档';

CREATE INDEX cmx_service_define_version_service_key_index ON cmx_service_define_version (service_key);
