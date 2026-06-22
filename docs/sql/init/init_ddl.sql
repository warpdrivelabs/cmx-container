-- =============================================
-- cmx-container 数据库定义 (DDL)
-- 包含：域/应用/模块/数据源表、插件表、服务表、插件市场表、文件存储表
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

COMMENT ON TABLE cmx_domain IS '域表';
COMMENT ON COLUMN cmx_domain.id IS 'ID';
COMMENT ON COLUMN cmx_domain.code IS '域编码，全局唯一，如: FIN, HR, SCM';
COMMENT ON COLUMN cmx_domain.name IS '域名称，如: 财务域, 人力资源域';
COMMENT ON COLUMN cmx_domain.description IS '域描述';
COMMENT ON COLUMN cmx_domain.type IS '类型: business(业务域), technical(技术域), product_line(产品线)';
COMMENT ON COLUMN cmx_domain.tags IS '多标签，JSON数组字符串，如 ["财务","核心","S4HANA"]';
COMMENT ON COLUMN cmx_domain.sort_order IS '排序字段，数值小的靠前';
COMMENT ON COLUMN cmx_domain.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_domain.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_domain.create_time IS '创建时间';
COMMENT ON COLUMN cmx_domain.update_time IS '更新时间';
COMMENT ON COLUMN cmx_domain.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_domain.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_domain.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_domain.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_core_domain_code ON cmx_domain (code);

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

COMMENT ON TABLE cmx_application IS '应用表';
COMMENT ON COLUMN cmx_application.id IS 'ID';
COMMENT ON COLUMN cmx_application.code IS '应用编码，全局唯一，如: FI, CO, MM';
COMMENT ON COLUMN cmx_application.domain_code IS '所属域编码，逻辑关联到cmx_domain.code';
COMMENT ON COLUMN cmx_application.name IS '应用名称，如: 财务会计, 管理会计';
COMMENT ON COLUMN cmx_application.description IS '应用描述';
COMMENT ON COLUMN cmx_application.type IS '类型: product(产品应用), platform(平台应用), integration(集成应用)';
COMMENT ON COLUMN cmx_application.tags IS '多标签，JSON数组字符串，如 ["财务核心","SAP_FI"]';
COMMENT ON COLUMN cmx_application.sort_order IS '排序字段，数值小的靠前';
COMMENT ON COLUMN cmx_application.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_application.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_application.create_time IS '创建时间';
COMMENT ON COLUMN cmx_application.update_time IS '更新时间';
COMMENT ON COLUMN cmx_application.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_application.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_application.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_application.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_coreapplication_code ON cmx_application (code);

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

COMMENT ON TABLE cmx_module IS '模块表';
COMMENT ON COLUMN cmx_module.id IS 'ID';
COMMENT ON COLUMN cmx_module.code IS '模块编码，全局唯一，如: GL, AR, AP';
COMMENT ON COLUMN cmx_module.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_module.application_code IS '所属应用编码，逻辑关联到cmx_application.code';
COMMENT ON COLUMN cmx_module.name IS '模块名称，如: 总账模块, 应收模块';
COMMENT ON COLUMN cmx_module.description IS '模块描述';
COMMENT ON COLUMN cmx_module.type IS '类型: business(业务模块), extension(扩展点), integration(集成点)';
COMMENT ON COLUMN cmx_module.tags IS '多标签，JSON数组字符串，如 ["总账","核心","FI-GL"]';
COMMENT ON COLUMN cmx_module.sort_order IS '排序字段，数值小的靠前';
COMMENT ON COLUMN cmx_module.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_module.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_module.create_time IS '创建时间';
COMMENT ON COLUMN cmx_module.update_time IS '更新时间';
COMMENT ON COLUMN cmx_module.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_module.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_module.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_module.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_coremodule_code ON cmx_module (code);

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

COMMENT ON TABLE cmx_sys_datasource IS 'cmx数据源管理';
COMMENT ON COLUMN cmx_sys_datasource.id IS '主键';
COMMENT ON COLUMN cmx_sys_datasource.db_id IS '数据源标识';
COMMENT ON COLUMN cmx_sys_datasource.db_schema IS '数据库模式';
COMMENT ON COLUMN cmx_sys_datasource.description IS '数据源描述';
COMMENT ON COLUMN cmx_sys_datasource.db_type IS '数据库类型(postgres;mysql)';
COMMENT ON COLUMN cmx_sys_datasource.db_url IS '数据库连接 URL';
COMMENT ON COLUMN cmx_sys_datasource.max_connections IS '最大连接数';
COMMENT ON COLUMN cmx_sys_datasource.min_connections IS '最小空闲连接数';
COMMENT ON COLUMN cmx_sys_datasource.connect_timeout IS '连接超时时间（秒）';
COMMENT ON COLUMN cmx_sys_datasource.idle_timeout IS '空闲连接超时时间（秒）';
COMMENT ON COLUMN cmx_sys_datasource.max_lifetime IS '最大生命周期（秒）';
COMMENT ON COLUMN cmx_sys_datasource.health_check_interval IS '健康检查间隔（秒）';
COMMENT ON COLUMN cmx_sys_datasource.health_check_timeout IS '健康检查超时（秒）';
COMMENT ON COLUMN cmx_sys_datasource.default_flag IS '是否默认;0否1是';
COMMENT ON COLUMN cmx_sys_datasource.source IS '数据源来源：config-配置文件, manual-手动维护';
COMMENT ON COLUMN cmx_sys_datasource.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_sys_datasource.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_sys_datasource.create_time IS '创建时间';
COMMENT ON COLUMN cmx_sys_datasource.update_time IS '更新时间';
COMMENT ON COLUMN cmx_sys_datasource.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_sys_datasource.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_sys_datasource.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_sys_datasource.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_datasource_db_id ON cmx_sys_datasource (db_id);

-- =============================================
-- 5. 插件注册表 (cmx_plugin)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin;
CREATE TABLE cmx_plugin
(
    id                    VARCHAR(64)              NOT NULL,
    plugin_id             VARCHAR(255)             NOT NULL,
    app_id           VARCHAR(64) NOT NULL DEFAULT 'default',
    name                  VARCHAR(500)             NOT NULL,
    version               VARCHAR(50)              NOT NULL,
    wasm_path             TEXT                     NOT NULL,
    install_path          TEXT                     NOT NULL,
    db_id                 VARCHAR(100),
    status                VARCHAR(30)                       DEFAULT 'installed',
    is_system             BOOLEAN                           DEFAULT FALSE,
    is_locked             BOOLEAN                           DEFAULT FALSE,
    domain_code           VARCHAR(64),
    application_code      VARCHAR(64),
    module_code           VARCHAR(64),
    vendor_name           VARCHAR(255),
    vendor_url            TEXT,
    vendor_contact        VARCHAR(255),
    metadata              JSONB,
    signature_algorithm   VARCHAR(50),
    signer_key_id         VARCHAR(255),
    zip_source_url        VARCHAR(500),
    zip_source_type       VARCHAR(30),
    storage_key      VARCHAR(500),
    storage_checksum VARCHAR(128),
    create_time           TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time           TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived              INT4                     NOT NULL DEFAULT 0,
    create_by             VARCHAR(100),
    create_name           VARCHAR(100),
    update_by             VARCHAR(100),
    update_name           VARCHAR(100),
    plugin_type           VARCHAR(50),
    source_path           VARCHAR(500),
    description           TEXT,
    marketplace_source_id VARCHAR(64),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_plugin IS '插件注册主表：存储所有已安装插件的核心信息基线版本';
COMMENT ON COLUMN cmx_plugin.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin.plugin_id IS '插件唯一标识 (如 "example_plugin")';
COMMENT ON COLUMN cmx_plugin.app_id IS '应用隔离标识，用于多租户或多应用场景下的插件隔离';
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
COMMENT ON COLUMN cmx_plugin.storage_key IS '存储键，标识插件包在存储系统中的唯一键';
COMMENT ON COLUMN cmx_plugin.storage_checksum IS '存储校验和，用于验证插件包完整性';
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
COMMENT ON COLUMN cmx_plugin.marketplace_source_id IS '市场版本来源ID，关联 cmx_marketplace_plugin_version.id，非市场安装时为 NULL';

CREATE INDEX idx_plugin_domain_app_module ON cmx_plugin (domain_code, application_code, module_code);
CREATE UNIQUE INDEX uk_cmx_plugin_app_plugin ON cmx_plugin (app_id, plugin_id);
CREATE INDEX idx_plugin_app_id ON cmx_plugin (app_id);

-- =============================================
-- 6. 版本历史表 (cmx_plugin_versions)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_versions;
CREATE TABLE cmx_plugin_versions
(
    id                    VARCHAR(64)              NOT NULL,
    plugin_id             VARCHAR(64)              NOT NULL,
    app_id VARCHAR(64) NOT NULL DEFAULT 'default',
    version               VARCHAR(50)              NOT NULL,
    install_path          TEXT                     NOT NULL,
    wasm_path             TEXT                     NOT NULL,
    is_current            BOOLEAN                  NOT NULL DEFAULT FALSE,
    installed_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    uninstalled_at        TIMESTAMP WITH TIME ZONE,
    zip_source_url        VARCHAR(500),
    zip_source_type       VARCHAR(30),
    build_type            VARCHAR(30),
    create_time           TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time           TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived              INT4                     NOT NULL DEFAULT 0,
    create_by             VARCHAR(100),
    create_name           VARCHAR(100),
    update_by             VARCHAR(100),
    update_name           VARCHAR(100),
    plugin_type           VARCHAR(50),
    source_path           VARCHAR(500),
    marketplace_source_id VARCHAR(64),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_plugin_versions IS '插件版本历史表：记录插件的版本历史';
COMMENT ON COLUMN cmx_plugin_versions.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_versions.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_versions.app_id IS '应用隔离标识，用于多租户或多应用场景下的版本隔离';
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
COMMENT ON COLUMN cmx_plugin_versions.marketplace_source_id IS '市场版本来源ID，关联 cmx_marketplace_plugin_version.id';

CREATE INDEX idx_version_plugin ON cmx_plugin_versions (plugin_id);
CREATE INDEX idx_plugin_versions_app_id ON cmx_plugin_versions (app_id);
ALTER TABLE cmx_plugin_versions
    ADD CONSTRAINT uk_cmx_plugin_versions_plugin_version UNIQUE (plugin_id, app_id, version);

-- =============================================
-- 7. 依赖关系表 (cmx_plugin_dependencies)
-- =============================================
-- DROP TABLE IF EXISTS cmx_plugin_dependencies;
-- CREATE TABLE cmx_plugin_dependencies
-- (
--     id                   VARCHAR(64)              NOT NULL,
--     plugin_id            VARCHAR(64)              NOT NULL,
--     app_id               VARCHAR(64)              NOT NULL DEFAULT 'default',
--     dependency_plugin_id VARCHAR(255)             NOT NULL,
--     dependency_name      VARCHAR(500),
--     version_constraint   VARCHAR(100),
--     min_version          VARCHAR(50),
--     max_version          VARCHAR(50),
--     is_optional          BOOLEAN                  NOT NULL DEFAULT FALSE,
--     is_dev               BOOLEAN                  NOT NULL DEFAULT FALSE,
--     resolved_version     VARCHAR(50),
--     resolution_status    VARCHAR(30),
--     create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     update_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     archived             INT4                     NOT NULL DEFAULT 0,
--     create_by            VARCHAR(100),
--     create_name          VARCHAR(100),
--     update_by            VARCHAR(100),
--     update_name          VARCHAR(100),
--     PRIMARY KEY (id)
-- );
--
-- COMMENT ON TABLE cmx_plugin_dependencies IS '插件依赖关系表：记录插件之间的依赖关系';
-- COMMENT ON COLUMN cmx_plugin_dependencies.id IS '主键ID';
-- COMMENT ON COLUMN cmx_plugin_dependencies.plugin_id IS '关联插件ID';
-- COMMENT ON COLUMN cmx_plugin_dependencies.app_id IS '应用隔离标识，用于多租户或多应用场景下的依赖隔离';
-- COMMENT ON COLUMN cmx_plugin_dependencies.dependency_plugin_id IS '依赖的插件ID (可能是未安装的)';
-- COMMENT ON COLUMN cmx_plugin_dependencies.dependency_name IS '依赖的插件名称';
-- COMMENT ON COLUMN cmx_plugin_dependencies.version_constraint IS '版本约束 (如 "^1.0.0", "~2.1.0", ">=1.0.0 <3.0.0")';
-- COMMENT ON COLUMN cmx_plugin_dependencies.min_version IS '最小版本';
-- COMMENT ON COLUMN cmx_plugin_dependencies.max_version IS '最大版本';
-- COMMENT ON COLUMN cmx_plugin_dependencies.is_optional IS '是否可选依赖';
-- COMMENT ON COLUMN cmx_plugin_dependencies.is_dev IS '是否开发依赖';
-- COMMENT ON COLUMN cmx_plugin_dependencies.resolved_version IS '已解析的版本';
-- COMMENT ON COLUMN cmx_plugin_dependencies.resolution_status IS '解析状态: resolved, conflict, missing';
-- COMMENT ON COLUMN cmx_plugin_dependencies.create_time IS '创建时间';
-- COMMENT ON COLUMN cmx_plugin_dependencies.update_time IS '更新时间';
-- COMMENT ON COLUMN cmx_plugin_dependencies.create_by IS '创建人ID';
-- COMMENT ON COLUMN cmx_plugin_dependencies.create_name IS '创建人名称';
-- COMMENT ON COLUMN cmx_plugin_dependencies.update_by IS '更新人ID';
-- COMMENT ON COLUMN cmx_plugin_dependencies.update_name IS '更新人名称';
--
-- CREATE INDEX idx_dep_plugin ON cmx_plugin_dependencies (plugin_id);
-- CREATE INDEX idx_dep_resolved ON cmx_plugin_dependencies (plugin_id, resolution_status);
-- CREATE INDEX idx_dep_app_id ON cmx_plugin_dependencies (app_id);
--
-- -- =============================================
-- -- 8. 节点部署记录表 (cmx_plugin_deployments)
-- -- =============================================
-- DROP TABLE IF EXISTS cmx_plugin_deployments;
-- CREATE TABLE cmx_plugin_deployments
-- (
--     id            VARCHAR(64)              NOT NULL,
--     plugin_id     VARCHAR(64)              NOT NULL,
--     app_id        VARCHAR(64)              NOT NULL DEFAULT 'default',
--     node_id       VARCHAR(100)             NOT NULL,
--     node_type     VARCHAR(50),
--     version       VARCHAR(50)              NOT NULL,
--     status        VARCHAR(30),
--     progress      INTEGER                           DEFAULT 0,
--     error_message TEXT,
--     error_details TEXT,
--     create_time   TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     update_time   TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     archived      INT4                     NOT NULL DEFAULT 0,
--     create_by     VARCHAR(100),
--     create_name   VARCHAR(100),
--     update_by     VARCHAR(100),
--     update_name   VARCHAR(100),
--     plugin_type   VARCHAR(50),
--     source_path   TEXT,
--     PRIMARY KEY (id)
-- );
--
-- COMMENT ON TABLE cmx_plugin_deployments IS '节点插件部署记录表：记录插件在各个节点上的部署状态';
-- COMMENT ON COLUMN cmx_plugin_deployments.id IS '主键ID';
-- COMMENT ON COLUMN cmx_plugin_deployments.plugin_id IS '关联插件ID';
-- COMMENT ON COLUMN cmx_plugin_deployments.app_id IS '应用隔离标识，用于多租户或多应用场景下的部署隔离';
-- COMMENT ON COLUMN cmx_plugin_deployments.node_id IS '节点标识';
-- COMMENT ON COLUMN cmx_plugin_deployments.node_type IS '节点类型: primary, replica, worker';
-- COMMENT ON COLUMN cmx_plugin_deployments.version IS '部署的版本';
-- COMMENT ON COLUMN cmx_plugin_deployments.status IS '部署状态';
-- COMMENT ON COLUMN cmx_plugin_deployments.progress IS '进度 (0-100)';
-- COMMENT ON COLUMN cmx_plugin_deployments.error_message IS '错误信息';
-- COMMENT ON COLUMN cmx_plugin_deployments.error_details IS '错误详情';
-- COMMENT ON COLUMN cmx_plugin_deployments.create_time IS '创建时间';
-- COMMENT ON COLUMN cmx_plugin_deployments.update_time IS '更新时间';
-- COMMENT ON COLUMN cmx_plugin_deployments.archived IS '归档标志：0-未归档，1-已归档';
-- COMMENT ON COLUMN cmx_plugin_deployments.create_by IS '创建人ID';
-- COMMENT ON COLUMN cmx_plugin_deployments.create_name IS '创建人名称';
-- COMMENT ON COLUMN cmx_plugin_deployments.update_by IS '更新人ID';
-- COMMENT ON COLUMN cmx_plugin_deployments.update_name IS '更新人名称';
-- COMMENT ON COLUMN cmx_plugin_deployments.plugin_type IS '插件类型: wasm/rhai';
-- COMMENT ON COLUMN cmx_plugin_deployments.source_path IS '源码路径';
--
-- CREATE INDEX idx_deploy_plugin ON cmx_plugin_deployments (plugin_id);
-- CREATE INDEX idx_deploy_node ON cmx_plugin_deployments (node_id);
-- CREATE INDEX idx_deploy_status ON cmx_plugin_deployments (status);
-- CREATE INDEX idx_deploy_app_id ON cmx_plugin_deployments (app_id);
-- ALTER TABLE cmx_plugin_deployments
--     ADD CONSTRAINT uk_cmx_plugin_deployments_plugin_node_version UNIQUE (plugin_id, node_id, version);

-- =============================================
-- 9. 审计日志表 (cmx_plugin_audit_log)
-- =============================================
DROP TABLE IF EXISTS cmx_plugin_audit_log;
CREATE TABLE cmx_plugin_audit_log
(
    id               VARCHAR(64)              NOT NULL,
    app_id VARCHAR(64) NOT NULL DEFAULT 'default',
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

COMMENT ON TABLE cmx_plugin_audit_log IS '审计日志表：记录插件操作日志';
COMMENT ON COLUMN cmx_plugin_audit_log.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_audit_log.app_id IS '应用隔离标识，用于多租户或多应用场景下的审计日志隔离';
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

CREATE INDEX idx_audit_plugin ON cmx_plugin_audit_log (plugin_id);
CREATE INDEX idx_audit_node ON cmx_plugin_audit_log (node_id);
CREATE INDEX idx_audit_operation ON cmx_plugin_audit_log (operation_type);
CREATE INDEX idx_audit_timestamp ON cmx_plugin_audit_log (started_at);
CREATE INDEX idx_audit_request ON cmx_plugin_audit_log (request_id);
CREATE INDEX idx_audit_app_id ON cmx_plugin_audit_log (app_id);

-- -- =============================================
-- -- 10. 系统默认插件配置表 (cmx_system_plugins)
-- -- =============================================
-- DROP TABLE IF EXISTS cmx_system_plugins;
-- CREATE TABLE cmx_system_plugins
-- (
--     id                  VARCHAR(64)              NOT NULL,
--     plugin_id           VARCHAR(255)             NOT NULL,
--     app_id              VARCHAR(64)              NOT NULL DEFAULT 'default',
--     name                VARCHAR(500)             NOT NULL,
--     version             VARCHAR(50)              NOT NULL,
--     install_order       INTEGER                  NOT NULL DEFAULT 0,
--     is_optional         BOOLEAN                  NOT NULL DEFAULT FALSE,
--     is_critical         BOOLEAN                  NOT NULL DEFAULT FALSE,
--     retry_count         INTEGER                  NOT NULL DEFAULT 3,
--     retry_delay_seconds INTEGER                  NOT NULL DEFAULT 10,
--     wait_for_plugins    VARCHAR(255),
--     source_type         VARCHAR(30)              NOT NULL,
--     source_path         TEXT,
--     source_url          TEXT,
--     status              VARCHAR(30)              NOT NULL DEFAULT 'pending',
--     last_installed_at   TIMESTAMP WITH TIME ZONE,
--     install_attempts    INTEGER                  NOT NULL DEFAULT 0,
--     create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     archived            INT4                     NOT NULL DEFAULT 0,
--     create_by           VARCHAR(100),
--     create_name         VARCHAR(100),
--     update_by           VARCHAR(100),
--     update_name         VARCHAR(100)
-- );
--
-- COMMENT ON TABLE cmx_system_plugins IS '系统默认插件配置表：配置系统启动时需要自动安装的插件';
-- COMMENT ON COLUMN cmx_system_plugins.id IS '主键ID';
-- COMMENT ON COLUMN cmx_system_plugins.plugin_id IS '插件唯一标识';
-- COMMENT ON COLUMN cmx_system_plugins.app_id IS '应用隔离标识，用于多租户或多应用场景下的系统插件隔离';
-- COMMENT ON COLUMN cmx_system_plugins.name IS '插件名称';
-- COMMENT ON COLUMN cmx_system_plugins.version IS '插件版本';
-- COMMENT ON COLUMN cmx_system_plugins.install_order IS '安装顺序 (数字越小越先安装)';
-- COMMENT ON COLUMN cmx_system_plugins.is_optional IS '是否可选 (可选则安装失败不阻止启动)';
-- COMMENT ON COLUMN cmx_system_plugins.is_critical IS '是否关键 (关键插件失败导致系统无法启动)';
-- COMMENT ON COLUMN cmx_system_plugins.retry_count IS '重试次数';
-- COMMENT ON COLUMN cmx_system_plugins.retry_delay_seconds IS '重试间隔 (秒)';
-- COMMENT ON COLUMN cmx_system_plugins.wait_for_plugins IS '需要等待完成的插件列表';
-- COMMENT ON COLUMN cmx_system_plugins.source_type IS '来源类型: bundled, url';
-- COMMENT ON COLUMN cmx_system_plugins.source_path IS '来源路径 (bundled 时为内置路径)';
-- COMMENT ON COLUMN cmx_system_plugins.source_url IS '来源 URL (url 类型时使用)';
-- COMMENT ON COLUMN cmx_system_plugins.status IS '状态';
-- COMMENT ON COLUMN cmx_system_plugins.create_time IS '创建时间';
-- COMMENT ON COLUMN cmx_system_plugins.update_time IS '更新时间';
-- COMMENT ON COLUMN cmx_system_plugins.archived IS '归档标志：0-未归档，1-已归档';
-- COMMENT ON COLUMN cmx_system_plugins.create_by IS '创建人ID';
-- COMMENT ON COLUMN cmx_system_plugins.create_name IS '创建人名称';
-- COMMENT ON COLUMN cmx_system_plugins.update_by IS '更新人ID';
-- COMMENT ON COLUMN cmx_system_plugins.update_name IS '更新人名称';
--
-- CREATE INDEX idx_system_plugin_order ON cmx_system_plugins (install_order);
-- CREATE INDEX idx_system_plugin_status ON cmx_system_plugins (status);
-- CREATE INDEX idx_system_plugins_app_id ON cmx_system_plugins (app_id);
--
-- -- =============================================
-- -- 11. 节点信息表 (cmx_plugin_nodes)
-- -- =============================================
-- DROP TABLE IF EXISTS cmx_plugin_nodes;
-- CREATE TABLE cmx_plugin_nodes
-- (
--     node_id                VARCHAR(64)              NOT NULL,
--     app_id                 VARCHAR(64)              NOT NULL DEFAULT 'default',
--     node_name              VARCHAR(255)             NOT NULL,
--     node_type              VARCHAR(30)              NOT NULL,
--     status                 VARCHAR(30)              NOT NULL,
--     is_active              BOOLEAN                  NOT NULL DEFAULT TRUE,
--     host                   VARCHAR(255)             NOT NULL,
--     port                   INTEGER                  NOT NULL,
--     protocol               VARCHAR(10)              NOT NULL DEFAULT 'http',
--     capabilities           TEXT,
--     last_health_check      TIMESTAMP WITH TIME ZONE,
--     health_check_interval  INTEGER                  NOT NULL DEFAULT 30,
--     plugin_manager_version VARCHAR(50),
--     registered_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     last_seen_at           TIMESTAMP WITH TIME ZONE,
--     create_time            TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     update_time            TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     archived               INT4                     NOT NULL DEFAULT 0,
--     create_by              VARCHAR(100),
--     create_name            VARCHAR(100),
--     update_by              VARCHAR(100),
--     update_name            VARCHAR(100)
-- );
--
-- COMMENT ON TABLE cmx_plugin_nodes IS '节点信息表：记录集群中的节点信息';
-- COMMENT ON COLUMN cmx_plugin_nodes.node_id IS '节点ID';
-- COMMENT ON COLUMN cmx_plugin_nodes.app_id IS '应用隔离标识，用于多租户或多应用场景下的节点隔离';
-- COMMENT ON COLUMN cmx_plugin_nodes.node_name IS '节点名称';
-- COMMENT ON COLUMN cmx_plugin_nodes.node_type IS '节点类型: primary, replica, worker';
-- COMMENT ON COLUMN cmx_plugin_nodes.status IS '节点状态: online, offline, maintenance';
-- COMMENT ON COLUMN cmx_plugin_nodes.is_active IS '是否激活';
-- COMMENT ON COLUMN cmx_plugin_nodes.host IS '主机地址';
-- COMMENT ON COLUMN cmx_plugin_nodes.port IS '端口';
-- COMMENT ON COLUMN cmx_plugin_nodes.protocol IS '协议';
-- COMMENT ON COLUMN cmx_plugin_nodes.capabilities IS '节点能力';
-- COMMENT ON COLUMN cmx_plugin_nodes.last_health_check IS '最后健康检查时间';
-- COMMENT ON COLUMN cmx_plugin_nodes.health_check_interval IS '健康检查间隔 (秒)';
-- COMMENT ON COLUMN cmx_plugin_nodes.plugin_manager_version IS '插件管理器版本';
-- COMMENT ON COLUMN cmx_plugin_nodes.registered_at IS '注册时间';
-- COMMENT ON COLUMN cmx_plugin_nodes.last_seen_at IS '最后可见时间';
-- COMMENT ON COLUMN cmx_plugin_nodes.create_time IS '创建时间';
-- COMMENT ON COLUMN cmx_plugin_nodes.update_time IS '更新时间';
-- COMMENT ON COLUMN cmx_plugin_nodes.archived IS '归档标志：0-未归档，1-已归档';
-- COMMENT ON COLUMN cmx_plugin_nodes.create_by IS '创建人ID';
-- COMMENT ON COLUMN cmx_plugin_nodes.create_name IS '创建人名称';
-- COMMENT ON COLUMN cmx_plugin_nodes.update_by IS '更新人ID';
-- COMMENT ON COLUMN cmx_plugin_nodes.update_name IS '更新人名称';
--
-- CREATE INDEX idx_node_status ON cmx_plugin_nodes (status);
-- CREATE INDEX idx_node_type ON cmx_plugin_nodes (node_type);
-- CREATE INDEX idx_node_app_id ON cmx_plugin_nodes (app_id);
--
-- -- =============================================
-- -- 12. 插件功能表 (cmx_plugin_features)
-- -- =============================================
-- DROP TABLE IF EXISTS cmx_plugin_features;
-- CREATE TABLE cmx_plugin_features
-- (
--     id             VARCHAR(64)              NOT NULL,
--     plugin_id      VARCHAR(64)              NOT NULL,
--     app_id         VARCHAR(64)              NOT NULL DEFAULT 'default',
--     plugin_version VARCHAR(50)              NOT NULL,
--     feature_id     VARCHAR(255)             NOT NULL,
--     feature_name   VARCHAR(500)             NOT NULL,
--     feature_type   VARCHAR(50)              NOT NULL,
--     description    TEXT,
--     config         JSONB,
--     status         VARCHAR(30)              NOT NULL DEFAULT 'active',
--     create_time    TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     update_time    TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
--     archived       INT4                     NOT NULL DEFAULT 0,
--     create_by      VARCHAR(100),
--     create_name    VARCHAR(100),
--     update_by      VARCHAR(100),
--     update_name    VARCHAR(100)
-- );
--
-- COMMENT ON TABLE cmx_plugin_features IS '插件功能表';
-- COMMENT ON COLUMN cmx_plugin_features.id IS '主键ID';
-- COMMENT ON COLUMN cmx_plugin_features.plugin_id IS '关联插件ID';
-- COMMENT ON COLUMN cmx_plugin_features.app_id IS '应用隔离标识，用于多租户或多应用场景下的功能隔离';
-- COMMENT ON COLUMN cmx_plugin_features.plugin_version IS '插件版本';
-- COMMENT ON COLUMN cmx_plugin_features.feature_id IS '功能唯一标识';
-- COMMENT ON COLUMN cmx_plugin_features.feature_name IS '功能名称';
-- COMMENT ON COLUMN cmx_plugin_features.feature_type IS '功能类型: service, event_handler, scheduler, api,function';
-- COMMENT ON COLUMN cmx_plugin_features.description IS '功能描述';
-- COMMENT ON COLUMN cmx_plugin_features.config IS '功能配置';
-- COMMENT ON COLUMN cmx_plugin_features.status IS '状态: active, inactive, error';
-- COMMENT ON COLUMN cmx_plugin_features.create_time IS '创建时间';
-- COMMENT ON COLUMN cmx_plugin_features.update_time IS '更新时间';
-- COMMENT ON COLUMN cmx_plugin_features.archived IS '归档标志：0-未归档，1-已归档';
-- COMMENT ON COLUMN cmx_plugin_features.create_by IS '创建人ID';
-- COMMENT ON COLUMN cmx_plugin_features.create_name IS '创建人名称';
-- COMMENT ON COLUMN cmx_plugin_features.update_by IS '更新人ID';
-- COMMENT ON COLUMN cmx_plugin_features.update_name IS '更新人名称';
--
-- CREATE INDEX idx_features_app_id ON cmx_plugin_features (app_id);

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
    app_id     VARCHAR(64) NOT NULL DEFAULT 'default',
    ddl_status VARCHAR(20) NOT NULL DEFAULT 'pending',
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

COMMENT ON TABLE cmx_meta_table_define IS '表定义元数据';
COMMENT ON COLUMN cmx_meta_table_define.id IS '主键';
COMMENT ON COLUMN cmx_meta_table_define.table_name IS '表名';
COMMENT ON COLUMN cmx_meta_table_define.display_name IS '显示名称';
COMMENT ON COLUMN cmx_meta_table_define.db_id IS '所属数据库id';
COMMENT ON COLUMN cmx_meta_table_define.plugin_id IS '插件id';
COMMENT ON COLUMN cmx_meta_table_define.version IS '当前使用的元数据插件版本';
COMMENT ON COLUMN cmx_meta_table_define.app_id IS '应用隔离标识，用于多租户或多应用场景下的元数据隔离';
COMMENT ON COLUMN cmx_meta_table_define.ddl_status IS 'DDL执行状态: pending(待执行), executing(执行中), completed(已完成), failed(执行失败)';
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

CREATE INDEX idx_meta_table_define_app_id ON cmx_meta_table_define (app_id);

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
    app_id VARCHAR(64) NOT NULL DEFAULT 'default',
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

COMMENT ON TABLE cmx_meta_table_define_version IS '表元数据版本表';
COMMENT ON COLUMN cmx_meta_table_define_version.id IS '主键';
COMMENT ON COLUMN cmx_meta_table_define_version.table_name IS '表名';
COMMENT ON COLUMN cmx_meta_table_define_version.display_name IS '显示名称';
COMMENT ON COLUMN cmx_meta_table_define_version.db_id IS '所属数据库id';
COMMENT ON COLUMN cmx_meta_table_define_version.plugin_id IS '插件id';
COMMENT ON COLUMN cmx_meta_table_define_version.version IS '插件版本';
COMMENT ON COLUMN cmx_meta_table_define_version.app_id IS '应用隔离标识，用于多租户或多应用场景下的元数据版本隔离';
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

CREATE INDEX idx_meta_table_define_version_app_id ON cmx_meta_table_define_version (app_id);

-- =============================================
-- 15. 服务定义表 (cmx_service_define)
-- =============================================
DROP TABLE IF EXISTS cmx_service_define;
CREATE TABLE cmx_service_define
(
    id               VARCHAR(64)  NOT NULL,
    service_key      VARCHAR(100) NOT NULL,
    app_id VARCHAR(64) NOT NULL DEFAULT 'default',
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

COMMENT ON TABLE cmx_service_define IS '服务定义表';
COMMENT ON COLUMN cmx_service_define.id IS '主键';
COMMENT ON COLUMN cmx_service_define.service_key IS '服务key';
COMMENT ON COLUMN cmx_service_define.app_id IS '应用隔离标识，用于多租户或多应用场景下的服务隔离';
COMMENT ON COLUMN cmx_service_define.service_name IS '服务名称';
COMMENT ON COLUMN cmx_service_define.description IS '服务描述';
COMMENT ON COLUMN cmx_service_define.plugin_id IS '所属插件id';
COMMENT ON COLUMN cmx_service_define.domain_code IS '所属域';
COMMENT ON COLUMN cmx_service_define.application_code IS '所属应用';
COMMENT ON COLUMN cmx_service_define.module_code IS '所属模块';
COMMENT ON COLUMN cmx_service_define.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_service_define.version IS '服务版本';
COMMENT ON COLUMN cmx_service_define.create_time IS '创建时间';
COMMENT ON COLUMN cmx_service_define.update_time IS '更新时间';
COMMENT ON COLUMN cmx_service_define.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_service_define.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_service_define.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_service_define.update_name IS '更新人名称';
COMMENT ON COLUMN cmx_service_define.archived IS '归档标志：0-未归档，1-已归档';

CREATE UNIQUE INDEX uk_cmx_service_define_app_key ON cmx_service_define (app_id, service_key);
CREATE INDEX idx_service_define_app_id ON cmx_service_define (app_id);

-- =============================================
-- 16. 服务定义版本表 (cmx_service_define_version)
-- =============================================
DROP TABLE IF EXISTS cmx_service_define_version;
CREATE TABLE cmx_service_define_version
(
    id             VARCHAR(64) NOT NULL,
    service_key    VARCHAR(100),
    app_id VARCHAR(64) NOT NULL DEFAULT 'default',
    version        VARCHAR(50),
    plugin_id      VARCHAR(64),
    plugin_version VARCHAR(50),
    config         TEXT,
    api_doc        TEXT,
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by      VARCHAR(100),
    create_name    VARCHAR(100),
    update_by      VARCHAR(100),
    update_name    VARCHAR(100),
    archived       INT4      DEFAULT 0,
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_service_define_version IS '服务定义版本表';
COMMENT ON COLUMN cmx_service_define_version.id IS '主键';
COMMENT ON COLUMN cmx_service_define_version.service_key IS '服务key';
COMMENT ON COLUMN cmx_service_define_version.app_id IS '应用隔离标识，用于多租户或多应用场景下的服务版本隔离';
COMMENT ON COLUMN cmx_service_define_version.version IS '服务版本';
COMMENT ON COLUMN cmx_service_define_version.plugin_id IS '服务所属插件';
COMMENT ON COLUMN cmx_service_define_version.plugin_version IS '所属插件版本';
COMMENT ON COLUMN cmx_service_define_version.config IS '服务编排配置';
COMMENT ON COLUMN cmx_service_define_version.api_doc IS '服务接口文档JSON，由api_doc_generator自动生成';
COMMENT ON COLUMN cmx_service_define_version.create_time IS '创建时间';
COMMENT ON COLUMN cmx_service_define_version.update_time IS '更新时间';
COMMENT ON COLUMN cmx_service_define_version.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_service_define_version.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_service_define_version.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_service_define_version.update_name IS '更新人名称';
COMMENT ON COLUMN cmx_service_define_version.archived IS '归档标志：0-未归档，1-已归档';

CREATE INDEX cmx_service_define_version_service_key_index ON cmx_service_define_version (service_key);
CREATE INDEX idx_service_define_version_app_id ON cmx_service_define_version (app_id);

-- =============================================
-- 17. 插件市场 - 插件主表 (cmx_marketplace_plugin)
-- =============================================
DROP TABLE IF EXISTS cmx_marketplace_plugin;
CREATE TABLE cmx_marketplace_plugin
(
    id                VARCHAR(64)  NOT NULL,
    plugin_id         VARCHAR(128) NOT NULL,
    name              VARCHAR(256),
    description       TEXT,
    short_description VARCHAR(512),
    icon_url          VARCHAR(512),
    category          VARCHAR(64),
    tags              JSONB,
    vendor_name       VARCHAR(128),
    vendor_url        VARCHAR(512),
    vendor_contact    VARCHAR(256),
    license_type      VARCHAR(32),
    homepage_url      VARCHAR(512),
    documentation_url VARCHAR(512),
    repository_url    VARCHAR(512),
    status            VARCHAR(32),
    is_featured       INT2,
    is_official       INT2,
    avg_rating        DECIMAL(3, 2),
    rating_count      INT4,
    download_count    INT8,
    install_count     INT8,
    domain_code       VARCHAR(64),
    application_code  VARCHAR(64),
    module_code       VARCHAR(64),
    plugin_type       VARCHAR(32),
    archived          INT4      DEFAULT 0,
    create_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by         VARCHAR(100),
    create_name       VARCHAR(100),
    update_by         VARCHAR(100),
    update_name       VARCHAR(100),
    PRIMARY KEY (id),
    UNIQUE (plugin_id)
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

CREATE INDEX idx_mp_category ON cmx_marketplace_plugin (category);
CREATE INDEX idx_mp_status ON cmx_marketplace_plugin (status);
CREATE INDEX idx_mp_featured ON cmx_marketplace_plugin (is_featured) WHERE is_featured = 1;
CREATE INDEX idx_mp_download_count ON cmx_marketplace_plugin (download_count DESC);
CREATE INDEX idx_mp_rating ON cmx_marketplace_plugin (avg_rating DESC);

-- =============================================
-- 18. 插件市场 - 版本表 (cmx_marketplace_plugin_version)
-- =============================================
DROP TABLE IF EXISTS cmx_marketplace_plugin_version;
CREATE TABLE cmx_marketplace_plugin_version
(
    id                   VARCHAR(64)  NOT NULL,
    plugin_id            VARCHAR(128) NOT NULL,
    version              VARCHAR(64)  NOT NULL,
    version_rank         INT4,
    changelog            TEXT,
    release_notes        TEXT,
    download_url         VARCHAR(512),
    package_size         INT8,
    checksum             VARCHAR(128),
    min_platform_version VARCHAR(32),
    max_platform_version VARCHAR(32),
    dependencies         JSONB,
    compatibility        JSONB,
    status               VARCHAR(32),
    is_latest            INT2,
    is_stable            INT2,
    download_count       INT8,
    published_at         TIMESTAMP,
    storage_file_id      VARCHAR(64),
    archived             INT4      DEFAULT 0,
    create_time          TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time          TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by            VARCHAR(100),
    create_name          VARCHAR(100),
    update_by            VARCHAR(100),
    update_name          VARCHAR(100),
    PRIMARY KEY (id),
    UNIQUE (plugin_id, version),
    CONSTRAINT fk_mpversion_plugin FOREIGN KEY (plugin_id) REFERENCES cmx_marketplace_plugin (plugin_id) ON DELETE CASCADE
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
COMMENT ON COLUMN cmx_marketplace_plugin_version.storage_file_id IS 'cmx-storage 文件唯一标识，关联 cmx_file_detail.id';
COMMENT ON COLUMN cmx_marketplace_plugin_version.archived IS '归档标记（0未归档/1已归档）';
COMMENT ON COLUMN cmx_marketplace_plugin_version.create_time IS '创建时间';
COMMENT ON COLUMN cmx_marketplace_plugin_version.update_time IS '更新时间';
COMMENT ON COLUMN cmx_marketplace_plugin_version.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_marketplace_plugin_version.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_marketplace_plugin_version.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_marketplace_plugin_version.update_name IS '更新人姓名';

CREATE INDEX idx_mpv_plugin_id ON cmx_marketplace_plugin_version (plugin_id);
CREATE INDEX idx_mpv_latest ON cmx_marketplace_plugin_version (plugin_id, is_latest) WHERE is_latest = 1;

-- =============================================
-- 19. 插件市场 - 下载统计表 (cmx_marketplace_download_stats)
-- =============================================
DROP TABLE IF EXISTS cmx_marketplace_download_stats;
CREATE TABLE cmx_marketplace_download_stats
(
    id             VARCHAR(64)  NOT NULL,
    plugin_id      VARCHAR(128) NOT NULL,
    version        VARCHAR(64),
    download_date  DATE,
    download_count INT4,
    install_count  INT4,
    source_type    VARCHAR(32),
    region         VARCHAR(32),
    archived       INT4      DEFAULT 0,
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by      VARCHAR(100),
    create_name    VARCHAR(100),
    update_by      VARCHAR(100),
    update_name    VARCHAR(100),
    PRIMARY KEY (id),
    UNIQUE (plugin_id, version, download_date, source_type)
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

CREATE INDEX idx_dstats_date ON cmx_marketplace_download_stats (download_date);

-- =============================================
-- 20. 插件市场 - 评分表 (cmx_marketplace_rating)
-- =============================================
DROP TABLE IF EXISTS cmx_marketplace_rating;
CREATE TABLE cmx_marketplace_rating
(
    id          VARCHAR(64)  NOT NULL,
    plugin_id   VARCHAR(128) NOT NULL,
    user_id     VARCHAR(128) NOT NULL,
    rating      INT4,
    review      TEXT,
    status      VARCHAR(32),
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by   VARCHAR(100),
    create_name VARCHAR(100),
    update_by   VARCHAR(100),
    update_name VARCHAR(100),
    PRIMARY KEY (id),
    UNIQUE (plugin_id, user_id),
    CONSTRAINT fk_rating_plugin FOREIGN KEY (plugin_id) REFERENCES cmx_marketplace_plugin (plugin_id) ON DELETE CASCADE
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

CREATE INDEX idx_rating_plugin ON cmx_marketplace_rating (plugin_id);

-- =============================================
-- 21. 文件详情表 (cmx_file_detail)
-- =============================================
DROP TABLE IF EXISTS cmx_file_detail;
CREATE TABLE cmx_file_detail
(
    id                VARCHAR(64)  NOT NULL,
    url               VARCHAR(512) NOT NULL,
    size              BIGINT,
    filename          VARCHAR(256),
    original_filename VARCHAR(256),
    base_path         VARCHAR(256),
    path              VARCHAR(256),
    ext               VARCHAR(32),
    content_type      VARCHAR(128),
    platform          VARCHAR(32),
    th_url            VARCHAR(512),
    th_filename       VARCHAR(256),
    th_size           BIGINT,
    th_content_type   VARCHAR(128),
    object_id         VARCHAR(64),
    object_type       VARCHAR(32),
    metadata          TEXT,
    user_metadata     TEXT,
    th_metadata       TEXT,
    th_user_metadata  TEXT,
    attr              TEXT,
    file_acl          VARCHAR(32),
    th_file_acl       VARCHAR(32),
    hash_info         TEXT,
    upload_id         VARCHAR(128),
    upload_status     INTEGER,
    archived          INTEGER   DEFAULT 0,
    create_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by         VARCHAR(100),
    create_name       VARCHAR(100),
    update_by         VARCHAR(100),
    update_name       VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_file_detail IS '文件详情表';
COMMENT ON COLUMN cmx_file_detail.id IS '主键ID';
COMMENT ON COLUMN cmx_file_detail.url IS '文件访问地址';
COMMENT ON COLUMN cmx_file_detail.size IS '文件大小，单位字节';
COMMENT ON COLUMN cmx_file_detail.filename IS '文件名称';
COMMENT ON COLUMN cmx_file_detail.original_filename IS '原始文件名';
COMMENT ON COLUMN cmx_file_detail.base_path IS '基础存储路径';
COMMENT ON COLUMN cmx_file_detail.path IS '存储路径';
COMMENT ON COLUMN cmx_file_detail.ext IS '文件扩展名';
COMMENT ON COLUMN cmx_file_detail.content_type IS 'MIME类型';
COMMENT ON COLUMN cmx_file_detail.platform IS '存储平台标识';
COMMENT ON COLUMN cmx_file_detail.th_url IS '缩略图访问路径';
COMMENT ON COLUMN cmx_file_detail.th_filename IS '缩略图名称';
COMMENT ON COLUMN cmx_file_detail.th_size IS '缩略图大小，单位字节';
COMMENT ON COLUMN cmx_file_detail.th_content_type IS '缩略图MIME类型';
COMMENT ON COLUMN cmx_file_detail.object_id IS '文件所属对象ID';
COMMENT ON COLUMN cmx_file_detail.object_type IS '文件所属对象类型';
COMMENT ON COLUMN cmx_file_detail.metadata IS '文件元数据';
COMMENT ON COLUMN cmx_file_detail.user_metadata IS '文件用户元数据';
COMMENT ON COLUMN cmx_file_detail.th_metadata IS '缩略图元数据';
COMMENT ON COLUMN cmx_file_detail.th_user_metadata IS '缩略图用户元数据';
COMMENT ON COLUMN cmx_file_detail.attr IS '附加属性';
COMMENT ON COLUMN cmx_file_detail.file_acl IS '文件ACL';
COMMENT ON COLUMN cmx_file_detail.th_file_acl IS '缩略图文件ACL';
COMMENT ON COLUMN cmx_file_detail.hash_info IS '哈希信息（JSON格式，含MD5等）';
COMMENT ON COLUMN cmx_file_detail.upload_id IS '上传ID，仅在手动分片上传时使用';
COMMENT ON COLUMN cmx_file_detail.upload_status IS '上传状态：0-普通上传，1-初始化完成，2-上传完成';
COMMENT ON COLUMN cmx_file_detail.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_file_detail.create_time IS '创建时间';
COMMENT ON COLUMN cmx_file_detail.update_time IS '更新时间';
COMMENT ON COLUMN cmx_file_detail.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_file_detail.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_file_detail.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_file_detail.update_name IS '更新人姓名';

CREATE INDEX idx_file_detail_platform ON cmx_file_detail (platform);
CREATE INDEX idx_file_detail_object_type ON cmx_file_detail (object_type);
CREATE INDEX idx_file_detail_upload_id ON cmx_file_detail (upload_id);

-- =============================================
-- 22. 文件分片信息表 (cmx_file_part_detail)
-- =============================================
DROP TABLE IF EXISTS cmx_file_part_detail;
CREATE TABLE cmx_file_part_detail
(
    id          VARCHAR(64) NOT NULL,
    platform    VARCHAR(32),
    upload_id   VARCHAR(128),
    e_tag       VARCHAR(255),
    part_number INTEGER,
    part_size   BIGINT,
    hash_info   TEXT,
    archived    INTEGER   DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by   VARCHAR(100),
    create_name VARCHAR(100),
    update_by   VARCHAR(100),
    update_name VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_file_part_detail IS '文件分片信息表，仅在手动分片上传时使用';
COMMENT ON COLUMN cmx_file_part_detail.id IS '主键ID';
COMMENT ON COLUMN cmx_file_part_detail.platform IS '存储平台标识';
COMMENT ON COLUMN cmx_file_part_detail.upload_id IS '上传ID';
COMMENT ON COLUMN cmx_file_part_detail.e_tag IS '分片ETag';
COMMENT ON COLUMN cmx_file_part_detail.part_number IS '分片号';
COMMENT ON COLUMN cmx_file_part_detail.part_size IS '分片大小，单位字节';
COMMENT ON COLUMN cmx_file_part_detail.hash_info IS '哈希信息';
COMMENT ON COLUMN cmx_file_part_detail.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_file_part_detail.create_time IS '创建时间';
COMMENT ON COLUMN cmx_file_part_detail.update_time IS '更新时间';
COMMENT ON COLUMN cmx_file_part_detail.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_file_part_detail.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_file_part_detail.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_file_part_detail.update_name IS '更新人姓名';

CREATE INDEX idx_file_part_detail_upload_id ON cmx_file_part_detail (upload_id);

-- =============================================
-- 23. OAuth2 客户端表 (cmx_auth_client)
-- =============================================
DROP TABLE IF EXISTS cmx_auth_client;
CREATE TABLE cmx_auth_client
(
    id              VARCHAR(64)  NOT NULL,
    client_id       VARCHAR(100) NOT NULL,
    client_name     VARCHAR(200) NOT NULL,
    client_secret   VARCHAR(500),
    client_type     VARCHAR(20)  NOT NULL,
    redirect_uris   TEXT         NOT NULL,
    grant_types     VARCHAR(200) NOT NULL,
    allowed_scopes  TEXT,
    pkce_required   BOOLEAN   DEFAULT TRUE,
    status          INT4      DEFAULT 1,
    description     VARCHAR(500),
    archived        INT4      DEFAULT 0,
    create_time     TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time     TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by       VARCHAR(100),
    create_name     VARCHAR(100),
    update_by       VARCHAR(100),
    update_name     VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_client IS 'OAuth2 客户端表';
COMMENT ON COLUMN cmx_auth_client.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_client.client_id IS '客户端标识';
COMMENT ON COLUMN cmx_auth_client.client_name IS '客户端名称';
COMMENT ON COLUMN cmx_auth_client.client_secret IS '客户端密钥（哈希存储）';
COMMENT ON COLUMN cmx_auth_client.client_type IS '客户端类型：public/confidential';
COMMENT ON COLUMN cmx_auth_client.redirect_uris IS '回调地址（JSON 数组）';
COMMENT ON COLUMN cmx_auth_client.grant_types IS '允许的授权类型（逗号分隔）';
COMMENT ON COLUMN cmx_auth_client.allowed_scopes IS '允许的 scope（逗号分隔）';
COMMENT ON COLUMN cmx_auth_client.pkce_required IS '是否强制 PKCE';
COMMENT ON COLUMN cmx_auth_client.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_auth_client.description IS '描述';
COMMENT ON COLUMN cmx_auth_client.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_client.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_client.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_client.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_client.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_client.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_client.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_auth_client_client_id ON cmx_auth_client (client_id);

-- =============================================
-- 24. Token 签发记录表 (cmx_auth_token_record)
-- =============================================
DROP TABLE IF EXISTS cmx_auth_token_record;
CREATE TABLE cmx_auth_token_record
(
    id            VARCHAR(64)  NOT NULL,
    jti           VARCHAR(100) NOT NULL,
    user_id       VARCHAR(64)  NOT NULL,
    token_type    VARCHAR(20)  NOT NULL,
    session_id    VARCHAR(100),
    device_type   VARCHAR(50),
    device_id     VARCHAR(200),
    ip_address    VARCHAR(50),
    issued_at     TIMESTAMP    NOT NULL,
    expires_at    TIMESTAMP    NOT NULL,
    revoked_at    TIMESTAMP,
    revoke_reason VARCHAR(200),
    archived      INT4      DEFAULT 0,
    create_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by     VARCHAR(100),
    create_name   VARCHAR(100),
    update_by     VARCHAR(100),
    update_name   VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_token_record IS 'Token 签发记录表（审计追踪）';
COMMENT ON COLUMN cmx_auth_token_record.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_token_record.jti IS 'JWT ID';
COMMENT ON COLUMN cmx_auth_token_record.user_id IS '用户ID';
COMMENT ON COLUMN cmx_auth_token_record.token_type IS 'Token 类型：access/refresh';
COMMENT ON COLUMN cmx_auth_token_record.session_id IS '关联会话ID';
COMMENT ON COLUMN cmx_auth_token_record.device_type IS '设备类型';
COMMENT ON COLUMN cmx_auth_token_record.device_id IS '设备ID';
COMMENT ON COLUMN cmx_auth_token_record.ip_address IS 'IP地址';
COMMENT ON COLUMN cmx_auth_token_record.issued_at IS '签发时间';
COMMENT ON COLUMN cmx_auth_token_record.expires_at IS '过期时间';
COMMENT ON COLUMN cmx_auth_token_record.revoked_at IS '撤销时间';
COMMENT ON COLUMN cmx_auth_token_record.revoke_reason IS '撤销原因';
COMMENT ON COLUMN cmx_auth_token_record.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_token_record.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_token_record.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_token_record.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_token_record.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_token_record.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_token_record.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_auth_token_record_jti ON cmx_auth_token_record (jti);
CREATE INDEX idx_cmx_auth_token_record_user ON cmx_auth_token_record (user_id);
CREATE INDEX idx_cmx_auth_token_record_expires ON cmx_auth_token_record (expires_at);

-- =============================================
-- 25. API Key 表 (cmx_auth_api_key)
-- =============================================
DROP TABLE IF EXISTS cmx_auth_api_key;
CREATE TABLE cmx_auth_api_key
(
    id           VARCHAR(64)  NOT NULL,
    key_prefix   VARCHAR(20)  NOT NULL,
    key_hash     VARCHAR(255) NOT NULL,
    user_id      VARCHAR(64),
    service_name VARCHAR(200),
    scopes       TEXT,
    rate_limit   INT4,
    expires_at   TIMESTAMP,
    status       INT4      DEFAULT 1,
    description  VARCHAR(500),
    archived     INT4      DEFAULT 0,
    create_time  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by    VARCHAR(100),
    create_name  VARCHAR(100),
    update_by    VARCHAR(100),
    update_name  VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_api_key IS 'API Key 表（服务间调用认证）';
COMMENT ON COLUMN cmx_auth_api_key.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_api_key.key_prefix IS 'Key 前缀（展示识别用）';
COMMENT ON COLUMN cmx_auth_api_key.key_hash IS 'SHA256 哈希（明文仅生成时返回一次）';
COMMENT ON COLUMN cmx_auth_api_key.user_id IS '关联用户ID';
COMMENT ON COLUMN cmx_auth_api_key.service_name IS '关联服务名称';
COMMENT ON COLUMN cmx_auth_api_key.scopes IS '允许的 scope（逗号分隔）';
COMMENT ON COLUMN cmx_auth_api_key.rate_limit IS '速率限制（请求/秒）';
COMMENT ON COLUMN cmx_auth_api_key.expires_at IS '过期时间（NULL=永不过期）';
COMMENT ON COLUMN cmx_auth_api_key.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_auth_api_key.description IS '描述';
COMMENT ON COLUMN cmx_auth_api_key.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_api_key.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_api_key.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_api_key.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_api_key.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_api_key.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_api_key.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_auth_api_key_prefix ON cmx_auth_api_key (key_prefix);
CREATE INDEX idx_cmx_auth_api_key_user ON cmx_auth_api_key (user_id);

-- =============================================
-- 26. 密码历史表 (cmx_auth_password_history)
-- =============================================
DROP TABLE IF EXISTS cmx_auth_password_history;
CREATE TABLE cmx_auth_password_history
(
    id            VARCHAR(64)  NOT NULL,
    user_id       VARCHAR(64)  NOT NULL,
    password_hash VARCHAR(500) NOT NULL,
    archived      INT4      DEFAULT 0,
    create_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by     VARCHAR(100),
    create_name   VARCHAR(100),
    update_by     VARCHAR(100),
    update_name   VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_password_history IS '密码历史表（防止密码重复使用）';
COMMENT ON COLUMN cmx_auth_password_history.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_password_history.user_id IS '用户ID';
COMMENT ON COLUMN cmx_auth_password_history.password_hash IS '密码哈希';
COMMENT ON COLUMN cmx_auth_password_history.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_password_history.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_password_history.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_password_history.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_password_history.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_password_history.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_password_history.update_name IS '更新人姓名';

CREATE INDEX idx_cmx_auth_password_history_user ON cmx_auth_password_history (user_id);

-- =============================================
-- 27. JWT 密钥表 (cmx_auth_jwt_key)
-- =============================================
DROP TABLE IF EXISTS cmx_auth_jwt_key;
CREATE TABLE cmx_auth_jwt_key
(
    id           VARCHAR(64)  NOT NULL,
    kid          VARCHAR(100) NOT NULL,
    algorithm    VARCHAR(20)  NOT NULL,
    public_key   TEXT         NOT NULL,
    status       INT4      DEFAULT 1,
    effective_at TIMESTAMP    NOT NULL,
    expired_at   TIMESTAMP,
    archived     INT4      DEFAULT 0,
    create_time  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by    VARCHAR(100),
    create_name  VARCHAR(100),
    update_by    VARCHAR(100),
    update_name  VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_jwt_key IS 'JWT 密钥表（密钥轮换管理）';
COMMENT ON COLUMN cmx_auth_jwt_key.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_jwt_key.kid IS '密钥ID（Key ID，写入 JWT Header）';
COMMENT ON COLUMN cmx_auth_jwt_key.algorithm IS '签名算法：RS256/HS256';
COMMENT ON COLUMN cmx_auth_jwt_key.public_key IS '公钥 PEM';
COMMENT ON COLUMN cmx_auth_jwt_key.status IS '状态：0-已失效，1-生效中，2-宽限期（仅验签）';
COMMENT ON COLUMN cmx_auth_jwt_key.effective_at IS '生效时间';
COMMENT ON COLUMN cmx_auth_jwt_key.expired_at IS '失效时间';
COMMENT ON COLUMN cmx_auth_jwt_key.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_jwt_key.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_jwt_key.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_jwt_key.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_jwt_key.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_jwt_key.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_jwt_key.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_auth_jwt_key_kid ON cmx_auth_jwt_key (kid);

-- =============================================
-- 28. 第三方 OAuth2 账号关联表 (cmx_auth_oauth2_account)
-- =============================================
DROP TABLE IF EXISTS cmx_auth_oauth2_account;
CREATE TABLE cmx_auth_oauth2_account
(
    id                     VARCHAR(64)   NOT NULL,
    user_id                VARCHAR(64)   NOT NULL,
    provider               VARCHAR(50)   NOT NULL,
    provider_user_id       VARCHAR(255)  NOT NULL,
    provider_username      VARCHAR(200),
    provider_email         VARCHAR(255),
    provider_email_verified BOOLEAN,
    provider_display_name  VARCHAR(200),
    provider_avatar_url    VARCHAR(1000),
    last_login_at          TIMESTAMP,
    status                 INT4      DEFAULT 1,
    archived               INT4      DEFAULT 0,
    create_time            TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time            TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by              VARCHAR(100),
    create_name            VARCHAR(100),
    update_by              VARCHAR(100),
    update_name            VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_oauth2_account IS '第三方 OAuth2 账号关联表';
COMMENT ON COLUMN cmx_auth_oauth2_account.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_oauth2_account.user_id IS '本地用户 ID';
COMMENT ON COLUMN cmx_auth_oauth2_account.provider IS 'OAuth2 Provider 标识（google/github 等）';
COMMENT ON COLUMN cmx_auth_oauth2_account.provider_user_id IS 'Provider 侧用户唯一标识';
COMMENT ON COLUMN cmx_auth_oauth2_account.provider_username IS 'Provider 侧用户名';
COMMENT ON COLUMN cmx_auth_oauth2_account.provider_email IS 'Provider 侧邮箱';
COMMENT ON COLUMN cmx_auth_oauth2_account.provider_email_verified IS 'Provider 侧邮箱是否已验证';
COMMENT ON COLUMN cmx_auth_oauth2_account.provider_display_name IS 'Provider 侧显示名';
COMMENT ON COLUMN cmx_auth_oauth2_account.provider_avatar_url IS 'Provider 侧头像 URL';
COMMENT ON COLUMN cmx_auth_oauth2_account.last_login_at IS '最近一次通过此 Provider 登录时间';
COMMENT ON COLUMN cmx_auth_oauth2_account.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_auth_oauth2_account.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_oauth2_account.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_oauth2_account.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_oauth2_account.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_oauth2_account.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_oauth2_account.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_oauth2_account.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_auth_oauth2_account_provider_user ON cmx_auth_oauth2_account (provider, provider_user_id);
CREATE INDEX idx_cmx_auth_oauth2_account_user ON cmx_auth_oauth2_account (user_id);
CREATE INDEX idx_cmx_auth_oauth2_account_provider_email ON cmx_auth_oauth2_account (provider, provider_email);

-- =============================================
-- 29. 用户表 (cmx_user)
-- =============================================
DROP TABLE IF EXISTS cmx_user;
CREATE TABLE cmx_user
(
    id             VARCHAR(64)  NOT NULL,
    username       VARCHAR(100) NOT NULL,
    password_hash  VARCHAR(500),
    nickname       VARCHAR(100),
    email          VARCHAR(200),
    phone          VARCHAR(50),
    avatar         VARCHAR(500),
    org_id         VARCHAR(64),
    status         INT4      DEFAULT 1,
    last_login_at  TIMESTAMP,
    last_login_ip  VARCHAR(50),
    description    VARCHAR(500),
    archived       INT4      DEFAULT 0,
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by      VARCHAR(100),
    create_name    VARCHAR(100),
    update_by      VARCHAR(100),
    update_name    VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_user IS '用户表';
COMMENT ON COLUMN cmx_user.id IS '主键ID';
COMMENT ON COLUMN cmx_user.username IS '用户名（唯一）';
COMMENT ON COLUMN cmx_user.password_hash IS '密码哈希（Argon2）';
COMMENT ON COLUMN cmx_user.nickname IS '昵称';
COMMENT ON COLUMN cmx_user.email IS '邮箱';
COMMENT ON COLUMN cmx_user.phone IS '手机号';
COMMENT ON COLUMN cmx_user.avatar IS '头像URL';
COMMENT ON COLUMN cmx_user.org_id IS '所属组织ID';
COMMENT ON COLUMN cmx_user.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_user.last_login_at IS '最后登录时间';
COMMENT ON COLUMN cmx_user.last_login_ip IS '最后登录IP';
COMMENT ON COLUMN cmx_user.description IS '描述';
COMMENT ON COLUMN cmx_user.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_user.create_time IS '创建时间';
COMMENT ON COLUMN cmx_user.update_time IS '更新时间';
COMMENT ON COLUMN cmx_user.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_user.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_user.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_user.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_user_username ON cmx_user (username);
CREATE UNIQUE INDEX uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL;

-- =============================================
-- 30. 角色表 (cmx_role)
-- =============================================
DROP TABLE IF EXISTS cmx_role;
CREATE TABLE cmx_role
(
    id             VARCHAR(64)  NOT NULL,
    code           VARCHAR(100) NOT NULL,
    name           VARCHAR(100) NOT NULL,
    data_scope     INT4      DEFAULT 1,
    sort_order     INT4      DEFAULT 0,
    description    VARCHAR(500),
    status         INT4      DEFAULT 1,
    archived       INT4      DEFAULT 0,
    parent_role_id VARCHAR(64),
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by      VARCHAR(100),
    create_name    VARCHAR(100),
    update_by      VARCHAR(100),
    update_name    VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_role IS '角色表';
COMMENT ON COLUMN cmx_role.id IS '主键ID';
COMMENT ON COLUMN cmx_role.code IS '角色编码（唯一）';
COMMENT ON COLUMN cmx_role.name IS '角色名称';
COMMENT ON COLUMN cmx_role.data_scope IS '数据权限范围：1-全部，2-自定义，3-本部门，4-本部门及子部门，5-仅本人（预留字段）';
COMMENT ON COLUMN cmx_role.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_role.description IS '描述';
COMMENT ON COLUMN cmx_role.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_role.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_role.parent_role_id IS '父角色ID（NULL表示根角色，不支持角色权限继承，仅用于层级展示）';
COMMENT ON COLUMN cmx_role.create_time IS '创建时间';
COMMENT ON COLUMN cmx_role.update_time IS '更新时间';
COMMENT ON COLUMN cmx_role.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_role.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_role.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_role.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_role_code ON cmx_role (code);

-- =============================================
-- 31. 用户角色关联表 (cmx_user_role)
-- =============================================
DROP TABLE IF EXISTS cmx_user_role;
CREATE TABLE cmx_user_role
(
    id         VARCHAR(64) NOT NULL,
    user_id    VARCHAR(64) NOT NULL,
    role_id    VARCHAR(64) NOT NULL,
    archived   INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by  VARCHAR(100),
    create_name VARCHAR(100),
    update_by  VARCHAR(100),
    update_name VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_user_role IS '用户角色关联表';
COMMENT ON COLUMN cmx_user_role.id IS '主键ID';
COMMENT ON COLUMN cmx_user_role.user_id IS '用户ID';
COMMENT ON COLUMN cmx_user_role.role_id IS '角色ID';
COMMENT ON COLUMN cmx_user_role.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_user_role.create_time IS '创建时间';
COMMENT ON COLUMN cmx_user_role.update_time IS '更新时间';
COMMENT ON COLUMN cmx_user_role.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_user_role.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_user_role.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_user_role.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_user_role ON cmx_user_role (user_id, role_id);
CREATE INDEX idx_cmx_user_role_user ON cmx_user_role (user_id);
CREATE INDEX idx_cmx_user_role_role ON cmx_user_role (role_id);

-- =============================================
-- 32. 权限表 (cmx_permission)
-- =============================================
DROP TABLE IF EXISTS cmx_permission;
CREATE TABLE cmx_permission
(
    id            VARCHAR(64)  NOT NULL,
    code          VARCHAR(200) NOT NULL,
    name          VARCHAR(100) NOT NULL,
    resource_type VARCHAR(20) DEFAULT 'api',
    parent_id     VARCHAR(64),
    sort_order    INT4      DEFAULT 0,
    description   VARCHAR(500),
    status        INT4      DEFAULT 1,
    archived      INT4      DEFAULT 0,
    create_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by     VARCHAR(100),
    create_name   VARCHAR(100),
    update_by     VARCHAR(100),
    update_name   VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_permission IS '权限表';
COMMENT ON COLUMN cmx_permission.id IS '主键ID';
COMMENT ON COLUMN cmx_permission.code IS '权限编码（唯一，如 system:user:list）';
COMMENT ON COLUMN cmx_permission.name IS '权限名称';
COMMENT ON COLUMN cmx_permission.resource_type IS '资源类型：api-接口，menu-菜单，button-按钮';
COMMENT ON COLUMN cmx_permission.parent_id IS '父权限ID（用于权限树结构）';
COMMENT ON COLUMN cmx_permission.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_permission.description IS '描述';
COMMENT ON COLUMN cmx_permission.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_permission.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_permission.create_time IS '创建时间';
COMMENT ON COLUMN cmx_permission.update_time IS '更新时间';
COMMENT ON COLUMN cmx_permission.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_permission.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_permission.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_permission.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_permission_code ON cmx_permission (code);
CREATE INDEX idx_cmx_permission_parent ON cmx_permission (parent_id);

-- =============================================
-- 33. 角色权限关联表 (cmx_role_permission)
-- =============================================
DROP TABLE IF EXISTS cmx_role_permission;
CREATE TABLE cmx_role_permission
(
    id            VARCHAR(64) NOT NULL,
    role_id       VARCHAR(64) NOT NULL,
    permission_id VARCHAR(64) NOT NULL,
    archived      INT4      DEFAULT 0,
    create_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by     VARCHAR(100),
    create_name   VARCHAR(100),
    update_by     VARCHAR(100),
    update_name   VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_role_permission IS '角色权限关联表';
COMMENT ON COLUMN cmx_role_permission.id IS '主键ID';
COMMENT ON COLUMN cmx_role_permission.role_id IS '角色ID';
COMMENT ON COLUMN cmx_role_permission.permission_id IS '权限ID';
COMMENT ON COLUMN cmx_role_permission.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_role_permission.create_time IS '创建时间';
COMMENT ON COLUMN cmx_role_permission.update_time IS '更新时间';
COMMENT ON COLUMN cmx_role_permission.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_role_permission.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_role_permission.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_role_permission.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id);
CREATE INDEX idx_cmx_role_permission_role ON cmx_role_permission (role_id);
CREATE INDEX idx_cmx_role_permission_permission ON cmx_role_permission (permission_id);

-- 34. 用户角色临时授权表 (cmx_user_role_assignment)
DROP TABLE IF EXISTS cmx_user_role_assignment;
CREATE TABLE cmx_user_role_assignment
(
    id              varchar(64) NOT NULL,
    user_id         varchar(64) NOT NULL,
    role_id         varchar(64) NOT NULL,
    effective_from  timestamp NOT NULL,
    effective_until timestamp NOT NULL,
    reason          varchar(500),
    source          varchar(20) DEFAULT 'manual',
    status          int4 DEFAULT 1,
    revoked_by      varchar(100),
    revoked_at      timestamp,
    archived        int4 DEFAULT 0,
    create_time     timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time     timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by       varchar(100),
    create_name     varchar(100),
    update_by       varchar(100),
    update_name     varchar(100),
    CONSTRAINT pk_cmx_user_role_assignment PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_user_role_assignment IS '用户角色临时授权表';
COMMENT ON COLUMN cmx_user_role_assignment.id IS '主键ID';
COMMENT ON COLUMN cmx_user_role_assignment.user_id IS '用户ID';
COMMENT ON COLUMN cmx_user_role_assignment.role_id IS '角色ID';
COMMENT ON COLUMN cmx_user_role_assignment.effective_from IS '生效开始时间';
COMMENT ON COLUMN cmx_user_role_assignment.effective_until IS '生效结束时间';
COMMENT ON COLUMN cmx_user_role_assignment.reason IS '授权理由（便于审计）';
COMMENT ON COLUMN cmx_user_role_assignment.source IS '授权来源：manual-手动，approval-审批，system-系统';
COMMENT ON COLUMN cmx_user_role_assignment.status IS '状态：0-已撤销，1-生效中';
COMMENT ON COLUMN cmx_user_role_assignment.revoked_by IS '撤销人';
COMMENT ON COLUMN cmx_user_role_assignment.revoked_at IS '撤销时间';
COMMENT ON COLUMN cmx_user_role_assignment.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_user_role_assignment.create_time IS '创建时间';
COMMENT ON COLUMN cmx_user_role_assignment.update_time IS '更新时间';
COMMENT ON COLUMN cmx_user_role_assignment.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_user_role_assignment.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_user_role_assignment.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_user_role_assignment.update_name IS '更新人姓名';

CREATE INDEX idx_cmx_user_role_assignment_user ON cmx_user_role_assignment (user_id);
CREATE INDEX idx_cmx_user_role_assignment_role ON cmx_user_role_assignment (role_id);
CREATE INDEX idx_cmx_user_role_assignment_time ON cmx_user_role_assignment (effective_from, effective_until);
CREATE INDEX idx_cmx_user_role_assignment_expire ON cmx_user_role_assignment (effective_until) WHERE status = 1 AND archived = 0;

-- 35. 权限规则表 (cmx_permission_rule)
DROP TABLE IF EXISTS cmx_permission_rule;
CREATE TABLE cmx_permission_rule
(
    id                 varchar(64) NOT NULL,
    code               varchar(100) NOT NULL,
    name               varchar(200) NOT NULL,
    rule_type          varchar(20) NOT NULL,
    violation_message  varchar(500),
    priority           int4 DEFAULT 0,
    description        varchar(500),
    status             int4 DEFAULT 1,
    archived           int4 DEFAULT 0,
    create_time        timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time        timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by          varchar(100),
    create_name        varchar(100),
    update_by          varchar(100),
    update_name        varchar(100),
    CONSTRAINT pk_cmx_permission_rule PRIMARY KEY (id),
    CONSTRAINT uk_cmx_permission_rule_code UNIQUE (code)
);

COMMENT ON TABLE cmx_permission_rule IS '权限规则表（互斥/依赖）';
COMMENT ON COLUMN cmx_permission_rule.id IS '主键ID';
COMMENT ON COLUMN cmx_permission_rule.code IS '规则编码（唯一，如 sod_finance）';
COMMENT ON COLUMN cmx_permission_rule.name IS '规则名称';
COMMENT ON COLUMN cmx_permission_rule.rule_type IS '规则类型：mutual_exclusion-互斥，dependency-依赖';
COMMENT ON COLUMN cmx_permission_rule.violation_message IS '违反规则时的错误消息（为空时使用默认消息）';
COMMENT ON COLUMN cmx_permission_rule.priority IS '优先级（数字越大越先校验，默认0）';
COMMENT ON COLUMN cmx_permission_rule.description IS '描述';
COMMENT ON COLUMN cmx_permission_rule.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_permission_rule.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_permission_rule.create_time IS '创建时间';
COMMENT ON COLUMN cmx_permission_rule.update_time IS '更新时间';
COMMENT ON COLUMN cmx_permission_rule.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_permission_rule.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_permission_rule.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_permission_rule.update_name IS '更新人姓名';

-- 36. 规则权限项表 (cmx_permission_rule_item)
DROP TABLE IF EXISTS cmx_permission_rule_item;
CREATE TABLE cmx_permission_rule_item
(
    id            varchar(64) NOT NULL,
    rule_id       varchar(64) NOT NULL,
    group_seq     int4 NOT NULL,
    permission_id varchar(64) NOT NULL,
    CONSTRAINT pk_cmx_permission_rule_item PRIMARY KEY (id),
    CONSTRAINT uk_cmx_permission_rule_item UNIQUE (rule_id, group_seq, permission_id)
);

CREATE INDEX idx_cmx_permission_rule_item_rule ON cmx_permission_rule_item (rule_id);
CREATE INDEX idx_cmx_permission_rule_item_perm ON cmx_permission_rule_item (permission_id);

COMMENT ON TABLE cmx_permission_rule_item IS '规则权限项表';
COMMENT ON COLUMN cmx_permission_rule_item.id IS '主键ID';
COMMENT ON COLUMN cmx_permission_rule_item.rule_id IS '关联规则ID';
COMMENT ON COLUMN cmx_permission_rule_item.group_seq IS '组序号：互斥规则下所有项两两互斥；依赖规则中 group_seq=1 为前置权限，group_seq=2 为依赖权限';
COMMENT ON COLUMN cmx_permission_rule_item.permission_id IS '关联权限ID';
