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
    title       VARCHAR(200),
    icon        VARCHAR(100),
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
COMMENT ON COLUMN cmx_domain.title IS '域英文标题/副标题';
COMMENT ON COLUMN cmx_domain.icon IS '域图标名（UI5 图标标识）';
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
    title       VARCHAR(200),
    icon        VARCHAR(100),
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
COMMENT ON COLUMN cmx_application.title IS '应用英文标题/副标题';
COMMENT ON COLUMN cmx_application.icon IS '应用图标名（UI5 图标标识）';
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
    title            VARCHAR(200),
    icon             VARCHAR(100),
    description      TEXT,
    type             VARCHAR(50),
    tags             TEXT,
    resource_root    VARCHAR(255),
    manifest_path    VARCHAR(500),
    theme            VARCHAR(100),
    theme_color      VARCHAR(50),
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
COMMENT ON COLUMN cmx_module.title IS '模块英文标题/副标题';
COMMENT ON COLUMN cmx_module.icon IS '模块图标名（UI5 图标标识）';
COMMENT ON COLUMN cmx_module.description IS '模块描述';
COMMENT ON COLUMN cmx_module.type IS '类型: business(业务模块), extension(扩展点), integration(集成点)';
COMMENT ON COLUMN cmx_module.tags IS '多标签，JSON数组字符串，如 ["总账","核心","FI-GL"]';
COMMENT ON COLUMN cmx_module.resource_root IS '模块资源目录相对路径（相对 data/ 根），格式 domain/application/module';
COMMENT ON COLUMN cmx_module.manifest_path IS '模块清单文件相对路径，格式 modules/<d>/<a>/<m>/module.json';
COMMENT ON COLUMN cmx_module.theme IS '模块主题名';
COMMENT ON COLUMN cmx_module.theme_color IS '模块主题色（十六进制或色名）';
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
    db_name               VARCHAR(128),
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
    domain_code           VARCHAR(64),
    application_code      VARCHAR(64),
    module_code           VARCHAR(64),
    source_type           VARCHAR(20),
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
COMMENT ON COLUMN cmx_sys_datasource.db_name IS '数据源名称（便于识别的显示名称）';
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
COMMENT ON COLUMN cmx_sys_datasource.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_sys_datasource.application_code IS '所属应用编码';
COMMENT ON COLUMN cmx_sys_datasource.module_code IS '所属模块编码';
COMMENT ON COLUMN cmx_sys_datasource.source_type IS '数据源类型：default-默认库，biz-业务库，other-其他';
COMMENT ON COLUMN cmx_sys_datasource.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_sys_datasource.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_sys_datasource.create_time IS '创建时间';
COMMENT ON COLUMN cmx_sys_datasource.update_time IS '更新时间';
COMMENT ON COLUMN cmx_sys_datasource.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_sys_datasource.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_sys_datasource.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_sys_datasource.update_name IS '更新人名称';

CREATE INDEX idx_datasource_domain_app_module ON cmx_sys_datasource (domain_code, application_code, module_code);

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
CREATE UNIQUE INDEX uk_cmx_plugin_versions_plugin_version ON cmx_plugin_versions (plugin_id, app_id, version);

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

-- =============================================
-- 10. 通用审计日志表 (cmx_audit_log)
-- 记录 Auth/Iam/Plugin/Biz 四个域的通用审计事件
-- =============================================
DROP TABLE IF EXISTS cmx_audit_log;
CREATE TABLE cmx_audit_log
(
    id              VARCHAR(64)              NOT NULL,
    app_id          VARCHAR(64)              NOT NULL DEFAULT 'default',
    domain          VARCHAR(20)              NOT NULL,
    operation       VARCHAR(100)             NOT NULL,
    result          VARCHAR(20)              NOT NULL,
    actor_id        VARCHAR(64),
    actor_name      VARCHAR(100),
    target_type     VARCHAR(50),
    target_id       VARCHAR(64),
    details         TEXT,
    request_id      VARCHAR(100),
    ip_address      VARCHAR(50),
    started_at      TIMESTAMP WITH TIME ZONE NOT NULL,
    duration_ms     BIGINT,
    create_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived        INT4                     NOT NULL DEFAULT 0,
    create_by       VARCHAR(100),
    create_name     VARCHAR(100),
    update_by       VARCHAR(100),
    update_name     VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_audit_log IS '通用审计日志表（Auth/Iam/Plugin/Biz 四域）';
COMMENT ON COLUMN cmx_audit_log.id IS '主键ID';
COMMENT ON COLUMN cmx_audit_log.app_id IS '应用隔离标识';
COMMENT ON COLUMN cmx_audit_log.domain IS '审计域：auth/iam/plugin/biz';
COMMENT ON COLUMN cmx_audit_log.operation IS '操作名称';
COMMENT ON COLUMN cmx_audit_log.result IS '操作结果：success/failure';
COMMENT ON COLUMN cmx_audit_log.actor_id IS '操作者ID';
COMMENT ON COLUMN cmx_audit_log.actor_name IS '操作者名称';
COMMENT ON COLUMN cmx_audit_log.target_type IS '目标资源类型';
COMMENT ON COLUMN cmx_audit_log.target_id IS '目标资源ID';
COMMENT ON COLUMN cmx_audit_log.details IS '操作详情（JSON 序列化文本）';
COMMENT ON COLUMN cmx_audit_log.request_id IS '请求ID（链路追踪）';
COMMENT ON COLUMN cmx_audit_log.ip_address IS '来源IP';
COMMENT ON COLUMN cmx_audit_log.started_at IS '操作开始时间';
COMMENT ON COLUMN cmx_audit_log.duration_ms IS '操作耗时（毫秒）';
COMMENT ON COLUMN cmx_audit_log.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_audit_log.create_time IS '创建时间';
COMMENT ON COLUMN cmx_audit_log.update_time IS '更新时间';
COMMENT ON COLUMN cmx_audit_log.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_audit_log.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_audit_log.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_audit_log.update_name IS '更新人姓名';

CREATE INDEX idx_cmx_audit_log_app_id   ON cmx_audit_log (app_id);
CREATE INDEX idx_cmx_audit_log_domain   ON cmx_audit_log (domain);
CREATE INDEX idx_cmx_audit_log_actor    ON cmx_audit_log (actor_id);
CREATE INDEX idx_cmx_audit_log_target   ON cmx_audit_log (target_type, target_id);
CREATE INDEX idx_cmx_audit_log_request  ON cmx_audit_log (request_id);
CREATE INDEX idx_cmx_audit_log_started  ON cmx_audit_log (started_at);
CREATE INDEX idx_cmx_audit_log_archived ON cmx_audit_log (archived);
CREATE INDEX idx_cmx_audit_log_result   ON cmx_audit_log (result);

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
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_cmx_marketplace_plugin_plugin_id ON cmx_marketplace_plugin (plugin_id);

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
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_cmx_marketplace_plugin_version_pver ON cmx_marketplace_plugin_version (plugin_id, version);

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
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_cmx_marketplace_dstats_unique ON cmx_marketplace_download_stats (plugin_id, version, download_date, source_type);

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
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_cmx_marketplace_rating_unique ON cmx_marketplace_rating (plugin_id, user_id);

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

CREATE UNIQUE INDEX uk_cmx_auth_client_client_id ON cmx_auth_client (client_id) WHERE archived = 0;

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
-- 28. Token 事件审计表 (cmx_auth_token_event)
-- =============================================
DROP TABLE IF EXISTS cmx_auth_token_event;
CREATE TABLE cmx_auth_token_event
(
    id           VARCHAR(64)  NOT NULL,
    event_type   VARCHAR(50)  NOT NULL,
    user_id      VARCHAR(64)  NOT NULL,
    jti          VARCHAR(100),
    detail       VARCHAR(500),
    archived     INT4      DEFAULT 0,
    create_time  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by    VARCHAR(100),
    create_name  VARCHAR(100),
    update_by    VARCHAR(100),
    update_name  VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_token_event IS 'Token 事件审计表（记录签发/撤销/刷新等关键事件）';
COMMENT ON COLUMN cmx_auth_token_event.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_token_event.event_type IS '事件类型：token_issued/token_revoked/token_refreshed/login_success/login_failed/password_changed';
COMMENT ON COLUMN cmx_auth_token_event.user_id IS '用户ID';
COMMENT ON COLUMN cmx_auth_token_event.jti IS 'JWT ID（关联 Token）';
COMMENT ON COLUMN cmx_auth_token_event.detail IS '事件详情';
COMMENT ON COLUMN cmx_auth_token_event.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_token_event.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_token_event.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_token_event.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_token_event.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_token_event.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_token_event.update_name IS '更新人姓名';

CREATE INDEX idx_cmx_auth_token_event_user ON cmx_auth_token_event (user_id);
CREATE INDEX idx_cmx_auth_token_event_type ON cmx_auth_token_event (event_type);
CREATE INDEX idx_cmx_auth_token_event_created ON cmx_auth_token_event (create_time);

-- =============================================
-- 29. 第三方 OAuth2 账号关联表 (cmx_auth_oauth2_account)
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
    gender         INT4      DEFAULT 0,
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
COMMENT ON COLUMN cmx_user.gender IS '性别：0-未知，1-男，2-女';
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

CREATE UNIQUE INDEX uk_cmx_user_username ON cmx_user (username) WHERE archived = 0;
CREATE UNIQUE INDEX uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL AND archived = 0;

-- =============================================
-- 29a. 角色组表 (cmx_role_group)
-- =============================================
DROP TABLE IF EXISTS cmx_role_group;
CREATE TABLE cmx_role_group
(
    id          VARCHAR(64)  NOT NULL,
    name        VARCHAR(100) NOT NULL,
    parent_id   VARCHAR(64),
    sort_order  INT4      DEFAULT 0,
    description VARCHAR(500),
    archived    INT4      DEFAULT 0,
    status      INT4      DEFAULT 1,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by   VARCHAR(100),
    create_name VARCHAR(100),
    update_by   VARCHAR(100),
    update_name VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_role_group IS '角色组表（树形结构）';
COMMENT ON COLUMN cmx_role_group.id IS '主键ID';
COMMENT ON COLUMN cmx_role_group.name IS '角色组名称';
COMMENT ON COLUMN cmx_role_group.parent_id IS '父角色组ID（NULL=根节点）';
COMMENT ON COLUMN cmx_role_group.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_role_group.description IS '描述';
COMMENT ON COLUMN cmx_role_group.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_role_group.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_role_group.create_time IS '创建时间';
COMMENT ON COLUMN cmx_role_group.update_time IS '更新时间';
COMMENT ON COLUMN cmx_role_group.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_role_group.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_role_group.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_role_group.update_name IS '更新人姓名';

CREATE INDEX idx_cmx_role_group_parent ON cmx_role_group (parent_id);
CREATE INDEX idx_cmx_role_group_status ON cmx_role_group (status);

-- =============================================
-- 30. 角色表 (cmx_role)
-- =============================================
DROP TABLE IF EXISTS cmx_role;
CREATE TABLE cmx_role
(
    id            VARCHAR(64)  NOT NULL,
    code          VARCHAR(100) NOT NULL,
    name          VARCHAR(100) NOT NULL,
    role_group_id VARCHAR(64),
    data_scope    INT4      DEFAULT 1,
    sort_order    INT4      DEFAULT 0,
    description   VARCHAR(500),
    status        INT4      DEFAULT 1,
    archived      INT4      DEFAULT 0,
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
COMMENT ON COLUMN cmx_role.role_group_id IS '所属角色组ID';
COMMENT ON COLUMN cmx_role.data_scope IS '数据权限范围：1-全部，2-自定义，3-本部门，4-本部门及子部门，5-仅本人（预留字段）';
COMMENT ON COLUMN cmx_role.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_role.description IS '描述';
COMMENT ON COLUMN cmx_role.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_role.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_role.create_time IS '创建时间';
COMMENT ON COLUMN cmx_role.update_time IS '更新时间';
COMMENT ON COLUMN cmx_role.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_role.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_role.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_role.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_role_code ON cmx_role (code) WHERE archived = 0;
CREATE INDEX idx_cmx_role_group_id ON cmx_role (role_group_id);

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

CREATE UNIQUE INDEX uk_cmx_user_role ON cmx_user_role (user_id, role_id) WHERE archived = 0;
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
    domain_code   VARCHAR(100),
    app_code      VARCHAR(100),
    module_code   VARCHAR(100),
    extension     TEXT,
    parent_code    VARCHAR(200),                   -- 父权限 code（根为 NULL）
    full_code_path VARCHAR(1000) NOT NULL,         -- code 全路径，如 /user:list/user:delete
    is_leaf        INT4      DEFAULT 0,            -- 1 叶子 / 0 非叶子
    level          INT4      DEFAULT 1,            -- 层级深度，根=1
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
COMMENT ON COLUMN cmx_permission.domain_code IS '所属域编码（如 platform、tenant）';
COMMENT ON COLUMN cmx_permission.app_code IS '所属应用编码（如 user-center、billing）';
COMMENT ON COLUMN cmx_permission.module_code IS '所属模块编码（如 user、order）';
COMMENT ON COLUMN cmx_permission.extension IS '扩展配置（用户自定义 JSON 文本）';
COMMENT ON COLUMN cmx_permission.parent_code IS '父权限编码（根节点为 NULL）';
COMMENT ON COLUMN cmx_permission.full_code_path IS '权限编码全路径（如 /user:list/user:delete）';
COMMENT ON COLUMN cmx_permission.is_leaf IS '是否叶子节点：1-是，0-否';
COMMENT ON COLUMN cmx_permission.level IS '层级深度（根节点为 1）';
COMMENT ON COLUMN cmx_permission.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_permission.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_permission.create_time IS '创建时间';
COMMENT ON COLUMN cmx_permission.update_time IS '更新时间';
COMMENT ON COLUMN cmx_permission.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_permission.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_permission.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_permission.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_permission_code ON cmx_permission (code) WHERE archived = 0;
CREATE INDEX idx_cmx_permission_parent ON cmx_permission (parent_id);
CREATE INDEX idx_cmx_permission_full_path ON cmx_permission (full_code_path);
CREATE INDEX idx_cmx_permission_parent_code ON cmx_permission (parent_code);
CREATE INDEX idx_cmx_permission_domain_code ON cmx_permission (domain_code);
CREATE INDEX idx_cmx_permission_app_code ON cmx_permission (app_code);
CREATE INDEX idx_cmx_permission_module_code ON cmx_permission (module_code);

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

CREATE UNIQUE INDEX uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id) WHERE archived = 0;
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
    PRIMARY KEY (id)
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

-- 35. 互斥规则表 (cmx_exclusion_rule)
DROP TABLE IF EXISTS cmx_exclusion_rule;
CREATE TABLE cmx_exclusion_rule
(
    id                  varchar(64) NOT NULL,
    code                varchar(100) NOT NULL,
    name                varchar(200) NOT NULL,
    subject_type        varchar(20) NOT NULL,
    primary_subject_id  varchar(64) NOT NULL,
    violation_message   varchar(500),
    priority            int4 DEFAULT 0,
    description         varchar(500),
    status              int4 DEFAULT 1,
    archived            int4 DEFAULT 0,
    create_time         timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time         timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by           varchar(100),
    create_name         varchar(100),
    update_by           varchar(100),
    update_name         varchar(100),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_cmx_exclusion_rule_code ON cmx_exclusion_rule (code) WHERE archived = 0;

COMMENT ON TABLE cmx_exclusion_rule IS '互斥规则表（功能互斥/角色互斥）';
COMMENT ON COLUMN cmx_exclusion_rule.id IS '主键ID';
COMMENT ON COLUMN cmx_exclusion_rule.code IS '规则编码（唯一）';
COMMENT ON COLUMN cmx_exclusion_rule.name IS '规则名称';
COMMENT ON COLUMN cmx_exclusion_rule.subject_type IS '对象类型：permission-功能权限互斥，role-角色互斥';
COMMENT ON COLUMN cmx_exclusion_rule.primary_subject_id IS '主要对象ID（权限ID或角色ID，取决于 subject_type）';
COMMENT ON COLUMN cmx_exclusion_rule.violation_message IS '违反规则时的错误消息（为空时使用默认消息）';
COMMENT ON COLUMN cmx_exclusion_rule.priority IS '优先级（数字越大越先校验，默认0）';
COMMENT ON COLUMN cmx_exclusion_rule.description IS '描述';
COMMENT ON COLUMN cmx_exclusion_rule.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_exclusion_rule.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_exclusion_rule.create_time IS '创建时间';
COMMENT ON COLUMN cmx_exclusion_rule.update_time IS '更新时间';
COMMENT ON COLUMN cmx_exclusion_rule.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_exclusion_rule.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_exclusion_rule.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_exclusion_rule.update_name IS '更新人姓名';

-- 36. 互斥对象明细表 (cmx_exclusion_rule_item)
DROP TABLE IF EXISTS cmx_exclusion_rule_item;
CREATE TABLE cmx_exclusion_rule_item
(
    id          varchar(64) NOT NULL,
    rule_id     varchar(64) NOT NULL,
    subject_id  varchar(64) NOT NULL,
    archived    int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by   varchar(100),
    create_name varchar(100),
    update_by   varchar(100),
    update_name varchar(100),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_cmx_exclusion_rule_item ON cmx_exclusion_rule_item (rule_id, subject_id);
CREATE INDEX idx_cmx_exclusion_rule_item_rule ON cmx_exclusion_rule_item (rule_id);
CREATE INDEX idx_cmx_exclusion_rule_item_subject ON cmx_exclusion_rule_item (subject_id);

COMMENT ON TABLE cmx_exclusion_rule_item IS '互斥对象明细表';
COMMENT ON COLUMN cmx_exclusion_rule_item.id IS '主键ID';
COMMENT ON COLUMN cmx_exclusion_rule_item.rule_id IS '关联规则ID';
COMMENT ON COLUMN cmx_exclusion_rule_item.subject_id IS '互斥对象ID（权限ID或角色ID，与规则 subject_type 一致）';
COMMENT ON COLUMN cmx_exclusion_rule_item.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_exclusion_rule_item.create_time IS '创建时间';
COMMENT ON COLUMN cmx_exclusion_rule_item.update_time IS '更新时间';
COMMENT ON COLUMN cmx_exclusion_rule_item.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_exclusion_rule_item.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_exclusion_rule_item.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_exclusion_rule_item.update_name IS '更新人姓名';

-- =============================================
-- 37. 表单定义表 (cmx_form)
-- =============================================
DROP TABLE IF EXISTS cmx_form;
CREATE TABLE cmx_form
(
    id               VARCHAR(64)  NOT NULL,
    code             VARCHAR(128) NOT NULL,
    name             VARCHAR(256) NOT NULL,
    description      TEXT,
    definition       JSONB,
    version          VARCHAR(64)  DEFAULT '1.0.0',
    domain_code      VARCHAR(64)  NOT NULL,
    application_code VARCHAR(64)  NOT NULL,
    module_code      VARCHAR(64)  NOT NULL,
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

COMMENT ON TABLE cmx_form IS '表单定义表';
COMMENT ON COLUMN cmx_form.id IS '主键ID';
COMMENT ON COLUMN cmx_form.code IS '表单编码，模块内唯一';
COMMENT ON COLUMN cmx_form.name IS '表单名称';
COMMENT ON COLUMN cmx_form.description IS '表单描述';
COMMENT ON COLUMN cmx_form.definition IS '表单完整定义JSON(字段/布局/校验)';
COMMENT ON COLUMN cmx_form.version IS '表单版本';
COMMENT ON COLUMN cmx_form.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_form.application_code IS '所属应用编码';
COMMENT ON COLUMN cmx_form.module_code IS '所属模块编码';
COMMENT ON COLUMN cmx_form.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_form.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_form.create_time IS '创建时间';
COMMENT ON COLUMN cmx_form.update_time IS '更新时间';
COMMENT ON COLUMN cmx_form.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_form.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_form.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_form.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_form_code ON cmx_form (code);
CREATE INDEX idx_cmx_form_module ON cmx_form (domain_code, application_code, module_code);

-- =============================================
-- 38. 菜单定义表 (cmx_menu)
-- =============================================
DROP TABLE IF EXISTS cmx_menu;
CREATE TABLE cmx_menu
(
    id               VARCHAR(64)   NOT NULL,
    code             VARCHAR(128)  NOT NULL,
    name             VARCHAR(256)  NOT NULL,
    description      VARCHAR(500),
    path             VARCHAR(512),
    icon             VARCHAR(128),
    component        VARCHAR(512),
    sort_order       INT4       DEFAULT 0,
    visible          INT4       DEFAULT 1,
    open_type        INT4       DEFAULT 0,
    fun_code         VARCHAR(200),
    domain_code      VARCHAR(64)   NOT NULL,
    application_code VARCHAR(64)   NOT NULL,
    module_code      VARCHAR(64)   NOT NULL,
    definition       JSONB,
    status           INT4       DEFAULT 1,
    leaf             INT4       DEFAULT 1,
    depth            INT4       DEFAULT 1,
    parent_id        VARCHAR(64),
    parent_code      VARCHAR(128),
    id_path          VARCHAR(1000),
    code_path        VARCHAR(1000),
    archived         INT4       DEFAULT 0,
    create_time      TIMESTAMP  DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP  DEFAULT CURRENT_TIMESTAMP,
    create_by        VARCHAR(100),
    create_name      VARCHAR(100),
    update_by        VARCHAR(100),
    update_name      VARCHAR(100),
    ext_attributes   TEXT,
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_menu IS '菜单定义表';
COMMENT ON COLUMN cmx_menu.id IS '主键ID';
COMMENT ON COLUMN cmx_menu.code IS '菜单编码，唯一';
COMMENT ON COLUMN cmx_menu.name IS '菜单名称';
COMMENT ON COLUMN cmx_menu.description IS '菜单描述';
COMMENT ON COLUMN cmx_menu.path IS '前端路由路径';
COMMENT ON COLUMN cmx_menu.icon IS '菜单图标';
COMMENT ON COLUMN cmx_menu.component IS '前端组件路径';
COMMENT ON COLUMN cmx_menu.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_menu.visible IS '是否可见：0-隐藏，1-显示';
COMMENT ON COLUMN cmx_menu.open_type IS '打开方式：0-应用页标签,1-浏览器标签,2-弹窗,3-抽屉,4-全屏显示,5-下拉菜单';
COMMENT ON COLUMN cmx_menu.fun_code IS '功能码，关联 cmx_permission.code';
COMMENT ON COLUMN cmx_menu.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_menu.application_code IS '所属应用编码';
COMMENT ON COLUMN cmx_menu.module_code IS '所属模块编码';
COMMENT ON COLUMN cmx_menu.definition IS '菜单完整定义JSON(items/children树形结构，整体透传)';
COMMENT ON COLUMN cmx_menu.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_menu.leaf IS '是否明细：1-是叶子节点，0-非叶子节点';
COMMENT ON COLUMN cmx_menu.depth IS '级数：根节点为1，逐层递增';
COMMENT ON COLUMN cmx_menu.parent_id IS '父节点ID，根节点为空';
COMMENT ON COLUMN cmx_menu.parent_code IS '父节点编码，根节点为空';
COMMENT ON COLUMN cmx_menu.id_path IS 'ID全路径，以/分隔，如 /root_id/parent_id/current_id';
COMMENT ON COLUMN cmx_menu.code_path IS '编号全路径，以/分隔，如 /ROOT_CODE/PARENT_CODE/CURRENT_CODE';
COMMENT ON COLUMN cmx_menu.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_menu.create_time IS '创建时间';
COMMENT ON COLUMN cmx_menu.update_time IS '更新时间';
COMMENT ON COLUMN cmx_menu.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_menu.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_menu.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_menu.update_name IS '更新人姓名';
COMMENT ON COLUMN cmx_menu.ext_attributes IS '扩展属性，存储JSON格式的额外业务属性';

CREATE UNIQUE INDEX uk_cmx_menu_code ON cmx_menu (code) WHERE archived = 0;
CREATE INDEX idx_cmx_menu_module ON cmx_menu (domain_code, application_code, module_code);
CREATE INDEX idx_cmx_menu_parent_id ON cmx_menu (parent_id);
-- 级联操作(移动/删除/树查询)按 code_path/id_path 前缀匹配,需索引支撑,否则全表扫描
CREATE INDEX idx_cmx_menu_code_path ON cmx_menu (code_path);
CREATE INDEX idx_cmx_menu_id_path ON cmx_menu (id_path);

-- =============================================
-- 39. 模块当前版本表 (cmx_module_current_version)
-- =============================================
DROP TABLE IF EXISTS cmx_module_current_version;
CREATE TABLE cmx_module_current_version
(
    id                VARCHAR(64) NOT NULL,
    module_id         VARCHAR(64) NOT NULL,
    domain_code       VARCHAR(64) NOT NULL,
    application_code  VARCHAR(64) NOT NULL,
    module_code       VARCHAR(64) NOT NULL,
    package_version   VARCHAR(14) NOT NULL,
    checksum          VARCHAR(128),
    manifest_snapshot JSONB,
    imported_at       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    imported_by       VARCHAR(100),
    source            VARCHAR(256),
    archived          INT4      DEFAULT 0,
    create_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by         VARCHAR(100),
    create_name       VARCHAR(100),
    update_by         VARCHAR(100),
    update_name       VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_module_current_version IS '模块当前版本表：每个模块当前生效的迁移包版本';
COMMENT ON COLUMN cmx_module_current_version.id IS '主键ID';
COMMENT ON COLUMN cmx_module_current_version.module_id IS '关联模块ID(逻辑关联cmx_module.id)';
COMMENT ON COLUMN cmx_module_current_version.domain_code IS '域编码';
COMMENT ON COLUMN cmx_module_current_version.application_code IS '应用编码';
COMMENT ON COLUMN cmx_module_current_version.module_code IS '模块编码(唯一，一个模块一行)';
COMMENT ON COLUMN cmx_module_current_version.package_version IS '当前迁移包版本号(导出时间戳yyyyMMddHHmmSS)';
COMMENT ON COLUMN cmx_module_current_version.checksum IS '当前迁移包校验和sha256';
COMMENT ON COLUMN cmx_module_current_version.manifest_snapshot IS '当前module.manifest.json快照';
COMMENT ON COLUMN cmx_module_current_version.imported_at IS '最近一次导入时间';
COMMENT ON COLUMN cmx_module_current_version.imported_by IS '最近一次导入人';
COMMENT ON COLUMN cmx_module_current_version.source IS '来源(文件名/URL)';
COMMENT ON COLUMN cmx_module_current_version.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_module_current_version.create_time IS '创建时间';
COMMENT ON COLUMN cmx_module_current_version.update_time IS '更新时间';
COMMENT ON COLUMN cmx_module_current_version.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_module_current_version.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_module_current_version.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_module_current_version.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_module_current_version_module ON cmx_module_current_version (module_code);
CREATE INDEX idx_cmx_module_current_version_dom_app_mod ON cmx_module_current_version (domain_code, application_code, module_code);

-- =============================================
-- 40. 模块版本历史表 (cmx_module_version_history)
-- =============================================
DROP TABLE IF EXISTS cmx_module_version_history;
CREATE TABLE cmx_module_version_history
(
    id                VARCHAR(64) NOT NULL,
    module_id         VARCHAR(64) NOT NULL,
    domain_code       VARCHAR(64) NOT NULL,
    application_code  VARCHAR(64) NOT NULL,
    module_code       VARCHAR(64) NOT NULL,
    package_version   VARCHAR(14) NOT NULL,
    checksum          VARCHAR(128),
    manifest_snapshot JSONB,
    imported_at       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    imported_by       VARCHAR(100),
    source            VARCHAR(256),
    notes             TEXT,
    archived          INT4      DEFAULT 0,
    create_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by         VARCHAR(100),
    create_name       VARCHAR(100),
    update_by         VARCHAR(100),
    update_name       VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_module_version_history IS '模块迁移包版本历史表';
COMMENT ON COLUMN cmx_module_version_history.id IS '主键ID';
COMMENT ON COLUMN cmx_module_version_history.module_id IS '关联模块ID(逻辑关联cmx_module.id)';
COMMENT ON COLUMN cmx_module_version_history.domain_code IS '域编码';
COMMENT ON COLUMN cmx_module_version_history.application_code IS '应用编码';
COMMENT ON COLUMN cmx_module_version_history.module_code IS '模块编码';
COMMENT ON COLUMN cmx_module_version_history.package_version IS '迁移包版本号(导出时间戳yyyyMMddHHmmSS)';
COMMENT ON COLUMN cmx_module_version_history.checksum IS '迁移包校验和sha256';
COMMENT ON COLUMN cmx_module_version_history.manifest_snapshot IS '导入时的module.manifest.json快照';
COMMENT ON COLUMN cmx_module_version_history.imported_at IS '导入时间';
COMMENT ON COLUMN cmx_module_version_history.imported_by IS '导入人';
COMMENT ON COLUMN cmx_module_version_history.source IS '来源(文件名/URL)';
COMMENT ON COLUMN cmx_module_version_history.notes IS '备注';
COMMENT ON COLUMN cmx_module_version_history.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_module_version_history.create_time IS '创建时间';
COMMENT ON COLUMN cmx_module_version_history.update_time IS '更新时间';
COMMENT ON COLUMN cmx_module_version_history.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_module_version_history.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_module_version_history.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_module_version_history.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_module_version_history_pkg ON cmx_module_version_history (module_code, package_version);
CREATE INDEX idx_cmx_module_version_history_module ON cmx_module_version_history (module_id);

-- =============================================
-- 37. 模型中心台账自描述表 (cmx_model_meta)
-- =============================================
DROP TABLE IF EXISTS cmx_model_meta;
CREATE TABLE cmx_model_meta
(
    id               VARCHAR(64)  NOT NULL,
    db_id            VARCHAR(100),
    meta_version     INT4         NOT NULL DEFAULT 1,
    app_id           VARCHAR(64)  NOT NULL,
    engine_version   VARCHAR(50),
    portal_version   VARCHAR(50),
    status           VARCHAR(20)  NOT NULL DEFAULT 'ready',
    initialized_at   TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    initialized_by   VARCHAR(100),
    initialized_name VARCHAR(100),
    last_upgraded_at TIMESTAMP,
    last_upgraded_by VARCHAR(100),
    remark           VARCHAR(500),
    create_time      TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_model_meta_db_app ON cmx_model_meta (db_id, app_id);

COMMENT ON TABLE  cmx_model_meta IS '模型中心台账自描述（每库单例）';
COMMENT ON COLUMN cmx_model_meta.id IS '主键ID';
COMMENT ON COLUMN cmx_model_meta.db_id IS '数据库ID';
COMMENT ON COLUMN cmx_model_meta.meta_version IS '台账 schema 版本，用于判定是否需要升级系统表';
COMMENT ON COLUMN cmx_model_meta.app_id IS '应用ID';
COMMENT ON COLUMN cmx_model_meta.engine_version IS '引擎版本';
COMMENT ON COLUMN cmx_model_meta.portal_version IS '门户版本';
COMMENT ON COLUMN cmx_model_meta.status IS '台账状态: ready / upgrading / failed';
COMMENT ON COLUMN cmx_model_meta.initialized_at IS '初始化时间';
COMMENT ON COLUMN cmx_model_meta.initialized_by IS '初始化人ID';
COMMENT ON COLUMN cmx_model_meta.initialized_name IS '初始化人姓名';
COMMENT ON COLUMN cmx_model_meta.last_upgraded_at IS '最近升级时间';
COMMENT ON COLUMN cmx_model_meta.last_upgraded_by IS '最近升级人';
COMMENT ON COLUMN cmx_model_meta.remark IS '备注';
COMMENT ON COLUMN cmx_model_meta.create_time IS '创建时间';
COMMENT ON COLUMN cmx_model_meta.update_time IS '更新时间';

-- =============================================
-- 38. 模型中心-模块部署当前态表 (cmx_model_module)
-- =============================================
DROP TABLE IF EXISTS cmx_model_module;
CREATE TABLE cmx_model_module
(
    id                  VARCHAR(64) NOT NULL,
    db_id               VARCHAR(100),
    app_id              VARCHAR(64) NOT NULL,
    domain_code         VARCHAR(100),
    application_code    VARCHAR(100),
    module_code         VARCHAR(100),
    module_name         VARCHAR(200),
    overall_status      VARCHAR(20) DEFAULT 'active',
    table_count         INT4        DEFAULT 0,
    def_source          VARCHAR(300),
    def_checksum        VARCHAR(64),
    first_deployed_at   TIMESTAMP,
    current_deployed_at TIMESTAMP,
    deployed_by         VARCHAR(100),
    deployed_name       VARCHAR(100),
    archived            INT4        DEFAULT 0,
    create_time         TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    update_time         TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_model_module_key ON cmx_model_module (db_id, app_id, domain_code, application_code, module_code);

COMMENT ON TABLE  cmx_model_module IS '模型中心-模块部署当前态主表（每模块一行；类型状态见 cmx_model_module_kind）';
COMMENT ON COLUMN cmx_model_module.id IS '主键ID';
COMMENT ON COLUMN cmx_model_module.db_id IS '数据库ID';
COMMENT ON COLUMN cmx_model_module.app_id IS '应用ID';
COMMENT ON COLUMN cmx_model_module.domain_code IS '域编码';
COMMENT ON COLUMN cmx_model_module.application_code IS '应用编码';
COMMENT ON COLUMN cmx_model_module.module_code IS '模块编码';
COMMENT ON COLUMN cmx_model_module.module_name IS '模块名称';
COMMENT ON COLUMN cmx_model_module.overall_status IS '整体状态: active/failed';
COMMENT ON COLUMN cmx_model_module.table_count IS '表数量';
COMMENT ON COLUMN cmx_model_module.def_source IS '定义来源文件';
COMMENT ON COLUMN cmx_model_module.def_checksum IS '定义文件校验和';
COMMENT ON COLUMN cmx_model_module.first_deployed_at IS '首次部署时间';
COMMENT ON COLUMN cmx_model_module.current_deployed_at IS '当前部署时间';
COMMENT ON COLUMN cmx_model_module.deployed_by IS '部署人ID';
COMMENT ON COLUMN cmx_model_module.deployed_name IS '部署人姓名';
COMMENT ON COLUMN cmx_model_module.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_model_module.create_time IS '创建时间';
COMMENT ON COLUMN cmx_model_module.update_time IS '更新时间';

-- =============================================
-- 39. 模型中心-模块类型当前态表 (cmx_model_module_kind)
-- =============================================
DROP TABLE IF EXISTS cmx_model_module_kind;
CREATE TABLE cmx_model_module_kind
(
    id               VARCHAR(64) NOT NULL,
    db_id            VARCHAR(100),
    app_id           VARCHAR(64) NOT NULL,
    domain_code      VARCHAR(100),
    application_code VARCHAR(100),
    module_code      VARCHAR(100),
    kind             VARCHAR(20) NOT NULL,
    version          VARCHAR(50),
    status           VARCHAR(20) DEFAULT 'none',
    table_count      INT4        DEFAULT 0,
    def_source       VARCHAR(300),
    def_checksum     VARCHAR(64),
    deployed_at      TIMESTAMP,
    deployed_by      VARCHAR(100),
    deployed_name    VARCHAR(100),
    error_message    TEXT,
    archived         INT4        DEFAULT 0,
    create_time      TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_model_module_kind_key ON cmx_model_module_kind (db_id, app_id, domain_code, application_code, module_code, kind);
CREATE INDEX idx_model_module_kind_module ON cmx_model_module_kind (db_id, domain_code, application_code, module_code);

COMMENT ON TABLE  cmx_model_module_kind IS '模型中心-模块类型当前态（每模块每 kind 一行；新增类型不改表结构）';
COMMENT ON COLUMN cmx_model_module_kind.id IS '主键ID';
COMMENT ON COLUMN cmx_model_module_kind.db_id IS '数据库ID';
COMMENT ON COLUMN cmx_model_module_kind.app_id IS '应用ID';
COMMENT ON COLUMN cmx_model_module_kind.domain_code IS '域编码';
COMMENT ON COLUMN cmx_model_module_kind.application_code IS '应用编码';
COMMENT ON COLUMN cmx_model_module_kind.module_code IS '模块编码';
COMMENT ON COLUMN cmx_model_module_kind.kind IS '模块类型: DCT/DOC/RPT/SEED/...';
COMMENT ON COLUMN cmx_model_module_kind.version IS '当前版本';
COMMENT ON COLUMN cmx_model_module_kind.status IS '类型状态: none/current/failed/upgrading';
COMMENT ON COLUMN cmx_model_module_kind.table_count IS '表数量';
COMMENT ON COLUMN cmx_model_module_kind.def_source IS '定义来源文件';
COMMENT ON COLUMN cmx_model_module_kind.def_checksum IS '定义文件校验和';
COMMENT ON COLUMN cmx_model_module_kind.deployed_at IS '部署时间';
COMMENT ON COLUMN cmx_model_module_kind.deployed_by IS '部署人ID';
COMMENT ON COLUMN cmx_model_module_kind.deployed_name IS '部署人姓名';
COMMENT ON COLUMN cmx_model_module_kind.error_message IS '错误信息';
COMMENT ON COLUMN cmx_model_module_kind.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_model_module_kind.create_time IS '创建时间';
COMMENT ON COLUMN cmx_model_module_kind.update_time IS '更新时间';

-- =============================================
-- 40. 模型中心-部署/升级历史表 (cmx_model_deploy_history)
-- =============================================
DROP TABLE IF EXISTS cmx_model_deploy_history;
CREATE TABLE cmx_model_deploy_history
(
    id               VARCHAR(64) NOT NULL,
    batch_id         VARCHAR(64),
    db_id            VARCHAR(100),
    app_id           VARCHAR(64) NOT NULL,
    domain_code      VARCHAR(100),
    application_code VARCHAR(100),
    module_code      VARCHAR(100),
    module_name      VARCHAR(200),
    kind             VARCHAR(20),
    action           VARCHAR(20),
    from_version     VARCHAR(50),
    to_version       VARCHAR(50),
    status           VARCHAR(20),
    ddl_summary      JSONB,
    object_count     INT4       DEFAULT 0,
    seed_rows        INT4       DEFAULT 0,
    def_ref          VARCHAR(300),
    def_version      VARCHAR(50),
    engine_version   VARCHAR(50),
    error_message    TEXT,
    started_at       TIMESTAMP,
    finished_at      TIMESTAMP,
    duration_ms      INT8,
    operator_id      VARCHAR(100),
    operator_name    VARCHAR(100),
    create_time      TIMESTAMP  DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE INDEX idx_model_history_module ON cmx_model_deploy_history (db_id, domain_code, application_code, module_code);
CREATE INDEX idx_model_history_batch  ON cmx_model_deploy_history (batch_id);
CREATE INDEX idx_model_history_time   ON cmx_model_deploy_history (create_time);

COMMENT ON TABLE  cmx_model_deploy_history IS '模型中心-部署/升级历史（追加式，永不改写）';
COMMENT ON COLUMN cmx_model_deploy_history.id IS '主键ID';
COMMENT ON COLUMN cmx_model_deploy_history.batch_id IS '批次ID';
COMMENT ON COLUMN cmx_model_deploy_history.db_id IS '数据库ID';
COMMENT ON COLUMN cmx_model_deploy_history.app_id IS '应用ID';
COMMENT ON COLUMN cmx_model_deploy_history.domain_code IS '域编码';
COMMENT ON COLUMN cmx_model_deploy_history.application_code IS '应用编码';
COMMENT ON COLUMN cmx_model_deploy_history.module_code IS '模块编码';
COMMENT ON COLUMN cmx_model_deploy_history.module_name IS '模块名称';
COMMENT ON COLUMN cmx_model_deploy_history.kind IS '操作类别: INIT/META_UPGRADE/DCT/DOC/SEED';
COMMENT ON COLUMN cmx_model_deploy_history.action IS '动作: deploy/upgrade/rollback';
COMMENT ON COLUMN cmx_model_deploy_history.from_version IS '原版本';
COMMENT ON COLUMN cmx_model_deploy_history.to_version IS '目标版本';
COMMENT ON COLUMN cmx_model_deploy_history.status IS '状态机: pending→executing→success/failed/skipped';
COMMENT ON COLUMN cmx_model_deploy_history.ddl_summary IS 'DDL 摘要 JSON';
COMMENT ON COLUMN cmx_model_deploy_history.object_count IS '对象数量';
COMMENT ON COLUMN cmx_model_deploy_history.seed_rows IS '初始数据行数';
COMMENT ON COLUMN cmx_model_deploy_history.def_ref IS '定义引用';
COMMENT ON COLUMN cmx_model_deploy_history.def_version IS '定义版本';
COMMENT ON COLUMN cmx_model_deploy_history.engine_version IS '引擎版本';
COMMENT ON COLUMN cmx_model_deploy_history.error_message IS '错误信息';
COMMENT ON COLUMN cmx_model_deploy_history.started_at IS '开始时间';
COMMENT ON COLUMN cmx_model_deploy_history.finished_at IS '完成时间';
COMMENT ON COLUMN cmx_model_deploy_history.duration_ms IS '耗时(毫秒)';
COMMENT ON COLUMN cmx_model_deploy_history.operator_id IS '操作人ID';
COMMENT ON COLUMN cmx_model_deploy_history.operator_name IS '操作人姓名';
COMMENT ON COLUMN cmx_model_deploy_history.create_time IS '创建时间';

-- =============================================
-- 41. 模型中心-源定义/初始数据留档表 (cmx_model_source)
-- =============================================
DROP TABLE IF EXISTS cmx_model_source;
CREATE TABLE cmx_model_source
(
    id               VARCHAR(64) NOT NULL,
    db_id            VARCHAR(100),
    app_id           VARCHAR(64) NOT NULL,
    domain_code      VARCHAR(100),
    application_code VARCHAR(100),
    module_code      VARCHAR(100),
    module_name      VARCHAR(200),
    kind             VARCHAR(20),
    version          VARCHAR(50),
    source_file      VARCHAR(300),
    source_json      JSONB,
    compiled_json    JSONB,
    checksum         VARCHAR(64),
    table_count      INT4       DEFAULT 0,
    seed_row_count   INT4       DEFAULT 0,
    is_current       INT4       DEFAULT 1,
    engine_version   VARCHAR(50),
    imported_at      TIMESTAMP  DEFAULT CURRENT_TIMESTAMP,
    imported_by      VARCHAR(100),
    imported_name    VARCHAR(100),
    remark           VARCHAR(500),
    create_time      TIMESTAMP  DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_model_source_ver ON cmx_model_source (db_id, app_id, domain_code, application_code, module_code, kind, version);
CREATE INDEX idx_model_source_current   ON cmx_model_source (db_id, domain_code, application_code, module_code, kind, is_current);

COMMENT ON TABLE  cmx_model_source IS '模型中心-源定义/初始数据 JSON 完整留档';
COMMENT ON COLUMN cmx_model_source.id IS '主键ID';
COMMENT ON COLUMN cmx_model_source.db_id IS '数据库ID';
COMMENT ON COLUMN cmx_model_source.app_id IS '应用ID';
COMMENT ON COLUMN cmx_model_source.domain_code IS '域编码';
COMMENT ON COLUMN cmx_model_source.application_code IS '应用编码';
COMMENT ON COLUMN cmx_model_source.module_code IS '模块编码';
COMMENT ON COLUMN cmx_model_source.module_name IS '模块名称';
COMMENT ON COLUMN cmx_model_source.kind IS '类别: DCT/DOC/SEED';
COMMENT ON COLUMN cmx_model_source.version IS '版本';
COMMENT ON COLUMN cmx_model_source.source_file IS '源文件路径';
COMMENT ON COLUMN cmx_model_source.source_json IS '源定义或初始数据 JSON 原文（完整保存，可复现/审计）';
COMMENT ON COLUMN cmx_model_source.compiled_json IS '编译后 JSON';
COMMENT ON COLUMN cmx_model_source.checksum IS '校验和';
COMMENT ON COLUMN cmx_model_source.table_count IS '表数量';
COMMENT ON COLUMN cmx_model_source.seed_row_count IS '初始数据行数';
COMMENT ON COLUMN cmx_model_source.is_current IS '是否当前版本：1-是，0-否';
COMMENT ON COLUMN cmx_model_source.engine_version IS '引擎版本';
COMMENT ON COLUMN cmx_model_source.imported_at IS '导入时间';
COMMENT ON COLUMN cmx_model_source.imported_by IS '导入人ID';
COMMENT ON COLUMN cmx_model_source.imported_name IS '导入人姓名';
COMMENT ON COLUMN cmx_model_source.remark IS '备注';
COMMENT ON COLUMN cmx_model_source.create_time IS '创建时间';

-- =============================================
-- 42. 模型中心-主控库跨库总览表 (cmx_model_registry)
-- =============================================
DROP TABLE IF EXISTS cmx_model_registry;
CREATE TABLE cmx_model_registry
(
    id               VARCHAR(64) NOT NULL,
    db_id            VARCHAR(100) NOT NULL,
    db_name          VARCHAR(200),
    db_type          VARCHAR(30),
    app_id           VARCHAR(64) NOT NULL,
    initialized      INT4        DEFAULT 0,
    meta_version     INT4,
    module_count     INT4        DEFAULT 0,
    table_count      INT4        DEFAULT 0,
    modules_summary  JSONB,
    last_deploy_at   TIMESTAMP,
    last_sync_at     TIMESTAMP,
    health           VARCHAR(20) DEFAULT 'unknown',
    create_time      TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_model_registry_db ON cmx_model_registry (db_id, app_id);

COMMENT ON TABLE  cmx_model_registry IS '主控库-各目标数据库模型部署总览（区分不同数据库）';
COMMENT ON COLUMN cmx_model_registry.id IS '主键ID';
COMMENT ON COLUMN cmx_model_registry.db_id IS '数据库ID';
COMMENT ON COLUMN cmx_model_registry.db_name IS '数据库名称';
COMMENT ON COLUMN cmx_model_registry.db_type IS '数据库类型';
COMMENT ON COLUMN cmx_model_registry.app_id IS '应用ID';
COMMENT ON COLUMN cmx_model_registry.initialized IS '是否已初始化：0-否，1-是';
COMMENT ON COLUMN cmx_model_registry.meta_version IS '台账 schema 版本';
COMMENT ON COLUMN cmx_model_registry.module_count IS '模块数量';
COMMENT ON COLUMN cmx_model_registry.table_count IS '表数量';
COMMENT ON COLUMN cmx_model_registry.modules_summary IS '各模块×kind 版本与状态摘要（供总览页免逐库查）';
COMMENT ON COLUMN cmx_model_registry.last_deploy_at IS '最近部署时间';
COMMENT ON COLUMN cmx_model_registry.last_sync_at IS '最近同步时间';
COMMENT ON COLUMN cmx_model_registry.health IS '健康状态: unknown/healthy/warning/error';
COMMENT ON COLUMN cmx_model_registry.create_time IS '创建时间';
COMMENT ON COLUMN cmx_model_registry.update_time IS '更新时间';

-- =============================================
-- 42. 业务单据版本化-整单快照表 (cmx_doc_revision)
-- =============================================
DROP TABLE IF EXISTS cmx_doc_revision;
CREATE TABLE cmx_doc_revision
(
    id             BIGINT       NOT NULL,
    doc_file       VARCHAR(200) NOT NULL,
    root_table     VARCHAR(100) NOT NULL,
    root_id        VARCHAR(64)  NOT NULL,
    rev_no         INT4         NOT NULL,
    is_current     INT4         NOT NULL DEFAULT 1,
    op             VARCHAR(16),
    snapshot       JSONB        NOT NULL,
    change_summary JSONB,
    reason         VARCHAR(500),
    actor_id       VARCHAR(64),
    actor_name     VARCHAR(100),
    biz_status     VARCHAR(32),
    created_at     TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

COMMENT ON TABLE  cmx_doc_revision            IS '业务单据版本化：整单 JSONB 快照（append-only，方案 §6A）';
COMMENT ON COLUMN cmx_doc_revision.id          IS '版本记录主键（雪花）';
COMMENT ON COLUMN cmx_doc_revision.doc_file    IS '单据定义（哪种单据）';
COMMENT ON COLUMN cmx_doc_revision.root_table  IS '根层表名（如 cv_batch）';
COMMENT ON COLUMN cmx_doc_revision.root_id     IS '单据根行 id（字符串化）';
COMMENT ON COLUMN cmx_doc_revision.rev_no      IS '该单第几版（1,2,3...）';
COMMENT ON COLUMN cmx_doc_revision.is_current  IS '是否当前版（同 root 仅一行为 1）';
COMMENT ON COLUMN cmx_doc_revision.op          IS '操作: create/update/delete/restore';
COMMENT ON COLUMN cmx_doc_revision.snapshot    IS '整单列式包快照（前端 fromJSON 可直接还原）';
COMMENT ON COLUMN cmx_doc_revision.change_summary IS '本版变更摘要';
COMMENT ON COLUMN cmx_doc_revision.reason      IS '变更原因（reason_required 时必填）';
COMMENT ON COLUMN cmx_doc_revision.actor_id    IS '操作者 id';
COMMENT ON COLUMN cmx_doc_revision.actor_name  IS '操作者名';
COMMENT ON COLUMN cmx_doc_revision.biz_status  IS '冗余当时单据状态，便于按态检索';
COMMENT ON COLUMN cmx_doc_revision.created_at  IS '创建时间';

CREATE UNIQUE INDEX uk_doc_rev     ON cmx_doc_revision (doc_file, root_id, rev_no);
CREATE INDEX        idx_doc_rev_cur ON cmx_doc_revision (doc_file, root_id, is_current);
CREATE INDEX        idx_doc_rev_time ON cmx_doc_revision (root_id, created_at);

-- =============================================
-- 43. 业务单据版本化-字段级变更明细表 (cmx_doc_change)
-- =============================================
DROP TABLE IF EXISTS cmx_doc_change;
CREATE TABLE cmx_doc_change
(
    id         BIGINT       NOT NULL,
    rev_id     BIGINT       NOT NULL,
    root_id    VARCHAR(64)  NOT NULL,
    layer      VARCHAR(100),
    row_id     VARCHAR(64),
    op         VARCHAR(8),
    field      VARCHAR(100),
    old_value  JSONB,
    new_value  JSONB,
    actor_id   VARCHAR(64),
    created_at TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

COMMENT ON TABLE  cmx_doc_change       IS '业务单据字段级变更明细（审计，方案 §6A.3）';
COMMENT ON COLUMN cmx_doc_change.id     IS '主键ID（雪花）';
COMMENT ON COLUMN cmx_doc_change.rev_id  IS '所属版本 cmx_doc_revision.id';
COMMENT ON COLUMN cmx_doc_change.root_id IS '单据根行 id（字符串化）';
COMMENT ON COLUMN cmx_doc_change.layer   IS '层表名';
COMMENT ON COLUMN cmx_doc_change.row_id  IS '变更的行';
COMMENT ON COLUMN cmx_doc_change.op      IS 'I/U/D';
COMMENT ON COLUMN cmx_doc_change.field   IS '变更字段（U 时逐字段一行）';
COMMENT ON COLUMN cmx_doc_change.old_value IS '旧值';
COMMENT ON COLUMN cmx_doc_change.new_value IS '新值';
COMMENT ON COLUMN cmx_doc_change.actor_id  IS '操作者 id';
COMMENT ON COLUMN cmx_doc_change.created_at IS '创建时间';

CREATE INDEX idx_doc_change_rev ON cmx_doc_change (rev_id);
CREATE INDEX idx_doc_change_row ON cmx_doc_change (root_id, row_id, field);

-- ================================================================
-- cmx-flow 流程引擎（M1 + M2）：运行态(RU) 3 表 + 历史态(HI) 2 表
-- 无 FOREIGN KEY（关联字段 + 索引替代）；实例终态时同事务归档到 HI 表。
-- 详见 docs/sql/migrations/20260717_001_cmx_flow_engine_tables.up.sql
-- ================================================================

-- 流程实例（运行态聚合根）
CREATE TABLE IF NOT EXISTS cmx_flow_instance
(
    id                  VARCHAR(64)  NOT NULL,
    definition_key      VARCHAR(128) NOT NULL,
    business_key        VARCHAR(128),
    state               VARCHAR(16)  NOT NULL,
    variables           JSONB        NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ  NOT NULL,
    updated_at          TIMESTAMPTZ  NOT NULL,
    ended_at            TIMESTAMPTZ,
    org_id              VARCHAR(64),
    parent_instance_id  VARCHAR(64),
    parent_token_id     VARCHAR(64),
    parent_node_bpmn_id VARCHAR(128),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_instance                    IS '流程实例（运行态聚合根）';
COMMENT ON COLUMN cmx_flow_instance.state              IS '实例状态：ACTIVE / COMPLETED / TERMINATED';
COMMENT ON COLUMN cmx_flow_instance.variables          IS '实例级流程变量（JSONB 动态 KV）';
COMMENT ON COLUMN cmx_flow_instance.parent_instance_id IS '父实例 id（M5 子流程：子实例指向主实例；主实例为 NULL）';
COMMENT ON COLUMN cmx_flow_instance.parent_token_id    IS '父实例中挂起等待的令牌 id（子完成时精确唤醒）';
COMMENT ON COLUMN cmx_flow_instance.parent_node_bpmn_id IS '父实例中发起本子实例的 callActivity 节点 bpmn id（M5.3 多挂载去重键；单挂载恒空）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_defkey ON cmx_flow_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_bizkey ON cmx_flow_instance (business_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_state  ON cmx_flow_instance (state);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_parent ON cmx_flow_instance (parent_instance_id);

-- 流程令牌（执行指针）
CREATE TABLE IF NOT EXISTS cmx_flow_token
(
    id           VARCHAR(64)  NOT NULL,
    instance_id  VARCHAR(64)  NOT NULL,
    node_bpmn_id VARCHAR(128) NOT NULL,
    state        VARCHAR(16)  NOT NULL,
    parent_id    VARCHAR(64),
    created_at   TIMESTAMPTZ  NOT NULL,
    updated_at   TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_token       IS '流程令牌（执行指针；一实例多令牌）';
COMMENT ON COLUMN cmx_flow_token.state IS '令牌状态：ACTIVE / WAITING / JOINING / WAITING_SUBFLOW / ENDED';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_token_instance ON cmx_flow_token (instance_id);

-- 用户任务（等待态外化）
CREATE TABLE IF NOT EXISTS cmx_flow_task
(
    id               VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    token_id         VARCHAR(64)  NOT NULL,
    node_bpmn_id     VARCHAR(128) NOT NULL,
    name             VARCHAR(255),
    assignee         VARCHAR(128),
    candidate_groups VARCHAR(512),
    element_value    JSONB,
    owner_user_id    VARCHAR(64),
    parent_task_id   VARCHAR(64),
    delegation_state VARCHAR(16),
    completed        BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ  NOT NULL,
    completed_at     TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task IS '用户任务（userTask 等待态的外化产物）';
COMMENT ON COLUMN cmx_flow_task.element_value    IS '多实例子任务携带的当前元素（会签每人各自数据；单实例为 NULL）';
COMMENT ON COLUMN cmx_flow_task.owner_user_id    IS 'M4.3 任务所有者（委派时 ≠ assignee）';
COMMENT ON COLUMN cmx_flow_task.parent_task_id   IS 'M4.3 父任务（加签临时任务指向原任务）';
COMMENT ON COLUMN cmx_flow_task.delegation_state IS 'M4.3 转签状态：NULL/DELEGATED/ADDSIGN/SUSPENDED';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_instance ON cmx_flow_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_assignee ON cmx_flow_task (assignee);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_open     ON cmx_flow_task (assignee, completed);

-- 历史实例（终态归档）
CREATE TABLE IF NOT EXISTS cmx_flow_hi_instance
(
    id             VARCHAR(64)  NOT NULL,
    definition_key VARCHAR(128) NOT NULL,
    business_key   VARCHAR(128),
    state          VARCHAR(16)  NOT NULL,
    variables      JSONB        NOT NULL DEFAULT '{}'::jsonb,
    created_at     TIMESTAMPTZ  NOT NULL,
    ended_at       TIMESTAMPTZ,
    duration_ms    BIGINT,
    archived_at    TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_hi_instance             IS '历史流程实例（终态归档，供审计/查询）';
COMMENT ON COLUMN cmx_flow_hi_instance.duration_ms IS '实例存续时长（毫秒）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_defkey ON cmx_flow_hi_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_bizkey ON cmx_flow_hi_instance (business_key);

-- 历史任务（办结归档）
CREATE TABLE IF NOT EXISTS cmx_flow_hi_task
(
    id           VARCHAR(64)  NOT NULL,
    instance_id  VARCHAR(64)  NOT NULL,
    node_bpmn_id VARCHAR(128) NOT NULL,
    name         VARCHAR(255),
    assignee     VARCHAR(128),
    created_at   TIMESTAMPTZ  NOT NULL,
    completed_at TIMESTAMPTZ,
    duration_ms  BIGINT,
    archived_at  TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_hi_task             IS '历史用户任务（办结归档，供工时分析/审计）';
COMMENT ON COLUMN cmx_flow_hi_task.duration_ms IS '任务办理时长（毫秒）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_instance ON cmx_flow_hi_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_assignee ON cmx_flow_hi_task (assignee);

-- 多实例执行域（M3：会签/或签账本；详见 migrations/20260717_002_cmx_flow_multi_instance.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_mi_scope
(
    id                   VARCHAR(64)  NOT NULL,
    instance_id          VARCHAR(64)  NOT NULL,
    node_bpmn_id         VARCHAR(128) NOT NULL,
    sequential           BOOLEAN      NOT NULL DEFAULT FALSE,
    total                INTEGER      NOT NULL,
    completed            INTEGER      NOT NULL DEFAULT 0,
    next_index           INTEGER      NOT NULL DEFAULT 0,
    collection           JSONB        NOT NULL DEFAULT '[]'::jsonb,
    element_var          VARCHAR(128),
    completion_condition VARCHAR(512),
    finished             BOOLEAN      NOT NULL DEFAULT FALSE,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_mi_scope             IS '多实例执行域（会签/或签的一次展开：计数与游标账本）';
COMMENT ON COLUMN cmx_flow_mi_scope.sequential  IS 'true=顺序(或签)；false=并行(会签)';
COMMENT ON COLUMN cmx_flow_mi_scope.total       IS '子实例总数（nrOfInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.completed   IS '已办结子实例数（nrOfCompletedInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.finished    IS '本域是否已收口（完成条件命中或全部完成）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_mi_scope_instance ON cmx_flow_mi_scope (instance_id);

-- 定时器作业（M2.5：边界定时器到期表；详见 migrations/20260717_003_cmx_flow_job.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_job
(
    id               VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    token_id         VARCHAR(64)  NOT NULL,
    boundary_bpmn_id VARCHAR(128) NOT NULL,
    cancel_activity  BOOLEAN      NOT NULL DEFAULT TRUE,
    due_at           TIMESTAMPTZ  NOT NULL,
    created_at       TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_job                 IS '定时器作业（边界定时器到期表）';
COMMENT ON COLUMN cmx_flow_job.token_id        IS '挂载令牌 id（停在宿主 userTask）；令牌离开即撤销作业';
COMMENT ON COLUMN cmx_flow_job.cancel_activity IS 'true=中断型；false=非中断型';
COMMENT ON COLUMN cmx_flow_job.due_at          IS '到期时刻（宿主到达 + 时长）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_instance ON cmx_flow_job (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_due      ON cmx_flow_job (due_at);

-- 组织/岗位（M4.1：IAM 补齐；详见 migrations/20260718_001_cmx_flow_identity.up.sql）
CREATE TABLE IF NOT EXISTS cmx_org
(
    id             VARCHAR(64)  NOT NULL,
    code           VARCHAR(100) NOT NULL,
    name           VARCHAR(100) NOT NULL,
    parent_id      VARCHAR(64),
    path           VARCHAR(500),
    leader_user_id VARCHAR(64),
    sort_order     INT4      DEFAULT 0,
    status         INT4      DEFAULT 1,
    archived       INT4      DEFAULT 0,
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE cmx_org IS '组织/部门表（树形；补齐 cmx_user.org_id 引用）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_org_code ON cmx_org (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_org_parent ON cmx_org (parent_id);

CREATE TABLE IF NOT EXISTS cmx_position
(
    id          VARCHAR(64)  NOT NULL,
    code        VARCHAR(100) NOT NULL,
    name        VARCHAR(100) NOT NULL,
    org_id      VARCHAR(64),
    level       INT4      DEFAULT 0,
    sort_order  INT4      DEFAULT 0,
    status      INT4      DEFAULT 1,
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE cmx_position IS '岗位表（组织内职位，与角色正交）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_position_code ON cmx_position (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_position_org ON cmx_position (org_id);

CREATE TABLE IF NOT EXISTS cmx_user_position
(
    id          VARCHAR(64) NOT NULL,
    user_id     VARCHAR(64) NOT NULL,
    position_id VARCHAR(64) NOT NULL,
    is_primary  BOOLEAN   DEFAULT FALSE,
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE cmx_user_position IS '用户-岗位关联表（一人可多岗）';
CREATE INDEX IF NOT EXISTS idx_cmx_user_position_user ON cmx_user_position (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_position_pos  ON cmx_user_position (position_id);

-- 任务候选人池（M4.1：多人候选待认领）
CREATE TABLE IF NOT EXISTS cmx_flow_task_candidate
(
    id               VARCHAR(64)  NOT NULL,
    task_id          VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    candidate_type   VARCHAR(16)  NOT NULL,
    candidate_ref    VARCHAR(128) NOT NULL,
    resolved_user_id VARCHAR(64)  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task_candidate                IS '任务候选人池（多人候选待认领；随快照全删重插）';
COMMENT ON COLUMN cmx_flow_task_candidate.candidate_type IS '候选来源：USER / ROLE / POSITION / ORG';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_instance ON cmx_flow_task_candidate (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_user     ON cmx_flow_task_candidate (resolved_user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_task     ON cmx_flow_task_candidate (task_id);

-- 抄送记录（M4.2：知会 + 已读；详见 migrations/20260718_002_cmx_flow_cc.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_cc
(
    id            VARCHAR(64)  NOT NULL,
    instance_id   VARCHAR(64)  NOT NULL,
    node_bpmn_id  VARCHAR(128),
    to_user_id    VARCHAR(64)  NOT NULL,
    from_user_id  VARCHAR(64),
    reason        VARCHAR(500),
    read_at       TIMESTAMPTZ,
    created_at    TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_cc            IS '抄送记录表（只读知会 + 已读追踪；不阻塞流程）';
COMMENT ON COLUMN cmx_flow_cc.read_at    IS '已读时刻（NULL = 未读）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_cc_instance ON cmx_flow_cc (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_cc_to_user  ON cmx_flow_cc (to_user_id, read_at);

-- 转签台账（M4.3：转办/加签/委派；详见 migrations/20260718_003_cmx_flow_delegation.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_task_delegation
(
    id           VARCHAR(64)  NOT NULL,
    task_id      VARCHAR(64)  NOT NULL,
    instance_id  VARCHAR(64)  NOT NULL,
    kind         VARCHAR(20)  NOT NULL,
    from_user_id VARCHAR(64)  NOT NULL,
    to_user_id   VARCHAR(64)  NOT NULL,
    temp_task_id VARCHAR(64),
    reason       VARCHAR(500),
    created_at   TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task_delegation      IS '转签台账（转办/加签/委派流转链）';
COMMENT ON COLUMN cmx_flow_task_delegation.kind IS 'TRANSFER / ADDSIGN_BEFORE / ADDSIGN_AFTER / DELEGATE';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_instance ON cmx_flow_task_delegation (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_task     ON cmx_flow_task_delegation (task_id);

-- 子流程组织绑定（M5.2：逻辑 key + 组织 → 具体子流程；详见 migrations/20260718_005_cmx_flow_subflow_binding.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_subflow_binding
(
    id                    VARCHAR(64)  NOT NULL,
    called_key            VARCHAR(128) NOT NULL,
    org_id                VARCHAR(64),
    target_definition_key VARCHAR(128) NOT NULL,
    enabled               BOOLEAN      NOT NULL DEFAULT TRUE,
    remark                VARCHAR(500),
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_subflow_binding       IS '子流程组织绑定（逻辑 key + 组织 → 具体子流程；M5.2 路由数据源）';
COMMENT ON COLUMN cmx_flow_subflow_binding.org_id IS '适用组织（NULL = 默认兜底绑定）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_key ON cmx_flow_subflow_binding (called_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_org ON cmx_flow_subflow_binding (org_id);

-- ================================================================
-- cmx-flow 流程定义持久化层（设计器 阶段0）
-- 含 DAM 三段列 + 版本变更说明列；详见 migrations/20260718_007/009/010
-- ================================================================

-- 流程定义主记录（当前指针：草稿 XML + 已发布版本指向）
CREATE TABLE IF NOT EXISTS cmx_flow_definition (
    key            VARCHAR(128) PRIMARY KEY,
    name           VARCHAR(255) NOT NULL,
    module         VARCHAR(64),
    category       VARCHAR(64),
    state          VARCHAR(16)  NOT NULL DEFAULT 'DRAFT',
    active_version INTEGER,
    draft_xml      TEXT,
    domain         VARCHAR(64),
    application    VARCHAR(64),
    updated_at     TIMESTAMPTZ  NOT NULL,
    updated_by     VARCHAR(64)
);
COMMENT ON TABLE  cmx_flow_definition                IS '流程定义主记录（当前指针：草稿 XML + 已发布版本指向）';
COMMENT ON COLUMN cmx_flow_definition.key            IS '流程定义 key（= BPMN process id）';
COMMENT ON COLUMN cmx_flow_definition.state          IS '状态：DRAFT / PUBLISHED';
COMMENT ON COLUMN cmx_flow_definition.active_version IS '当前已发布版本号（未发布为 NULL）';
COMMENT ON COLUMN cmx_flow_definition.draft_xml      IS '当前草稿的 BPMN XML（设计器产物）';
COMMENT ON COLUMN cmx_flow_definition.domain         IS '所属域（DAM 三段之一，如 fi）';
COMMENT ON COLUMN cmx_flow_definition.application    IS '所属应用（DAM 三段之一，如 cmxfico）';
COMMENT ON COLUMN cmx_flow_definition.module         IS '所属模块（DAM 三段之一，如 gl）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_module ON cmx_flow_definition (module);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_state  ON cmx_flow_definition (state);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_dam    ON cmx_flow_definition (domain, application, module);

-- 流程定义版本历史（不可变，每次发布追加 BPMN 快照）
CREATE TABLE IF NOT EXISTS cmx_flow_definition_version (
    id           VARCHAR(64)  PRIMARY KEY,
    def_key      VARCHAR(128) NOT NULL,
    version      INTEGER      NOT NULL,
    bpmn_xml     TEXT         NOT NULL,
    note         VARCHAR(512),
    published_at TIMESTAMPTZ  NOT NULL,
    published_by VARCHAR(64)
);
COMMENT ON TABLE  cmx_flow_definition_version         IS '流程定义版本历史（不可变，每次发布追加 BPMN 快照）';
COMMENT ON COLUMN cmx_flow_definition_version.version IS '版本号（同 def_key 下从 1 递增）';
COMMENT ON COLUMN cmx_flow_definition_version.note    IS '本版本变更说明（发布时填写，可空）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_def_version ON cmx_flow_definition_version (def_key, version);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_def_version_key   ON cmx_flow_definition_version (def_key);

-- ================================================================
-- MDM 主数据治理表（平台级，不走 compile）
-- 含激活映射配置 / 版本留痕 / 交叉引用 / 值映射 / 匹配组 / 分发订阅 / 事件日志
-- 主键规约：cmx_ 平台表 VARCHAR(64) snowflake；md_ 治理表 BIGINT（承接 cm_*.id）
-- 无外键约束（关联字段 + 索引替代）
-- 详见 migrations/20260804_001_mdm_governance
-- ================================================================

-- 1. 激活映射配置（UI 配置器维护，激活器读取执行）
DROP TABLE IF EXISTS cmx_mdm_activation;
CREATE TABLE cmx_mdm_activation (
    id              VARCHAR(64)  NOT NULL,
    activation_code VARCHAR(64)  NOT NULL,
    source_doc_type VARCHAR(64)  NOT NULL,
    cr_type         VARCHAR(16)  NOT NULL,
    target_dict     VARCHAR(64)  NOT NULL,
    target_table    VARCHAR(64)  NOT NULL,
    header_mapping  JSONB        NOT NULL DEFAULT '{}'::jsonb,
    line_mappings   JSONB                 DEFAULT '{}'::jsonb,
    code_rule_code  VARCHAR(64),
    subject_name_field VARCHAR(64),
    subject_code_field VARCHAR(64),
    header_groups   JSONB        NOT NULL DEFAULT '[]'::jsonb,
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_mdm_activation IS 'MDM 激活映射配置（单据→主数据），UI 配置器维护，激活器读取执行';
COMMENT ON COLUMN cmx_mdm_activation.id              IS '主键（snowflake，应用层生成）';
COMMENT ON COLUMN cmx_mdm_activation.activation_code IS '映射码（如 supplier_apply）';
COMMENT ON COLUMN cmx_mdm_activation.source_doc_type IS '来源单据类型（如 mdm_supplier_apply）';
COMMENT ON COLUMN cmx_mdm_activation.cr_type         IS '变更类型 create/update/merge/block/flag_delete';
COMMENT ON COLUMN cmx_mdm_activation.target_dict     IS '目标头字典码（如 supplier）';
COMMENT ON COLUMN cmx_mdm_activation.target_table    IS '目标头物理表名（如 cm_supplier，配置器选字典时从 dct/meta tableName 一并写入，激活器直接用）';
COMMENT ON COLUMN cmx_mdm_activation.header_mapping  IS '头映射 {单据字段:主数据列}';
COMMENT ON COLUMN cmx_mdm_activation.line_mappings   IS '明细映射 [{lineType,targetDict,targetTable,parentIdField,fields}]';
COMMENT ON COLUMN cmx_mdm_activation.code_rule_code  IS 'code 由哪个编码规则生成（新建时，M8 接 cmx-code）';
COMMENT ON COLUMN cmx_mdm_activation.subject_name_field IS '主体名字段来源（payload 内字段名，前端按此填 subject_name）';
COMMENT ON COLUMN cmx_mdm_activation.subject_code_field IS '主体编码字段来源（为空则由 codeRule 铸号）';
COMMENT ON COLUMN cmx_mdm_activation.header_groups  IS '头映射分组(UI 展示用,[{groupCode,groupName,fields:[源字段名]}]);激活器不读,header_mapping 落库仍扁平';
COMMENT ON COLUMN cmx_mdm_activation.is_active       IS '是否启用';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_mdm_activation_code     ON cmx_mdm_activation (activation_code);
CREATE        INDEX IF NOT EXISTS idx_cmx_mdm_activation_doctype ON cmx_mdm_activation (source_doc_type, cr_type);

-- 2. 主数据版本留痕（激活器写入）
DROP TABLE IF EXISTS md_audit;
CREATE TABLE md_audit (
    id            BIGINT       NOT NULL,
    dict_code     VARCHAR(64)  NOT NULL,
    record_id     BIGINT       NOT NULL,
    version       INT          NOT NULL,
    action        VARCHAR(16)  NOT NULL,
    source_cr_id  BIGINT,
    field         VARCHAR(64),
    old_value     JSONB,
    new_value     JSONB,
    operated_by   BIGINT       NOT NULL,
    operated_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_audit IS '主数据版本留痕（激活器写入）';
COMMENT ON COLUMN md_audit.id            IS '主键（应用层生成）';
COMMENT ON COLUMN md_audit.dict_code     IS 'cm_* 字典码';
COMMENT ON COLUMN md_audit.record_id     IS 'cm_*.id（无物理FK）';
COMMENT ON COLUMN md_audit.version       IS '激活版本号';
COMMENT ON COLUMN md_audit.action        IS 'create/update/freeze/merge/archive';
COMMENT ON COLUMN md_audit.source_cr_id  IS '触发此变更的 CR 单据 cv_mdm_apply.id';
COMMENT ON COLUMN md_audit.field         IS '变更字段（变更场景）';
COMMENT ON COLUMN md_audit.old_value     IS '旧值';
COMMENT ON COLUMN md_audit.new_value     IS '新值';
COMMENT ON COLUMN md_audit.operated_by   IS '操作人ID';
CREATE INDEX IF NOT EXISTS idx_md_audit_record ON md_audit (dict_code, record_id, version);

-- 3. 交叉引用（Key Mapping）
DROP TABLE IF EXISTS md_xref;
CREATE TABLE md_xref (
    id             BIGINT       NOT NULL,
    dict_code      VARCHAR(64)  NOT NULL,
    record_id      BIGINT       NOT NULL,
    source_system  VARCHAR(64)  NOT NULL,
    source_ref     VARCHAR(128) NOT NULL,
    xref_status    VARCHAR(16)  NOT NULL DEFAULT 'active',
    confidence     SMALLINT     NOT NULL DEFAULT 50,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_xref IS '主数据交叉引用（Key Mapping）';
COMMENT ON COLUMN md_xref.id          IS '主键（应用层生成）';
COMMENT ON COLUMN md_xref.xref_status IS '引用状态 active/inactive';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_xref_src     ON md_xref (source_system, source_ref);
CREATE        INDEX IF NOT EXISTS idx_md_xref_record ON md_xref (dict_code, record_id);

-- 4. 值映射（Value Mapping）
DROP TABLE IF EXISTS md_value_map;
CREATE TABLE md_value_map (
    id        BIGINT       NOT NULL,
    field     VARCHAR(64)  NOT NULL,
    src_sys   VARCHAR(64)  NOT NULL,
    src_val   VARCHAR(128) NOT NULL,
    tgt_sys   VARCHAR(64)  NOT NULL,
    tgt_val   VARCHAR(128) NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_value_map IS '主数据值映射（Value Mapping）';
COMMENT ON COLUMN md_value_map.id IS '主键（应用层生成）';

-- 5. 查重规则配置（查重界面内维护，find-duplicates 读取执行）
DROP TABLE IF EXISTS md_match_config;
CREATE TABLE md_match_config (
    id             BIGINT       NOT NULL,
    rule_name      VARCHAR(128) NOT NULL,
    dict_code      VARCHAR(64)  NOT NULL,
    target_table   VARCHAR(64)  NOT NULL,
    specs          JSONB        NOT NULL DEFAULT '[]'::jsonb,
    cluster_keys   JSONB        NOT NULL DEFAULT '[]'::jsonb,
    survive_fields JSONB        NOT NULL DEFAULT '[]'::jsonb,
    thresholds     JSONB,
    is_active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_match_config IS '查重规则配置（按字典维度），查重界面内维护，find-duplicates 读取执行';
COMMENT ON COLUMN md_match_config.id IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_config.specs IS '比较字段 [{field,weight,kind:Exact|EditDistance}]';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_match_config_dict_rule ON md_match_config (dict_code, rule_name);
CREATE        INDEX IF NOT EXISTS idx_md_match_config_dict      ON md_match_config (dict_code);

-- 6. 匹配组/存活裁决
DROP TABLE IF EXISTS md_match_group;
CREATE TABLE md_match_group (
    id               BIGINT       NOT NULL,
    dict_code        VARCHAR(64)  NOT NULL,
    group_key        VARCHAR(256) NOT NULL,
    member_ids       JSONB        NOT NULL,
    master_id        BIGINT,
    score            SMALLINT     NOT NULL,
    decision         VARCHAR(16)  NOT NULL,
    survivorship_log JSONB,
    status           VARCHAR(16)  NOT NULL DEFAULT 'pending',
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_match_group IS '匹配组/存活裁决';
COMMENT ON COLUMN md_match_group.id     IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_group.status IS 'pending/auto_merged/reviewed/rejected';
CREATE INDEX IF NOT EXISTS idx_md_match_group_dict ON md_match_group (dict_code, status);

-- 7. 分发订阅
DROP TABLE IF EXISTS md_subscription;
CREATE TABLE md_subscription (
    id          BIGINT       NOT NULL,
    target_sys  VARCHAR(64)  NOT NULL,
    dict_code   VARCHAR(64)  NOT NULL,
    filter      JSONB,
    field_map   JSONB,
    channel     VARCHAR(16)  NOT NULL,
    active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_subscription IS '分发订阅配置';
COMMENT ON COLUMN md_subscription.id      IS '主键（应用层生成）';
COMMENT ON COLUMN md_subscription.channel IS '通道 event/rest/batch';

-- 8. 分发事件日志（激活器激活成功时写入；主键 VARCHAR(64) snowflake，seq 为有序拉取列非主键）
DROP TABLE IF EXISTS md_event_log;
CREATE TABLE md_event_log (
    id          VARCHAR(64)  NOT NULL,
    seq         BIGSERIAL    NOT NULL,
    dict_code   VARCHAR(64)  NOT NULL,
    record_id   BIGINT       NOT NULL,
    event_type  VARCHAR(16)  NOT NULL,
    payload     JSONB        NOT NULL,
    emitted_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_event_log IS '分发事件日志（delta，消费者按 seq 拉取）';
COMMENT ON COLUMN md_event_log.id         IS '主键（snowflake，应用层生成，对齐全库主键惯例）';
COMMENT ON COLUMN md_event_log.seq        IS '有序拉取序列（DB 自增，非主键，供消费者 delta 排序）';
COMMENT ON COLUMN md_event_log.event_type IS 'created/updated/merged';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_event_log_seq   ON md_event_log (seq);
CREATE        INDEX IF NOT EXISTS idx_md_event_log_dict ON md_event_log (dict_code, seq);

-- =====================================================
-- cmx-code 编码引擎（两张表合并迁移）
-- 1. cmx_code_rule  —— 编码规则库（纯算法：段序列，不带 target，可被多处复用）
-- 2. cmx_code_gap   —— 编码断号表（连号域空缺号回收，只存空缺 ≠ 已分配）
-- 规则按域/应用/模块（DAM）隔离，既有规则无 DAM 默认空串，兼容存量
-- =====================================================

-- ─────────────────────────────────────────────────────
-- 1. 编码规则库
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cmx_code_rule (
    id              BIGINT                  NOT NULL,
    rule_code       VARCHAR(64)             NOT NULL,
    rule_name       VARCHAR(128)            NOT NULL,
    mode            VARCHAR(16)             NOT NULL DEFAULT 'auto',
    org_scope       VARCHAR(64),
    condition       TEXT,
    segments        JSONB                   NOT NULL DEFAULT '[]',
    joiner          VARCHAR(4)              NOT NULL DEFAULT '',
    pattern         TEXT,
    enable_gap      BOOLEAN                 NOT NULL DEFAULT FALSE,
    use_sequence    BOOLEAN                 NOT NULL DEFAULT FALSE,
    valid_from      DATE,
    valid_to        DATE,
    priority        INT4                    NOT NULL DEFAULT 100,
    is_active       BOOLEAN                 NOT NULL DEFAULT TRUE,
    -- DAM 维度（域/应用/模块隔离，空串=兼容存量/全局可见）
    domain_code     VARCHAR(32)             NOT NULL DEFAULT '',
    application_code VARCHAR(32)            NOT NULL DEFAULT '',
    module_code     VARCHAR(32)             NOT NULL DEFAULT '',
    create_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived        INT4                    NOT NULL DEFAULT 0,
    create_by       VARCHAR(100),
    update_by       VARCHAR(100),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_code_rule_rule_code ON cmx_code_rule (rule_code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS ix_cmx_code_rule_active ON cmx_code_rule (is_active, priority);
CREATE INDEX IF NOT EXISTS ix_cmx_code_rule_archived ON cmx_code_rule (archived);
-- DAM + archived 复合索引：按模块过滤规则列表的主查询路径
CREATE INDEX IF NOT EXISTS ix_cmx_code_rule_dam ON cmx_code_rule (domain_code, application_code, module_code, archived);

COMMENT ON TABLE cmx_code_rule IS '编码规则库（纯算法，不带 target，可被多处复用）';
COMMENT ON COLUMN cmx_code_rule.id IS '主键ID（pk52）';
COMMENT ON COLUMN cmx_code_rule.rule_code IS '规则码（人类可读，全局唯一，如 supplier_hq）';
COMMENT ON COLUMN cmx_code_rule.rule_name IS '规则名称（展示用）';
COMMENT ON COLUMN cmx_code_rule.mode IS '模式：auto（引擎生成）| manual（用户手敲，引擎只校验）';
COMMENT ON COLUMN cmx_code_rule.org_scope IS '受控组织（可选，逗号分隔多组织，组织命中才生效）';
COMMENT ON COLUMN cmx_code_rule.condition IS '适用条件（JSON 算子 {"eq":[...]} 或字符串 field==value，可选）';
COMMENT ON COLUMN cmx_code_rule.segments IS '段序列 JSON（auto 必填）';
COMMENT ON COLUMN cmx_code_rule.joiner IS '段间连接符（默认空串）';
COMMENT ON COLUMN cmx_code_rule.pattern IS '校验正则（可选，manual 兜底 + auto 结果校验）';
COMMENT ON COLUMN cmx_code_rule.enable_gap IS '是否启用断号补偿（连号域才开，默认关）';
COMMENT ON COLUMN cmx_code_rule.use_sequence IS '是否使用 PG SEQUENCE 兜底（极端高并发可选，默认关）';
COMMENT ON COLUMN cmx_code_rule.valid_from IS '规则版本化·生效起始日期';
COMMENT ON COLUMN cmx_code_rule.valid_to IS '规则版本化·生效结束日期';
COMMENT ON COLUMN cmx_code_rule.priority IS '多规则选优（取大，默认 100）';
COMMENT ON COLUMN cmx_code_rule.is_active IS '是否启用';
COMMENT ON COLUMN cmx_code_rule.domain_code IS '所属域编码（如 fi），空串=兼容存量/全局可见';
COMMENT ON COLUMN cmx_code_rule.application_code IS '所属应用编码（如 cmxfico）';
COMMENT ON COLUMN cmx_code_rule.module_code IS '所属模块编码（如 gl）';

-- ─────────────────────────────────────────────────────
-- 2. 编码断号表
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cmx_code_gap (
    id              BIGINT                  NOT NULL,
    prefix          VARCHAR(128)            NOT NULL,
    serial_val      BIGINT                  NOT NULL,
    width           INT4                    NOT NULL,
    create_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id)
);

-- 按前缀查断号（take_gap 取最小断号）
CREATE INDEX IF NOT EXISTS ix_cmx_code_gap_prefix ON cmx_code_gap (prefix, serial_val);

COMMENT ON TABLE cmx_code_gap IS '编码断号表（只存空缺，≠已分配；连号域 enable_gap=true 才启用）';
COMMENT ON COLUMN cmx_code_gap.id IS '主键ID（pk52）';
COMMENT ON COLUMN cmx_code_gap.prefix IS '断号所属前缀（如 FV20260804）';
COMMENT ON COLUMN cmx_code_gap.serial_val IS '断号流水值（如 8）';
COMMENT ON COLUMN cmx_code_gap.width IS '流水宽度（补零用）';

-- ─────────────────────────────────────────────────────
-- 3. 编码发号序列表
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cmx_code_seq (
    id              BIGINT                  NOT NULL,
    rule_code       VARCHAR(64)             NOT NULL,
    prefix          VARCHAR(128)            NOT NULL,
    current_val     BIGINT                  NOT NULL DEFAULT 0,
    width           INT4                    NOT NULL DEFAULT 4,
    update_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_code_seq_prefix ON cmx_code_seq (rule_code, prefix);

COMMENT ON TABLE cmx_code_seq IS '编码发号序列表（集群安全发号源，use_sequence=true 才启用）';
COMMENT ON COLUMN cmx_code_seq.id IS '主键ID（pk52）';
COMMENT ON COLUMN cmx_code_seq.rule_code IS '关联 cmx_code_rule.rule_code';
COMMENT ON COLUMN cmx_code_seq.prefix IS '发号分组键（含 reset_key，如 FV20260804）';
COMMENT ON COLUMN cmx_code_seq.current_val IS '已发到的最大流水值（0=首启未探测）';
COMMENT ON COLUMN cmx_code_seq.width IS '流水宽度（补零用，记录首次发号时的宽度）';
