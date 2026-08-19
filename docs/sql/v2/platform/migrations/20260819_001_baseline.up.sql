-- ============================================================
-- 迁移说明：平台库（主库）基线迁移 —— 全量表结构 + 内置数据
-- 影响表：全部 cmx_ 平台表（不含 cmx_flow_* 流程运行态表，彼等建业务库）
-- 操作类型：CREATE TABLE/INDEX IF NOT EXISTS + 对齐 ALTER + INSERT ON CONFLICT（无损幂等）
-- 回滚方式：无独立 down（基线不做回滚；如需清库见旧版归档 DROP 重建方式）
-- 说明：本文件 = platform/init_ddl.sql + platform/init_dml.sql 合并。
--       新环境执行本基线即建成全量；已有库执行为幂等 no-op / 结构补齐。
-- ============================================================

-- ============================================================
-- CMX 平台库（主库）全量 DDL（基线内嵌版）
--
-- 与 init_ddl.sql 的差异：每表区块内多一段「结构对齐 ALTER」——
-- 表结构停在旧链中途的存量库，建表语句是 no-op 不补列，须先由对齐区
-- 补齐到终态，后续 COMMENT / 种子引用新列才不报错；新库则全部即建即过。
-- 每表区块布局：CREATE TABLE → 结构对齐 ALTER → COMMENT → 索引
-- ============================================================

CREATE TABLE IF NOT EXISTS cmx_domain
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_core_domain_code ON cmx_domain (code);

-- =============================================
-- 2. 应用表 (cmx_application)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_application
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_coreapplication_code ON cmx_application (code);

-- =============================================
-- 3. 模块表 (cmx_module)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_module
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_coremodule_code ON cmx_module (code);

-- =============================================
-- 4. 数据源表 (cmx_sys_datasource)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_sys_datasource
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


DROP INDEX IF EXISTS uk_cmx_datasource_db_id;

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

CREATE INDEX IF NOT EXISTS idx_datasource_domain_app_module ON cmx_sys_datasource (domain_code, application_code, module_code);

-- =============================================
-- 5. 插件注册表 (cmx_plugin)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_plugin
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

CREATE INDEX IF NOT EXISTS idx_plugin_domain_app_module ON cmx_plugin (domain_code, application_code, module_code);
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_plugin_app_plugin ON cmx_plugin (app_id, plugin_id);
CREATE INDEX IF NOT EXISTS idx_plugin_app_id ON cmx_plugin (app_id);

-- =============================================
-- 6. 版本历史表 (cmx_plugin_versions)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_plugin_versions
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

CREATE INDEX IF NOT EXISTS idx_version_plugin ON cmx_plugin_versions (plugin_id);
CREATE INDEX IF NOT EXISTS idx_plugin_versions_app_id ON cmx_plugin_versions (app_id);
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_plugin_versions_plugin_version ON cmx_plugin_versions (plugin_id, app_id, version);

-- =============================================
-- 7. 依赖关系表 (cmx_plugin_dependencies)
-- =============================================
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
-- -- =============================================
-- -- 8. 节点部署记录表 (cmx_plugin_deployments)
-- -- =============================================
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
--     ADD CONSTRAINT uk_cmx_plugin_deployments_plugin_node_version UNIQUE (plugin_id, node_id, version);

-- =============================================
-- 9. 审计日志表 (cmx_plugin_audit_log)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_plugin_audit_log
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

CREATE INDEX IF NOT EXISTS idx_audit_plugin ON cmx_plugin_audit_log (plugin_id);
CREATE INDEX IF NOT EXISTS idx_audit_node ON cmx_plugin_audit_log (node_id);
CREATE INDEX IF NOT EXISTS idx_audit_operation ON cmx_plugin_audit_log (operation_type);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON cmx_plugin_audit_log (started_at);
CREATE INDEX IF NOT EXISTS idx_audit_request ON cmx_plugin_audit_log (request_id);
CREATE INDEX IF NOT EXISTS idx_audit_app_id ON cmx_plugin_audit_log (app_id);

-- =============================================
-- 10. 通用审计日志表 (cmx_audit_log)
-- 记录 Auth/Iam/Plugin/Biz 四个域的通用审计事件
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_audit_log
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

CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_app_id   ON cmx_audit_log (app_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_domain   ON cmx_audit_log (domain);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_actor    ON cmx_audit_log (actor_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_target   ON cmx_audit_log (target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_request  ON cmx_audit_log (request_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_started  ON cmx_audit_log (started_at);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_archived ON cmx_audit_log (archived);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_result   ON cmx_audit_log (result);

-- -- =============================================
-- -- 10. 系统默认插件配置表 (cmx_system_plugins)
-- -- =============================================
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
-- -- =============================================
-- -- 11. 节点信息表 (cmx_plugin_nodes)
-- -- =============================================
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
-- -- =============================================
-- -- 12. 插件功能表 (cmx_plugin_features)
-- -- =============================================
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

-- =============================================
-- 13. 表定义元数据表 (cmx_meta_table_define)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_meta_table_define
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

CREATE INDEX IF NOT EXISTS idx_meta_table_define_app_id ON cmx_meta_table_define (app_id);

-- =============================================
-- 14. 表定义元数据版本表 (cmx_meta_table_define_version)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_meta_table_define_version
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

CREATE INDEX IF NOT EXISTS idx_meta_table_define_version_app_id ON cmx_meta_table_define_version (app_id);

-- =============================================
-- 15. 服务定义表 (cmx_service_define)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_service_define
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_service_define_app_key ON cmx_service_define (app_id, service_key);
CREATE INDEX IF NOT EXISTS idx_service_define_app_id ON cmx_service_define (app_id);

-- =============================================
-- 16. 服务定义版本表 (cmx_service_define_version)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_service_define_version
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

CREATE INDEX IF NOT EXISTS cmx_service_define_version_service_key_index ON cmx_service_define_version (service_key);
CREATE INDEX IF NOT EXISTS idx_service_define_version_app_id ON cmx_service_define_version (app_id);

-- =============================================
-- 17. 插件市场 - 插件主表 (cmx_marketplace_plugin)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_plugin
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_marketplace_plugin_plugin_id ON cmx_marketplace_plugin (plugin_id);

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

CREATE INDEX IF NOT EXISTS idx_mp_category ON cmx_marketplace_plugin (category);
CREATE INDEX IF NOT EXISTS idx_mp_status ON cmx_marketplace_plugin (status);
CREATE INDEX IF NOT EXISTS idx_mp_featured ON cmx_marketplace_plugin (is_featured) WHERE is_featured = 1;
CREATE INDEX IF NOT EXISTS idx_mp_download_count ON cmx_marketplace_plugin (download_count DESC);
CREATE INDEX IF NOT EXISTS idx_mp_rating ON cmx_marketplace_plugin (avg_rating DESC);

-- =============================================
-- 18. 插件市场 - 版本表 (cmx_marketplace_plugin_version)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_plugin_version
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_marketplace_plugin_version_pver ON cmx_marketplace_plugin_version (plugin_id, version);

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

CREATE INDEX IF NOT EXISTS idx_mpv_plugin_id ON cmx_marketplace_plugin_version (plugin_id);
CREATE INDEX IF NOT EXISTS idx_mpv_latest ON cmx_marketplace_plugin_version (plugin_id, is_latest) WHERE is_latest = 1;

-- =============================================
-- 19. 插件市场 - 下载统计表 (cmx_marketplace_download_stats)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_download_stats
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_marketplace_dstats_unique ON cmx_marketplace_download_stats (plugin_id, version, download_date, source_type);

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

CREATE INDEX IF NOT EXISTS idx_dstats_date ON cmx_marketplace_download_stats (download_date);

-- =============================================
-- 20. 插件市场 - 评分表 (cmx_marketplace_rating)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_marketplace_rating
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_marketplace_rating_unique ON cmx_marketplace_rating (plugin_id, user_id);

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

CREATE INDEX IF NOT EXISTS idx_rating_plugin ON cmx_marketplace_rating (plugin_id);

-- =============================================
-- 21. 文件详情表 (cmx_file_detail)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_file_detail
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

CREATE INDEX IF NOT EXISTS idx_file_detail_platform ON cmx_file_detail (platform);
CREATE INDEX IF NOT EXISTS idx_file_detail_object_type ON cmx_file_detail (object_type);
CREATE INDEX IF NOT EXISTS idx_file_detail_upload_id ON cmx_file_detail (upload_id);

-- =============================================
-- 22. 文件分片信息表 (cmx_file_part_detail)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_file_part_detail
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

CREATE INDEX IF NOT EXISTS idx_file_part_detail_upload_id ON cmx_file_part_detail (upload_id);

-- =============================================
-- 23. OAuth2 客户端表 (cmx_auth_client)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_auth_client
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_auth_client DROP CONSTRAINT IF EXISTS uk_cmx_auth_client_client_id;
DROP INDEX IF EXISTS uk_cmx_auth_client_client_id;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_client_client_id ON cmx_auth_client (client_id) WHERE archived = 0;

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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_client_client_id ON cmx_auth_client (client_id) WHERE archived = 0;

-- =============================================
-- 25. API Key 表 (cmx_auth_api_key)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_auth_api_key
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_api_key_prefix ON cmx_auth_api_key (key_prefix);
CREATE INDEX IF NOT EXISTS idx_cmx_auth_api_key_user ON cmx_auth_api_key (user_id);

-- =============================================
-- 26. 密码历史表 (cmx_auth_password_history)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_auth_password_history
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

CREATE INDEX IF NOT EXISTS idx_cmx_auth_password_history_user ON cmx_auth_password_history (user_id);

-- =============================================
-- 27. JWT 密钥表 (cmx_auth_jwt_key)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_auth_jwt_key
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_jwt_key_kid ON cmx_auth_jwt_key (kid);

-- =============================================
-- 28. Token 事件审计表 (cmx_auth_token_event)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_auth_token_event
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

CREATE INDEX IF NOT EXISTS idx_cmx_auth_token_event_user ON cmx_auth_token_event (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_auth_token_event_type ON cmx_auth_token_event (event_type);
CREATE INDEX IF NOT EXISTS idx_cmx_auth_token_event_created ON cmx_auth_token_event (create_time);

-- =============================================
-- 29. 第三方 OAuth2 账号关联表 (cmx_auth_oauth2_account)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_auth_oauth2_account
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_oauth2_account_provider_user ON cmx_auth_oauth2_account (provider, provider_user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_auth_oauth2_account_user ON cmx_auth_oauth2_account (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_auth_oauth2_account_provider_email ON cmx_auth_oauth2_account (provider, provider_email);

-- =============================================
-- 29. 用户表 (cmx_user)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_user
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_user DROP CONSTRAINT IF EXISTS uk_cmx_user_username;
DROP INDEX IF EXISTS uk_cmx_user_username;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_username ON cmx_user (username) WHERE archived = 0;
ALTER TABLE cmx_user DROP CONSTRAINT IF EXISTS uk_cmx_user_email;
DROP INDEX IF EXISTS uk_cmx_user_email;

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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_username ON cmx_user (username) WHERE archived = 0;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL AND archived = 0;

-- =============================================
-- 29a. 角色组表 (cmx_role_group)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_role_group
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

CREATE INDEX IF NOT EXISTS idx_cmx_role_group_parent ON cmx_role_group (parent_id);
CREATE INDEX IF NOT EXISTS idx_cmx_role_group_status ON cmx_role_group (status);

-- =============================================
-- 30. 角色表 (cmx_role)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_role
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_role DROP CONSTRAINT IF EXISTS uk_cmx_role_code;
DROP INDEX IF EXISTS uk_cmx_role_code;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_role_code ON cmx_role (code) WHERE archived = 0;

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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_role_code ON cmx_role (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_role_group_id ON cmx_role (role_group_id);

-- =============================================
-- 31. 用户角色关联表 (cmx_user_role)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_user_role
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_user_role DROP CONSTRAINT IF EXISTS uk_cmx_user_role;
DROP INDEX IF EXISTS uk_cmx_user_role;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_role ON cmx_user_role (user_id, role_id) WHERE archived = 0;

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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_role ON cmx_user_role (user_id, role_id) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_user_role_user ON cmx_user_role (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_role_role ON cmx_user_role (role_id);

-- =============================================
-- 32. 权限表 (cmx_permission)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_permission
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_permission DROP CONSTRAINT IF EXISTS uk_cmx_permission_code;
DROP INDEX IF EXISTS uk_cmx_permission_code;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_permission_code ON cmx_permission (code) WHERE archived = 0;

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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_permission_code ON cmx_permission (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_permission_parent ON cmx_permission (parent_id);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_full_path ON cmx_permission (full_code_path);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_parent_code ON cmx_permission (parent_code);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_domain_code ON cmx_permission (domain_code);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_app_code ON cmx_permission (app_code);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_module_code ON cmx_permission (module_code);

-- =============================================
-- 33. 角色权限关联表 (cmx_role_permission)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_role_permission
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_role_permission DROP CONSTRAINT IF EXISTS uk_cmx_role_permission;
DROP INDEX IF EXISTS uk_cmx_role_permission;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id) WHERE archived = 0;

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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_role_permission_role ON cmx_role_permission (role_id);
CREATE INDEX IF NOT EXISTS idx_cmx_role_permission_permission ON cmx_role_permission (permission_id);

-- 34. 用户角色临时授权表 (cmx_user_role_assignment)
CREATE TABLE IF NOT EXISTS cmx_user_role_assignment
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

CREATE INDEX IF NOT EXISTS idx_cmx_user_role_assignment_user ON cmx_user_role_assignment (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_role_assignment_role ON cmx_user_role_assignment (role_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_role_assignment_time ON cmx_user_role_assignment (effective_from, effective_until);
CREATE INDEX IF NOT EXISTS idx_cmx_user_role_assignment_expire ON cmx_user_role_assignment (effective_until) WHERE status = 1 AND archived = 0;

-- 35. 互斥规则表 (cmx_exclusion_rule)
CREATE TABLE IF NOT EXISTS cmx_exclusion_rule
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_exclusion_rule DROP CONSTRAINT IF EXISTS uk_cmx_exclusion_rule_code;
DROP INDEX IF EXISTS uk_cmx_exclusion_rule_code;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_exclusion_rule_code ON cmx_exclusion_rule (code) WHERE archived = 0;;

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_exclusion_rule_code ON cmx_exclusion_rule (code) WHERE archived = 0;

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
CREATE TABLE IF NOT EXISTS cmx_exclusion_rule_item
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_exclusion_rule_item ON cmx_exclusion_rule_item (rule_id, subject_id);
CREATE INDEX IF NOT EXISTS idx_cmx_exclusion_rule_item_rule ON cmx_exclusion_rule_item (rule_id);
CREATE INDEX IF NOT EXISTS idx_cmx_exclusion_rule_item_subject ON cmx_exclusion_rule_item (subject_id);

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
CREATE TABLE IF NOT EXISTS cmx_form
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_form_code ON cmx_form (code);
CREATE INDEX IF NOT EXISTS idx_cmx_form_module ON cmx_form (domain_code, application_code, module_code);

-- =============================================
-- 38. 菜单定义表 (cmx_menu)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_menu
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE cmx_menu DROP CONSTRAINT IF EXISTS uk_cmx_menu_code;
DROP INDEX IF EXISTS uk_cmx_menu_code;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_menu_code ON cmx_menu (code) WHERE archived = 0;

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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_menu_code ON cmx_menu (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_menu_module ON cmx_menu (domain_code, application_code, module_code);
CREATE INDEX IF NOT EXISTS idx_cmx_menu_parent_id ON cmx_menu (parent_id);
-- 级联操作(移动/删除/树查询)按 code_path/id_path 前缀匹配,需索引支撑,否则全表扫描
CREATE INDEX IF NOT EXISTS idx_cmx_menu_code_path ON cmx_menu (code_path);
CREATE INDEX IF NOT EXISTS idx_cmx_menu_id_path ON cmx_menu (id_path);

-- =============================================
-- 39. 模块当前版本表 (cmx_module_current_version)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_module_current_version
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_module_current_version_module ON cmx_module_current_version (module_code);
CREATE INDEX IF NOT EXISTS idx_cmx_module_current_version_dom_app_mod ON cmx_module_current_version (domain_code, application_code, module_code);

-- =============================================
-- 40. 模块版本历史表 (cmx_module_version_history)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_module_version_history
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_module_version_history_pkg ON cmx_module_version_history (module_code, package_version);
CREATE INDEX IF NOT EXISTS idx_cmx_module_version_history_module ON cmx_module_version_history (module_id);

-- =============================================
-- 37. 模型中心台账自描述表 (cmx_model_meta)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_model_meta
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_model_meta_db_app ON cmx_model_meta (db_id, app_id);

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
CREATE TABLE IF NOT EXISTS cmx_model_module
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_key ON cmx_model_module (db_id, app_id, domain_code, application_code, module_code);

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
CREATE TABLE IF NOT EXISTS cmx_model_module_kind
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_kind_key ON cmx_model_module_kind (db_id, app_id, domain_code, application_code, module_code, kind);
CREATE INDEX IF NOT EXISTS idx_model_module_kind_module ON cmx_model_module_kind (db_id, domain_code, application_code, module_code);

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
CREATE TABLE IF NOT EXISTS cmx_model_deploy_history
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

CREATE INDEX IF NOT EXISTS idx_model_history_module ON cmx_model_deploy_history (db_id, domain_code, application_code, module_code);
CREATE INDEX IF NOT EXISTS idx_model_history_batch  ON cmx_model_deploy_history (batch_id);
CREATE INDEX IF NOT EXISTS idx_model_history_time   ON cmx_model_deploy_history (create_time);

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
CREATE TABLE IF NOT EXISTS cmx_model_source
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_model_source_ver ON cmx_model_source (db_id, app_id, domain_code, application_code, module_code, kind, version);
CREATE INDEX IF NOT EXISTS idx_model_source_current   ON cmx_model_source (db_id, domain_code, application_code, module_code, kind, is_current);

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
CREATE TABLE IF NOT EXISTS cmx_model_registry
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_model_registry_db ON cmx_model_registry (db_id, app_id);

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
CREATE TABLE IF NOT EXISTS cmx_doc_revision
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

CREATE UNIQUE INDEX IF NOT EXISTS uk_doc_rev     ON cmx_doc_revision (doc_file, root_id, rev_no);
CREATE INDEX IF NOT EXISTS        idx_doc_rev_cur ON cmx_doc_revision (doc_file, root_id, is_current);
CREATE INDEX IF NOT EXISTS        idx_doc_rev_time ON cmx_doc_revision (root_id, created_at);

-- =============================================
-- 43. 业务单据版本化-字段级变更明细表 (cmx_doc_change)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_doc_change
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

CREATE INDEX IF NOT EXISTS idx_doc_change_rev ON cmx_doc_change (rev_id);
CREATE INDEX IF NOT EXISTS idx_doc_change_row ON cmx_doc_change (root_id, row_id, field);

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

-- ============================================================
-- 补丁段：旧 init_ddl 快照未收录的终态索引（对象覆盖核对发现）
-- ============================================================

-- 插件版本当前态部分索引（原迁移 20260501_001，快照漏收）
CREATE INDEX IF NOT EXISTS idx_version_current
    ON cmx_plugin_versions (plugin_id, is_current) WHERE is_current = TRUE;

-- ============================================================
-- CMX 平台库（主库）内置数据（DML）— docs/sql/v2/platform/init_dml.sql
--
-- 目标库：default 数据源（平台/主库）
-- 风格：无损幂等，全部 ON CONFLICT / NOT EXISTS 防重，可重复执行
-- 来源：见各段注释（旧 init_dml.sql + dam完善.sql + 迁移链种子终态）
-- ============================================================

-- ============================================================
-- 1. DAM 注册数据：域 / 应用 / 模块
-- 来源：迁移 20260714_001（registry.json 入库）+ MDM 域补录（basic/dataplatform/mdm）
-- 幂等：ON CONFLICT (id) DO UPDATE（DAM 注册表为配置数据，重放以仓库为准刷新）
-- ============================================================

-- ---- 1. 域（cmx_domain）7 条 ----
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES
    ('fi',     'fi',     '财务资源管理',   'Finance',                'expense-report', '财务、会计核算、总账及 ERP 凭证相关资源。', 1, 0, 1),
    ('hr',     'hr',     '人力资源管理',   'Human Resources',         'employee',       '招聘与候选人等人力资源资源。',           1, 0, 2),
    ('cr',     'cr',     '客户资源管理',   'Collaboration Resources', 'collaborate',    '协作资源、示例页面与门户扩展资源。',       1, 0, 3),
    ('dr',     'dr',     '数据资源管理',   'data Resources',          'database',       '',                                       1, 0, 4),
    ('sc',     'sc',     '生产资源管理',   '生产资源管理',             'machine',        '',                                       1, 0, 5),
    ('portal', 'portal', '门户',           'Portal',                  'home',           '门户平台域。',                            0, 0, 8),
    ('basic', 'basic', '基础主数据',     'Master Data',             'database',       '企业主数据管理平台（MDM）：主数据(cm_*)只存 published，变更请求(cv_*)走审批激活。', 1, 0, 9)
ON CONFLICT (id) DO UPDATE SET
    code = EXCLUDED.code, name = EXCLUDED.name, title = EXCLUDED.title,
    icon = EXCLUDED.icon, description = EXCLUDED.description,
    status = EXCLUDED.status, archived = EXCLUDED.archived, sort_order = EXCLUDED.sort_order;

-- ---- 2. 应用（cmx_application）11 条 ----
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES
    ('portal_portal',  'portal',  'portal', '门户',         'Portal',                'home',                          '门户平台应用。',           0, 0, 8),
    ('fi_cmxfico',     'cmxfico', 'fi',     '会计核算',     'CMX FICO',              'expense-report',                '自研会计核算应用。',       1, 0, 1),
    ('fi_sap',         'sap',     'fi',     'SAP',          'SAP FI',                'business-objects-experience',   'SAP 总账样例资源。',       1, 0, 2),
    ('fi_ebs',         'ebs',     'fi',     'Oracle EBS',   'Oracle EBS',            'decrease-line-height',          'Oracle EBS 总账样例资源。', 1, 0, 3),
    ('fi_yonyou',      'yonyou',  'fi',     '用友',         'Yonyou',                'developer-settings',            '用友总账样例资源。',       1, 0, 4),
    ('fi_kingdee',     'kingdee', 'fi',     '金蝶',         'Kingdee',               'electronic-medical-record',     '金蝶总账样例资源。',       1, 0, 5),
    ('hr_recruit',     'recruit', 'hr',     '招聘',         'Recruitment',           'add-employee',                  '招聘服务目录。',           1, 0, 6),
    ('cr_explorer',    'explorer','cr',     '资源浏览',     'Explorer',              'documents',                     '资源浏览与菜单页面示例。', 1, 0, 7),
    ('dr_zhili',       'zhili',   'dr',     '数据中台',     '',                      'display-more',                  '',                        1, 0, 8),
    ('sc_datalake',    'datalake','sc',     '数据湖',       '',                      'background',                    '',                        1, 0, 9),
    ('basic_dataplatform', 'dataplatform', 'basic', '数据平台', 'Data Platform', 'database', '主数据管理平台应用：含主数据建模、激活器、变更请求、匹配合并、分发。', 1, 0, 10)
ON CONFLICT (id) DO UPDATE SET
    code = EXCLUDED.code, domain_code = EXCLUDED.domain_code, name = EXCLUDED.name,
    title = EXCLUDED.title, icon = EXCLUDED.icon, description = EXCLUDED.description,
    status = EXCLUDED.status, archived = EXCLUDED.archived, sort_order = EXCLUDED.sort_order;

-- ---- 3. 模块（cmx_module）10 条 ----
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES
    ('portal_portal_overview', 'overview', 'portal', 'portal',
 '平台总览', '门户平台总览', 'home', '门户平台使用入门与总览帮助。',
 '[]', 'portal/portal/overview', 'modules/portal/portal/overview/module.json', 0, 0, 8),
    ('fi_cmxfico_gl', 'gl', 'fi', 'cmxfico',
 '总账', '会计核算管理 / 总账', 'activity-items', '会计核算管理、ERP 凭证、总账科目、辅助核算等资源。',
 '["fi.cmxfico.gl","cmxfico.gl"]', 'fi/cmxfico/gl', 'modules/fi/cmxfico/gl/module.json', 1, 0, 1),
    ('fi_sap_gl', 'sap_gl', 'fi', 'sap',
 'SAP 总账', 'SAP 总账样例', 'business-objects-experience', 'SAP FI 总账样例。',
 '[]', 'fi/sap/sap_gl', 'modules/fi/sap/sap_gl/module.json', 1, 0, 2),
    ('fi_ebs_gl', 'ebs_gl', 'fi', 'ebs',
 'Oracle EBS 总账', 'Oracle EBS 总账样例', 'database', 'Oracle EBS 总账样例。',
 '[]', 'fi/ebs/ebs_gl', 'modules/fi/ebs/ebs_gl/module.json', 1, 0, 3),
    ('fi_yonyou_gl', 'yonyou_gl', 'fi', 'yonyou',
 '用友总账', '用友总账样例', 'database', '用友总账样例。',
 '[]', 'fi/yonyou/yonyou_gl', 'modules/fi/yonyou/yonyou_gl/module.json', 1, 0, 4),
    ('fi_kingdee_gl', 'kingdee_gl', 'fi', 'kingdee',
 '金蝶总账', '金蝶总账样例', 'database', '金蝶总账样例。',
 '[]', 'fi/kingdee/kingdee_gl', 'modules/fi/kingdee/kingdee_gl/module.json', 1, 0, 5),
    ('hr_recruit_candidate', 'candidate', 'hr', 'recruit',
 '候选人', '招聘候选人服务目录', 'employee', '候选人服务目录。',
 '[]', 'hr/recruit/candidate', 'modules/hr/recruit/candidate/module.json', 1, 0, 6),
    ('cr_explorer_explorer-menu', 'explorer-menu', 'cr', 'explorer',
 'Explorer 菜单', 'CR Explorer 菜单页面示例', 'documents', 'CR Explorer 菜单页面示例。',
 '[]', 'cr/explorer/explorer-menu', 'modules/cr/explorer/explorer-menu/module.json', 1, 0, 7),
    ('fi_cmxfico_report', 'report', 'fi', 'cmxfico',
 '报表', '报表', 'excel-attachment', '',
 '[]', 'fi/cmxfico/report', 'modules/fi/cmxfico/report/module.json', 1, 0, 8),
    ('basic_dataplatform_mdm', 'mdm', 'basic', 'dataplatform',
 '主数据', '企业主数据管理', 'database', '主数据(cm_*)只存 published；变更请求(cv_*)走审批激活；含激活映射配置、匹配合并、分发订阅。',
 '["basic.dataplatform.mdm","mdm"]', 'basic/dataplatform/mdm', 'modules/basic/dataplatform/mdm/module.json', 1, 0, 9)
ON CONFLICT (id) DO UPDATE SET
    code = EXCLUDED.code, domain_code = EXCLUDED.domain_code, application_code = EXCLUDED.application_code,
    name = EXCLUDED.name, title = EXCLUDED.title, icon = EXCLUDED.icon, description = EXCLUDED.description,
    tags = EXCLUDED.tags, resource_root = EXCLUDED.resource_root, manifest_path = EXCLUDED.manifest_path,
    status = EXCLUDED.status, archived = EXCLUDED.archived, sort_order = EXCLUDED.sort_order;

-- ============================================================
-- 2. 内置角色与权限（IAM）
-- 来源：init_dml.sql（admin/user 角色 + 24 条 GL 权限种子）
--       + 迁移 20260817_001（mdm_approver 审批角色 + admin 预分配）
-- 幂等：ON CONFLICT DO NOTHING / NOT EXISTS 防重
-- ============================================================

-- 内置角色（cmx_role）3 条
INSERT INTO cmx_role (id, code, name, data_scope, sort_order, status, archived, description)
VALUES
    ('1898765432100001001', 'admin', '系统管理员', 1, 1, 1, 0, '拥有全部权限'),
    ('1898765432100001002', 'user', '普通用户', 5, 2, 1, 0, '仅查看本人数据')
ON CONFLICT (id) DO NOTHING;

-- 主数据审批角色（BPMN candidateGroups 引用，20260817_001）
INSERT INTO cmx_role (id, code, name, data_scope, sort_order, status, description)
VALUES ('1898765432100001101', 'mdm_approver', '主数据审批员', 1, 10, 1, '主数据变更申请审批（候选池认领）')
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

-- 内置权限（cmx_permission）24 条（20260714 改码后的 fi/cmxfico/gl 小写编码版）
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667162255360', 'gl:account', '科目管理', 'menu', null, 3, '会计科目体系维护', 1, 0, '2026-06-25 09:53:36.798272', '2026-06-25 09:53:36.798272', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:account', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676112900096', 'gl:account:add', '科目新增', 'button', '7475848667162255360', 1, '新增会计科目', 1, 0, '2026-06-25 09:53:38.915084', '2026-06-25 09:53:38.915084', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:add', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848677895479296', 'gl:account:delete', '科目删除', 'button', '7475848667162255360', 3, '删除末级科目', 1, 0, '2026-06-25 09:53:39.343854', '2026-06-25 09:53:39.343854', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:delete', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676989509632', 'gl:account:edit', '科目编辑', 'button', '7475848667162255360', 2, '修改科目信息', 1, 0, '2026-06-25 09:53:39.129204', '2026-06-25 09:53:39.129204', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:edit', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848679703224320', 'gl:account:export', '科目导出', 'button', '7475848667162255360', 5, '导出科目体系', 1, 0, '2026-06-25 09:53:39.776420', '2026-06-25 09:53:39.776420', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:export', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848678809837568', 'gl:account:query', '科目查询', 'api', '7475848667162255360', 4, '查询科目树形列表', 1, 0, '2026-06-25 09:53:39.562303', '2026-06-25 09:53:39.562303', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:query', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848665702637568', 'gl:dashboard', '总账仪表盘', 'menu', null, 1, '总账模块概览看板', 1, 0, '2026-06-25 09:53:36.449545', '2026-06-25 09:53:36.449545', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:dashboard', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670211514368', 'gl:dashboard:refresh', '数据刷新', 'button', '7475848665702637568', 2, '手动刷新仪表盘统计', 1, 0, '2026-06-25 09:53:37.499114', '2026-06-25 09:53:37.499114', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:refresh', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848669469122560', 'gl:dashboard:view', '仪表盘查看', 'api', '7475848665702637568', 1, '查看总账仪表盘数据', 1, 0, '2026-06-25 09:53:37.325539', '2026-06-25 09:53:37.325539', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:view', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667845926912', 'gl:period', '期末处理', 'menu', null, 4, '期末结账与损益结转', 1, 0, '2026-06-25 09:53:36.957909', '2026-06-25 09:53:36.957909', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:period', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848680584028160', 'gl:period:close', '期末结账', 'button', '7475848667845926912', 1, '对当前会计期间结账', 1, 0, '2026-06-25 09:53:39.985243', '2026-06-25 09:53:39.985243', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:period', '/gl:period/gl:period:close', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848682249166848', 'gl:period:settle', '损益结转', 'button', '7475848667845926912', 3, '结转本期损益至本年利润', 1, 0, '2026-06-25 09:53:40.383092', '2026-06-25 09:53:40.383092', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:period', '/gl:period/gl:period:settle', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848681427083264', 'gl:period:unclose', '反结账', 'button', '7475848667845926912', 2, '撤销已结账期间', 1, 0, '2026-06-25 09:53:40.186302', '2026-06-25 09:53:40.186302', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:period', '/gl:period/gl:period:unclose', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848668558958592', 'gl:report', '报表中心', 'menu', null, 5, '总账报表查询与导出', 1, 0, '2026-06-25 09:53:37.122063', '2026-06-25 09:53:37.122063', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:report', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848683444543488', 'gl:report:balance', '资产负债表', 'api', '7475848668558958592', 1, '生成资产负债表', 1, 0, '2026-06-25 09:53:40.665266', '2026-06-25 09:53:40.665266', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:report', '/gl:report/gl:report:balance', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848685168402432', 'gl:report:cashflow', '现金流量表', 'api', '7475848668558958592', 3, '生成现金流量表', 1, 0, '2026-06-25 09:53:41.078737', '2026-06-25 09:53:41.078737', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:report', '/gl:report/gl:report:cashflow', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848684400844800', 'gl:report:income', '利润表', 'api', '7475848668558958592', 2, '生成利润表', 1, 0, '2026-06-25 09:53:40.893176', '2026-06-25 09:53:40.893176', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:report', '/gl:report/gl:report:income', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848666486972416', 'gl:voucher', '凭证管理', 'menu', null, 2, '会计凭证全生命周期管理', 1, 0, '2026-06-25 09:53:36.636949', '2026-06-25 09:53:36.636949', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:voucher', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670991654912', 'gl:voucher:add', '凭证录入', 'button', '7475848666486972416', 1, '新增会计凭证', 1, 0, '2026-06-25 09:53:37.676977', '2026-06-25 09:53:37.676977', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:add', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848673445322752', 'gl:voucher:audit', '凭证审核', 'button', '7475848666486972416', 4, '审核/反审核凭证', 1, 0, '2026-06-25 09:53:38.281608', '2026-06-25 09:53:38.281608', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:audit', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848675001409536', 'gl:voucher:delete', '凭证删除', 'button', '7475848666486972416', 6, '删除作废凭证', 1, 0, '2026-06-25 09:53:38.655424', '2026-06-25 09:53:38.655424', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:delete', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848672686153728', 'gl:voucher:edit', '凭证修改', 'button', '7475848666486972416', 3, '修改未审核凭证', 1, 0, '2026-06-25 09:53:38.088995', '2026-06-25 09:53:38.088995', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:edit', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848674095439872', 'gl:voucher:post', '凭证过账', 'button', '7475848666486972416', 5, '将审核凭证过账到账簿', 1, 0, '2026-06-25 09:53:38.439356', '2026-06-25 09:53:38.439356', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:post', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848671801155584', 'gl:voucher:query', '凭证查询', 'api', '7475848666486972416', 2, '按条件查询凭证列表', 1, 0, '2026-06-25 09:53:37.878384', '2026-06-25 09:53:37.878384', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:query', 1, 2) ON CONFLICT (id) DO NOTHING;

-- admin 预分配 mdm_approver（角色无成员 = 审批任务对所有人不可见，故预分配）
INSERT INTO cmx_user_role (id, user_id, role_id, archived, create_time)
SELECT '1898765432100001201', u.id, '1898765432100001101', 0, CURRENT_TIMESTAMP
FROM cmx_user u
WHERE u.username = 'admin'
  AND NOT EXISTS (
    SELECT 1 FROM cmx_user_role ur
    WHERE ur.user_id = u.id AND ur.role_id = '1898765432100001101'
  );

-- ============================================================
-- 3. 菜单（cmx_menu）
-- 来源：init_menu.sql（menu-pages JSON 生成的全量最新状态，147 条）
--       + 追加迁移 20260801_002 / 20260815_001 / 20260815_002（5 条，
--         init_menu.sql 快照未含，见对象覆盖核对报告）
-- 幂等：ON CONFLICT (code) WHERE archived = 0 DO NOTHING / WHERE NOT EXISTS
-- ============================================================

INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829632', 'gl', '基础数据管理', 'tabler-filled/home', NULL, 1, '{"caption":"基础数据管理","expanded":true,"name":"gl"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 0, '/gl', '/7484975236341829632', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829633', 'portal-console', '系统控制', 'tabler-outline/load-balancer', NULL, 1, '{"caption":"系统控制","workspace":{"prepare":{"caption":"系统控制-原生模式","icon":"tabler-outline/wash-tumble-dry","width":"720px","height":"520px","views":[{"tabLabel":"概览","type":"placeholder","data":{"title":"控制台首页"}},{"tabLabel":"快捷数据","type":"json","data":{"value":{"region":"content","tabs":2}}}]},"content":{"caption":"系统控制-原生模式","icon":"edit","views":[{"tabLabel":"概览","type":"placeholder","data":{"title":"控制台首页"}},{"tabLabel":"快捷数据","type":"json","data":{"value":{"region":"content","tabs":2}}}]},"explorer":{"caption":"导航","icon":"detail-view","views":[{"tabLabel":"概览","icon":"detail-view","type":"placeholder","data":{"note":"Explorer 多视图 — 签 1"}},{"tabLabel":"模型","icon":"source-code","type":"code","data":{"code":"// 示例代码视图\nentity PortalPage {\n  id: string;\n  title: string;\n}"}}]},"property":{"caption":"智能体","icon":"documents","views":[{"tabLabel":"页面","icon":"document","type":"placeholder","data":{}},{"tabLabel":"说明","icon":"information","type":"markdown","data":{"text":"同一菜单可在四个区域各配置多个视图；底部 Tab 切换。"}}]},"bottom":{"caption":"日志","icon":"log","views":[{"tabLabel":"状态","icon":"log","type":"placeholder","data":{"title":"Bottom 第一视图（类型可与第一不同）"}},{"tabLabel":"概览","icon":"detail-view","type":"placeholder","data":{"note":"Bottom 多视图 — 签 2"}}]}},"name":"console"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/portal-console', '/7484975236341829632/7484975236341829633', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829634', 'user-edit', '编辑用户', 'edit', NULL, 2, '{"caption":"编辑用户","dialogspace":{"title":"编辑用户","icon":"person-placeholder","description":"修改基本信息","content":{"label":"用户信息","icon":"form","views":[{"htmlPageId":"user-form-page"}]}}}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/user-edit', '/7484975236341829632/7484975236341829634', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829635', 'deploy-config', '部署配置', 'shipping-status', NULL, 3, '{"caption":"部署配置","dialogspace":{"title":"部署配置向导","icon":"shipping-status","description":"v2.4.1 → 生产环境","buttons":[{"id":"help","text":"帮助","icon":"sys-help","design":"Transparent"},{"id":"preview","text":"预览","icon":"show","design":"Transparent"}],"explorer":{"label":"环境列表","icon":"navigation-down-arrow","views":[{"htmlPageId":"exp_view1","tabLabel":"环境树"},{"htmlPageId":"exp_view2","tabLabel":"最近"}]},"content":{"label":"配置项","icon":"settings","views":[{"htmlPageId":"ctn_view1","tabLabel":"基本","tabIcon":"form"},{"htmlPageId":"ctn_view2","tabLabel":"变量","tabIcon":"key-user-settings"},{"htmlPageId":"pre_view1","tabLabel":"密钥","tabIcon":"locked"}]},"property":{"label":"属性","icon":"hint","views":[{"htmlPageId":"pro_view1"},{"htmlPageId":"pro_view2"}]},"bottom":{"views":[{"htmlPageId":"btm_view1","tabLabel":"日志"},{"htmlPageId":"btm_view2","tabLabel":"检查"}]},"confirmText":"开始部署","cancelText":"取消","dialogWidth":"90vw","dialogHeight":"85vh","explorerWidth":240,"propertyWidth":300,"footerHeight":140}}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/deploy-config', '/7484975236341829632/7484975236341829635', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829636', 'portal-overview', '页面组装测试', 'tabler-outline/nut', NULL, 4, '{"caption":"页面组装测试","workspace":{"id":"test_workspace","params":{"userName":"Jeff","defaultMessage":"Hello from menu params"},"prepare":{"caption":"准备页面组装测试","icon":"home","height":"500px","width":"600px","views":[{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"pre_view1"},{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"pre_view2"}]},"model":{"type":"html_pages","html_page":"ws-shared-model","id":"ws-model"},"inner":{"caption":"内部用对话框表单，打开菜单时只加载，不显示","icon":"edit","views":[{"tabLabel":"inner1","icon":"detail-view","type":"html_pages","html_page":"inner_page2"},{"tabLabel":"inner2","icon":"detail-view","type":"html_pages","html_page":"inner_page1"}]},"embed":{"caption":"内部嵌入表单，打开菜单时在页面中的embed_page组件展示","icon":"edit","views":[{"tabLabel":"embed1","icon":"detail-view","type":"html_pages","html_page":"embed_page1"},{"tabLabel":"测试页面","icon":"detail-view","type":"html_pages","html_page":"embed_page2"}]},"content":{"caption":"系统概览-页面模式","icon":"edit","views":[{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"ctn_view1"},{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"ctn_view2"}]},"explorer":{"caption":"导航","icon":"detail-view","views":[{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"exp_view1"},{"tabLabel":"模型","icon":"source-code","type":"html_pages","html_page":"exp_view2"}]},"property":{"caption":"智能体","icon":"documents","views":[{"tabLabel":"页面","icon":"document","type":"html_pages","html_page":"pro_view1"},{"tabLabel":"说明","icon":"information","type":"html_pages","html_page":"pro_view2"}]},"bottom":{"caption":"日志","icon":"log","views":[{"tabLabel":"状态","icon":"log","type":"html_pages","html_page":"btm_view1"},{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"btm_view2"}]},"float":{"views":[{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"flt_view1"},{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"flt_view2"}]}},"type":"workspace-node","name":"overview"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/portal-overview', '/7484975236341829632/7484975236341829636', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829637', 'outlook-mailbox', 'Outlook邮箱', 'email', NULL, 5, '{"caption":"Outlook邮箱","workspace":{"explorer":{"caption":"邮箱目录","icon":"email","views":[{"tabLabel":"目录","icon":"email","type":"html_pages","html_page":"outlook-mail-explorer"}]},"content":{"caption":"邮件列表与正文","icon":"list","views":[{"tabLabel":"邮件","icon":"list","type":"html_pages","html_page":"outlook-mail-content"}]}},"type":"workspace-node","name":"outlook-mailbox"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/outlook-mailbox', '/7484975236341829632/7484975236341829637', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829638', 'demo-master-slave', '主从表单生成测试', 'table-view', NULL, 6, '{"caption":"主从表单生成测试","workspace":{"content":{"caption":"主从表格示例","icon":"table-view","views":[{"tabLabel":"主从表格","icon":"table-view","type":"html_pages","html_page":"demo-master-slave"}]}},"type":"workspace-node","name":"master-slave-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/demo-master-slave', '/7484975236341829632/7484975236341829638', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829639', 'combo-box-demo', 'cmx-combo-box 三模式演示', 'tabler-outline/select', NULL, 7, '{"caption":"cmx-combo-box 三模式演示","workspace":{"content":{"caption":"cmx-combo-box 三模式演示","icon":"tabler-outline/select","views":[{"tabLabel":"combo-box 演示","icon":"tabler-outline/select","type":"html_pages","html_page":"combo-box-demo"}]}},"type":"workspace-node","name":"combo-box-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/combo-box-demo', '/7484975236341829632/7484975236341829639', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829640', 'tabulator-treegrid-demo', 'cmx-tabulator 会计科目树', 'tabler-outline/binary-tree', NULL, 8, '{"caption":"cmx-tabulator 会计科目树","workspace":{"content":{"caption":"cmx-tabulator 会计科目树（cf_gl_account 自分级字典）","icon":"tabler-outline/binary-tree","views":[{"tabLabel":"会计科目树","icon":"tabler-outline/binary-tree","type":"html_pages","html_page":"tabulator-treegrid-demo"}]}},"type":"workspace-node","name":"tabulator-treegrid-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/tabulator-treegrid-demo', '/7484975236341829632/7484975236341829640', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829641', 'dict-select-demo', 'cmx-dict-select 数据字典选择演示', 'tabler-outline/list-search', NULL, 9, '{"caption":"cmx-dict-select 数据字典选择演示","workspace":{"content":{"caption":"cmx-dict-select 数据字典选择演示","icon":"tabler-outline/list-search","views":[{"tabLabel":"字典选择演示","icon":"tabler-outline/list-search","type":"html_pages","html_page":"dict-select-demo"}]}},"type":"workspace-node","name":"dict-select-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/dict-select-demo', '/7484975236341829632/7484975236341829641', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829642', 'cmx-input-editors-demo', '录入控件演示（文本/数字/日期/日期时间）', 'tabler-outline/forms', NULL, 10, '{"caption":"录入控件演示（文本/数字/日期/日期时间）","workspace":{"content":{"caption":"录入控件演示（文本/数字/日期/日期时间）","icon":"tabler-outline/forms","views":[{"tabLabel":"录入控件演示","icon":"tabler-outline/forms","type":"html_pages","html_page":"cmx-input-editors-demo"}]}},"type":"workspace-node","name":"cmx-input-editors-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/cmx-input-editors-demo', '/7484975236341829632/7484975236341829642', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829643', 'cmx-grid-form-linkage-demo', 'grid↔form 联动（四录入组件·模型面板）', 'tabler-outline/table-options', NULL, 11, '{"caption":"grid↔form 联动（四录入组件·模型面板）","workspace":{"content":{"caption":"grid↔form 联动（四录入组件·模型面板）","icon":"tabler-outline/table-options","views":[{"tabLabel":"grid↔form 联动","icon":"tabler-outline/table-options","type":"html_pages","html_page":"cmx-grid-form-linkage-demo"}]}},"type":"workspace-node","name":"cmx-grid-form-linkage-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/cmx-grid-form-linkage-demo', '/7484975236341829632/7484975236341829643', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829644', 'fi-gl-voucher', '财务会计凭证', 'tabler-outline/file-spreadsheet', NULL, 12, '{"caption":"财务会计凭证","workspace":{"content":{"caption":"财务会计凭证","icon":"table-view","views":[{"tabLabel":"凭证","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.voucher-neo"}]}},"type":"workspace-node","name":"voucher"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-voucher', '/7484975236341829632/7484975236341829644', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829645', 'fi-gl-voucher-demo', '会计凭证（三层·维度选择）', 'tabler-outline/stack-2', NULL, 13, '{"caption":"会计凭证（三层·维度选择）","workspace":{"content":{"caption":"会计凭证（头/分录/明细 三层）","icon":"table-view","views":[{"tabLabel":"凭证","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.gl-voucher-demo"}]}},"type":"workspace-node","name":"gl-voucher-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-voucher-demo', '/7484975236341829632/7484975236341829645', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829646', 'fi-gl-voucher-v1-demo', '会计凭证（v1 单表多字典）', 'tabler-outline/database', NULL, 14, '{"caption":"会计凭证（v1 单表多字典）","workspace":{"content":{"caption":"会计凭证 v1(单表多字典)","icon":"table-view","views":[{"tabLabel":"凭证","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.gl-voucher-v1-demo"}]}},"type":"workspace-node","name":"gl-voucher-v1-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-voucher-v1-demo', '/7484975236341829632/7484975236341829646', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829647', 'fi-gl-trade', '交易单据', 'tabler-outline/shopping-cart', NULL, 15, '{"caption":"交易单据","workspace":{"content":{"caption":"交易单据","icon":"table-view","views":[{"tabLabel":"单据","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.trade-neo"}]}},"type":"workspace-node","name":"trade"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-trade', '/7484975236341829632/7484975236341829647', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829648', 'fi-gl-travel-expense', '差旅费报销', 'tabler-outline/plane-departure', NULL, 16, '{"caption":"差旅费报销","workspace":{"content":{"caption":"差旅费报销","icon":"table-view","views":[{"tabLabel":"报销单","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.travel-expense-neo"}]}},"type":"workspace-node","name":"travel-expense"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-travel-expense', '/7484975236341829632/7484975236341829648', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829649', 'fi-gl-trade-form', '交易单据（表单视图）', 'tabler-outline/forms', NULL, 17, '{"caption":"交易单据（表单视图）","workspace":{"content":{"caption":"交易单据（表单视图）","icon":"form","views":[{"tabLabel":"单据","icon":"form","type":"html_pages","html_page":"fi.cmxfico.gl.trade-form"}]}},"type":"workspace-node","name":"trade-form"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-trade-form', '/7484975236341829632/7484975236341829649', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829650', 'fi-gl-dict-editor-demo', '字典选择编辑器演示', 'tabler-outline/edit', NULL, 18, '{"caption":"字典选择编辑器演示","workspace":{"content":{"caption":"cmx-dict-select 编辑器演示（grid + form）","icon":"value-help","views":[{"tabLabel":"编辑器演示","icon":"value-help","type":"html_pages","html_page":"fi.cmxfico.gl.dict-editor-demo"}]}},"type":"workspace-node","name":"dict-editor-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dict-editor-demo', '/7484975236341829632/7484975236341829650', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829651', 'fi-gl-master-data-demo', '总账主数据演示', 'tabler-outline/database-search', NULL, 19, '{"caption":"总账主数据演示","workspace":{"content":{"caption":"总账主数据演示","icon":"value-help","views":[{"tabLabel":"主数据演示","icon":"value-help","type":"html_pages","html_page":"fi.cmxfico.gl.gl-master-data-demo"}]}},"type":"workspace-node","name":"gl-master-data-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-master-data-demo', '/7484975236341829632/7484975236341829651', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829652', 'fi-gl-dict-help-test', '主数据帮助测试', 'tabler-outline/test-pipe', NULL, 20, '{"caption":"主数据帮助测试","workspace":{"content":{"caption":"主数据帮助测试","icon":"value-help","views":[{"tabLabel":"字典帮助测试","icon":"search","type":"html_pages","html_page":"fi.cmxfico.gl.dict-help-test"}]}},"type":"workspace-node","name":"dict-help-test"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dict-help-test', '/7484975236341829632/7484975236341829652', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829653', 'fi-gl-dict-help-lab', '主数据帮助实验室', 'tabler-outline/flask', NULL, 21, '{"caption":"主数据帮助实验室","workspace":{"content":{"caption":"主数据帮助实验室","icon":"simulate","views":[{"tabLabel":"帮助实验室","icon":"simulate","type":"html_pages","html_page":"fi.cmxfico.gl.dict-help-lab"}]}},"type":"workspace-node","name":"dict-help-lab"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dict-help-lab', '/7484975236341829632/7484975236341829653', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829654', 'fi-gl-meta-model-service-test', '元数据模型服务测试', 'database', NULL, 22, '{"caption":"元数据模型服务测试","workspace":{"content":{"caption":"元数据模型服务测试","icon":"database","views":[{"tabLabel":"功能测试","icon":"database","type":"html_pages","html_page":"fi.cmxfico.gl.meta-model-service-test"}]}},"type":"workspace-node","name":"meta-model-service-test"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-meta-model-service-test', '/7484975236341829632/7484975236341829654', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829655', 'fi-gl-registry-editor', '注册表编辑器', 'tabler-outline/database-search', NULL, 23, '{"caption":"注册表编辑器","workspace":{"id":"registry_editor","content":{"caption":"注册表编辑器","icon":"tabler-outline/database-search","views":[{"id":"registry-editor-content","tabLabel":"注册表","icon":"tabler-outline/database-search","type":"native_pages","native_page":"portal.system.registry-editor","view":"content"}]}},"type":"workspace-node","name":"registry-editor"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-registry-editor', '/7484975236341829632/7484975236341829655', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829656', 'fi-gl-dict-grid-meta', '数据字典动态表格', 'grid', NULL, 24, '{"caption":"数据字典动态表格","workspace":{"content":{"caption":"数据字典动态表格","icon":"grid","views":[{"tabLabel":"字典内容","icon":"grid","type":"html_pages","html_page":"fi.cmxfico.gl.dict-grid-meta"}]}},"type":"workspace-node","name":"dict-grid-meta"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dict-grid-meta', '/7484975236341829632/7484975236341829656', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829657', 'fi-gl-erp-voucher-cnpc', 'ERP凭证样例(CNPC)', 'receipt', NULL, 25, '{"caption":"ERP凭证样例(CNPC)","workspace":{"content":{"caption":"ERP凭证样例(CNPC)","icon":"receipt","views":[{"tabLabel":"凭证对比","icon":"receipt","type":"html_pages","html_page":"fi.cmxfico.gl.erp-voucher-cnpc"}]}},"type":"workspace-node","name":"erp-voucher-cnpc"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-erp-voucher-cnpc', '/7484975236341829632/7484975236341829657', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829658', 'fi-gl-erp-voucher-cnpc-ms', 'ERP凭证样例(MasterSlave模型版)', 'org-chart', NULL, 26, '{"caption":"ERP凭证样例(MasterSlave模型版)","workspace":{"content":{"caption":"ERP凭证样例(MasterSlave模型版)","icon":"org-chart","views":[{"tabLabel":"四层主从凭证","icon":"org-chart","type":"html_pages","html_page":"fi.cmxfico.gl.erp-voucher-cnpc-ms"}]}},"type":"workspace-node","name":"erp-voucher-cnpc-ms"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-erp-voucher-cnpc-ms', '/7484975236341829632/7484975236341829658', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829659', 'fi-gl-fico-ws', 'ERP凭证(三区工作台)', 'business-objects-experience', NULL, 27, '{"caption":"ERP凭证(三区工作台)","workspace":{"id":"fico_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-model","id":"fico-ws-model"},"explorer":{"caption":"凭证批","icon":"list","views":[{"id":"fico-ws-explorer","tabLabel":"凭证批","icon":"list","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-explorer"}]},"content":{"caption":"凭证","icon":"document-text","views":[{"id":"fico-ws-content","tabLabel":"凭证视图","icon":"document-text","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-content"},{"id":"fico-ws-source","tabLabel":"凭证数据","icon":"source-code","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-source"}]},"property":{"caption":"属性/操作","icon":"detail-view","views":[{"id":"fico-ws-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-detail"},{"id":"fico-ws-prop-actions","tabLabel":"扩展操作","icon":"action-settings","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-actions"},{"id":"fico-ws-prop-fx","tabLabel":"外币管理","icon":"currency","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-fx"},{"id":"fico-ws-prop-budget","tabLabel":"预算管理","icon":"money-bills","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-budget"}]}},"type":"workspace-node","name":"fico-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-fico-ws', '/7484975236341829632/7484975236341829659', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829660', 'fi-gl-rpt-designer', '报表设计器', 'table-chart', NULL, 28, '{"caption":"报表设计器","workspace":{"id":"rpt_designer","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-model","id":"rpt-designer-model"},"explorer":{"caption":"模板目录","icon":"tree","views":[{"id":"rpt-designer-explorer","tabLabel":"模板","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-explorer"}]},"content":{"caption":"设计画布","icon":"edit","views":[{"id":"rpt-designer-content","tabLabel":"设计","icon":"edit","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-content"}]},"property":{"caption":"公式向导/属性","icon":"detail-view","views":[{"id":"rpt-designer-prop","tabLabel":"配置","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-prop"}]}},"type":"workspace-node","name":"rpt-designer"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-rpt-designer', '/7484975236341829632/7484975236341829660', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829661', 'fi-gl-rpt-spreadjs-designer', '报表设计器（SpreadJS）', 'tabler-outline/file-spreadsheet', NULL, 29, '{"caption":"报表设计器（SpreadJS）","workspace":{"id":"rpt_spreadjs_designer","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-model","id":"rpt-spreadjs-designer-model"},"explorer":{"caption":"报表模板","icon":"tree","views":[{"id":"rpt-spreadjs-designer-explorer","tabLabel":"模板","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-explorer"}]},"content":{"caption":"设计画布","icon":"edit","views":[{"id":"rpt-spreadjs-designer-content","tabLabel":"设计","icon":"edit","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-content"}]},"property":{"caption":"公式向导/样式","icon":"detail-view","views":[{"id":"rpt-spreadjs-designer-prop","tabLabel":"配置","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-prop"}]}},"type":"workspace-node","name":"rpt-spreadjs-designer"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-rpt-spreadjs-designer', '/7484975236341829632/7484975236341829661', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829662', 'fi-gl-rpt-design-workbench', '报表设计工作台', 'table-chart', NULL, 30, '{"caption":"报表设计工作台","workspace":{"id":"rpt_design_workbench","model":{"type":"native_pages","native_page":"portal.rpt.design-workbench","id":"rpt-design-workbench-model","view":"content","props":{"mode":"model"}},"explorer":{"caption":"报表类别","icon":"folder","views":[{"id":"rpt-design-workbench-explorer","tabLabel":"类别","icon":"folder","type":"native_pages","native_page":"portal.rpt.design-workbench","view":"explorer"}]},"content":{"caption":"报表设计工作台","icon":"table-chart","views":[{"id":"rpt-design-workbench-content","tabLabel":"报表","icon":"table-chart","type":"native_pages","native_page":"portal.rpt.design-workbench","view":"content"}]},"property":{"caption":"报表属性","icon":"detail-view","views":[{"id":"rpt-design-workbench-prop","tabLabel":"属性","icon":"detail-view","type":"native_pages","native_page":"portal.rpt.design-workbench","view":"property"}]}},"type":"workspace-node","name":"rpt-design-workbench"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-rpt-design-workbench', '/7484975236341829632/7484975236341829662', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829663', 'fi-gl-flow-design-workbench', '流程设计工作台', 'workflow-tasks', NULL, 31, '{"caption":"流程设计工作台","workspace":{"id":"flow_design_workbench","explorer":{"caption":"流程定义","icon":"tree","views":[{"id":"flow-design-workbench-explorer","tabLabel":"定义","icon":"tree","type":"native_pages","native_page":"portal.flow.design-workbench","view":"explorer"}]},"content":{"caption":"流程设计工作台","icon":"workflow-tasks","views":[{"id":"flow-design-workbench-content","tabLabel":"流程图","icon":"workflow-tasks","type":"native_pages","native_page":"portal.flow.design-workbench","view":"content"}]},"property":{"caption":"节点属性","icon":"detail-view","views":[{"id":"flow-design-workbench-prop","tabLabel":"属性","icon":"detail-view","type":"native_pages","native_page":"portal.flow.design-workbench","view":"property"}]}},"type":"workspace-node","name":"flow-design-workbench"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-flow-design-workbench', '/7484975236341829632/7484975236341829663', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829664', 'fi-gl-rpt-app-workbench', '报表应用工作台', 'table-chart', NULL, 32, '{"caption":"报表应用工作台","workspace":{"id":"rpt_app_workbench","model":{"type":"native_pages","native_page":"portal.rpt.report-app-workbench","id":"rpt-app-workbench-model","view":"content","props":{"mode":"data"}},"explorer":{"caption":"组织与期间","icon":"tree","views":[{"id":"rpt-app-workbench-explorer","tabLabel":"组织/期间","icon":"tree","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"explorer"}]},"content":{"caption":"报表应用工作台","icon":"table-chart","views":[{"id":"rpt-app-workbench-content","tabLabel":"报表","icon":"table-chart","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"content"}]},"property":{"caption":"属性","icon":"detail-view","views":[{"id":"rpt-app-workbench-prop","tabLabel":"报表属性","icon":"detail-view","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"property"},{"id":"rpt-app-workbench-prop-org","tabLabel":"组织机构","icon":"tree","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"propertyOrg"}]}},"type":"workspace-node","name":"rpt-app-workbench"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-rpt-app-workbench', '/7484975236341829632/7484975236341829664', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829665', 'fi-gl-fico-ws-doc', 'ERP凭证(doc服务版)', 'business-objects-experience', NULL, 33, '{"caption":"ERP凭证(doc服务版)","workspace":{"id":"fico_ws_doc","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-model","id":"fico-ws-doc-model"},"explorer":{"caption":"凭证批","icon":"list","views":[{"id":"fico-ws-doc-explorer","tabLabel":"凭证批","icon":"list","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-explorer"}]},"content":{"caption":"凭证","icon":"document-text","views":[{"id":"fico-ws-doc-content","tabLabel":"凭证视图","icon":"document-text","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-content"},{"id":"fico-ws-doc-source","tabLabel":"凭证数据","icon":"source-code","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-source"}]},"property":{"caption":"属性/操作","icon":"detail-view","views":[{"id":"fico-ws-doc-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-prop-detail"},{"id":"fico-ws-doc-prop-actions","tabLabel":"扩展操作","icon":"action-settings","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-prop-actions"},{"id":"fico-ws-doc-prop-fx","tabLabel":"外币管理","icon":"currency","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-prop-fx"},{"id":"fico-ws-doc-prop-budget","tabLabel":"预算管理","icon":"money-bills","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-doc-prop-budget"}]}},"type":"workspace-node","name":"fico-ws-doc"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-fico-ws-doc', '/7484975236341829632/7484975236341829665', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829666', 'fi-gl-dict-ws', '合并组织机构维护', 'org-chart', NULL, 34, '{"caption":"合并组织机构维护","workspace":{"id":"dict_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.dictws-model","id":"dictws-model"},"explorer":{"caption":"层级树","icon":"tree","views":[{"id":"dictws-explorer","tabLabel":"层级树","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.dictws-explorer"}]},"content":{"caption":"节点列表","icon":"table-view","views":[{"id":"dictws-content","tabLabel":"节点表格","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictws-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"dictws-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictws-prop-detail"}]}},"type":"workspace-node","name":"dict-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dict-ws', '/7484975236341829632/7484975236341829666', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829667', 'fi-gl-acct-ws', '会计核算管理', 'business-objects-experience', NULL, 35, '{"caption":"会计核算管理","workspace":{"id":"acct_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.acctws-model","id":"acctws-model"},"explorer":{"caption":"合并组织","icon":"org-chart","views":[{"id":"acctws-explorer","tabLabel":"合并组织","icon":"org-chart","type":"html_pages","html_page":"fi.cmxfico.gl.acctws-explorer"}]},"content":{"caption":"科目核算","icon":"business-objects-experience","views":[{"id":"acctws-content","tabLabel":"科目核算","icon":"business-objects-experience","type":"html_pages","html_page":"fi.cmxfico.gl.acctws-content"}]},"property":{"caption":"科目详情","icon":"detail-view","views":[{"id":"acctws-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.acctws-prop-detail"}]}},"type":"workspace-node","name":"acct-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-acct-ws', '/7484975236341829632/7484975236341829667', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829668', 'fi-gl-glacct-ws', '总账科目维护', 'account', NULL, 36, '{"caption":"总账科目维护","workspace":{"id":"glacct_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.glacct-model","id":"glacct-model"},"explorer":{"caption":"科目树","icon":"tree","views":[{"id":"glacct-explorer","tabLabel":"科目树","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.glacct-explorer"}]},"content":{"caption":"子科目","icon":"table-view","views":[{"id":"glacct-content","tabLabel":"子科目表格","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.glacct-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"glacct-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.glacct-prop-detail"}]}},"type":"workspace-node","name":"glacct-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-glacct-ws', '/7484975236341829632/7484975236341829668', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829669', 'fi-gl-dictflat-ws', '币种字典维护(平级)', 'currency', NULL, 37, '{"caption":"币种字典维护(平级)","workspace":{"id":"dictflat_ws","explorerWidth":240,"propertyWidth":320,"model":{"type":"html_pages","html_page":"fi.cmxfico.gl.dictflat-model","id":"dictflat-model"},"explorer":{"caption":"检索","icon":"filter","views":[{"id":"dictflat-explorer","tabLabel":"检索","icon":"filter","type":"html_pages","html_page":"fi.cmxfico.gl.dictflat-explorer"}]},"content":{"caption":"币种列表","icon":"table-view","views":[{"id":"dictflat-content","tabLabel":"币种列表","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictflat-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"dictflat-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictflat-prop-detail"}]}},"type":"workspace-node","name":"dictflat-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dictflat-ws', '/7484975236341829632/7484975236341829669', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829670', 'fi-gl-dicttree-ws', '总账科目维护(自分级)', 'tree', NULL, 38, '{"caption":"总账科目维护(自分级)","workspace":{"id":"dicttree_ws","explorerWidth":280,"propertyWidth":320,"model":{"type":"html_pages","html_page":"fi.cmxfico.gl.dicttree-model","id":"dicttree-model"},"explorer":{"caption":"科目树","icon":"tree","views":[{"id":"dicttree-explorer","tabLabel":"科目树","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.dicttree-explorer"}]},"content":{"caption":"子科目","icon":"table-view","views":[{"id":"dicttree-content","tabLabel":"子科目表格","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.dicttree-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"dicttree-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.dicttree-prop-detail"}]}},"type":"workspace-node","name":"dicttree-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dicttree-ws', '/7484975236341829632/7484975236341829670', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829671', 'fi-gl-dictcls-ws', '总账科目维护(带分类)', 'customer', NULL, 39, '{"caption":"总账科目维护(带分类)","workspace":{"id":"dictcls_ws","explorerWidth":260,"propertyWidth":320,"model":{"type":"html_pages","html_page":"fi.cmxfico.gl.dictcls-model","id":"dictcls-model"},"explorer":{"caption":"伙伴分类","icon":"tree","views":[{"id":"dictcls-explorer","tabLabel":"伙伴分类","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.dictcls-explorer"}]},"content":{"caption":"合作伙伴","icon":"table-view","views":[{"id":"dictcls-content","tabLabel":"伙伴列表","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictcls-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"dictcls-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictcls-prop-detail"}]}},"type":"workspace-node","name":"dictcls-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dictcls-ws', '/7484975236341829632/7484975236341829671', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829672', 'fi-gl-dictrel-ws', '核算主体分组关系维护', 'org-chart', NULL, 40, '{"caption":"核算主体分组关系维护","workspace":{"id":"dictrel_ws","explorerWidth":260,"propertyWidth":340,"model":{"type":"html_pages","html_page":"fi.cmxfico.gl.dictrel-model","id":"dictrel-model"},"explorer":{"caption":"主分组","icon":"tree","views":[{"id":"dictrel-explorer","tabLabel":"主分组","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.dictrel-explorer"}]},"content":{"caption":"关系成员","icon":"table-view","views":[{"id":"dictrel-content","tabLabel":"关系成员","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictrel-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"dictrel-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictrel-prop-detail"}]}},"type":"workspace-node","name":"dictrel-ws"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-gl-dictrel-ws', '/7484975236341829632/7484975236341829672', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829673', 'fi-cmxfico-voucher-doc-json', '会计凭证①(html·DataSet)', 'tabler-outline/receipt', NULL, 41, '{"caption":"会计凭证①(html·DataSet)","workspace":{"content":{"caption":"会计凭证业务单据（html_pages · sqlx DataSet + JSON）","icon":"document","views":[{"tabLabel":"凭证(html_pages)","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.voucher-doc"}]}},"type":"workspace-node","name":"voucher-doc-json"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-voucher-doc-json', '/7484975236341829632/7484975236341829673', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829674', 'fi-cmxfico-voucher-native-json', '会计凭证②(native·DataSet)', 'tabler-outline/receipt', NULL, 42, '{"caption":"会计凭证②(native·DataSet)","workspace":{"content":{"caption":"会计凭证业务单据（通用页 · sqlx DataSet + JSON）","icon":"document","views":[{"tabLabel":"凭证(通用页·DataSet)","icon":"table-view","type":"native_pages","native_page":"portal.doc.doc-loader","view":"content","props":{"domain":"fi","application":"cmxfico","module":"gl","file":"cmxfico_doc_meta_v1.json","dbId":"fico-db","apiPath":"/api/doc/data/sqlx-dataset-json"}}]}},"type":"workspace-node","name":"voucher-native-json"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-voucher-native-json', '/7484975236341829632/7484975236341829674', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829675', 'fi-cmxfico-voucher-doc-sqlxbin', '会计凭证③(html·Sqlx二进制)', 'tabler-outline/receipt', NULL, 43, '{"caption":"会计凭证③(html·Sqlx二进制)","workspace":{"content":{"caption":"会计凭证业务单据（html_pages · sqlx ZmcDataSet + 二进制）","icon":"document","views":[{"tabLabel":"凭证(html·Sqlx二进制)","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.voucher-doc-sqlxbin"}]}},"type":"workspace-node","name":"voucher-doc-sqlxbin"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-voucher-doc-sqlxbin', '/7484975236341829632/7484975236341829675', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829676', 'fi-cmxfico-voucher-native-bin', '会计凭证④(native·tokio二进制)', 'tabler-outline/receipt', NULL, 44, '{"caption":"会计凭证④(native·tokio二进制)","workspace":{"content":{"caption":"会计凭证业务单据（通用页 · tokio ZmcDataSet + 二进制）","icon":"document","views":[{"tabLabel":"凭证(通用页·tokio二进制)","icon":"table-view","type":"native_pages","native_page":"portal.doc.doc-loader","view":"content","props":{"domain":"fi","application":"cmxfico","module":"gl","file":"cmxfico_doc_meta_v1.json","dbId":"fico-db","apiPath":"/api/doc/data/tokio-zmc-msgpack","binary":true}}]}},"type":"workspace-node","name":"voucher-native-bin"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-voucher-native-bin', '/7484975236341829632/7484975236341829676', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829677', 'fi-cmxfico-voucher-native-zmcjson', '会计凭证⑤(native·Zmc→JSON)', 'tabler-outline/receipt', NULL, 45, '{"caption":"会计凭证⑤(native·Zmc→JSON)","workspace":{"content":{"caption":"会计凭证业务单据（通用页 · ZmcDataSet 零拷贝装载 + 纯 JSON 出口）","icon":"document","views":[{"tabLabel":"凭证(通用页·Zmc→JSON)","icon":"table-view","type":"native_pages","native_page":"portal.doc.doc-loader","view":"content","props":{"domain":"fi","application":"cmxfico","module":"gl","file":"cmxfico_doc_meta_v1.json","dbId":"fico-db","apiPath":"/api/doc/data/tokio-zmc-json"}}]}},"type":"workspace-node","name":"voucher-native-zmcjson"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-voucher-native-zmcjson', '/7484975236341829632/7484975236341829677', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829678', 'fi-cmxfico-doc-service-test-native', '加载服务自检(native)', 'tabler-outline/checklist', NULL, 46, '{"caption":"加载服务自检(native)","workspace":{"content":{"caption":"业务单据加载服务 · 全后端能力自检（native_pages）","icon":"sys-monitor","views":[{"tabLabel":"自检(native)","icon":"sys-monitor","type":"native_pages","native_page":"portal.doc.doc-service-test","view":"content","props":{"domain":"fi","application":"cmxfico","module":"gl","file":"cmxfico_doc_meta_v1.json","dbId":"fico-db"}}]}},"type":"workspace-node","name":"doc-service-test-native"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-doc-service-test-native', '/7484975236341829632/7484975236341829678', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829679', 'fi-cmxfico-doc-service-test-html', '加载服务自检(html)', 'tabler-outline/checklist', NULL, 47, '{"caption":"加载服务自检(html)","workspace":{"content":{"caption":"业务单据加载服务 · 全后端能力自检（html_pages）","icon":"sys-monitor","views":[{"tabLabel":"自检(html)","icon":"sys-monitor","type":"html_pages","html_page":"fi.cmxfico.gl.doc-service-test"}]}},"type":"workspace-node","name":"doc-service-test-html"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-doc-service-test-html', '/7484975236341829632/7484975236341829679', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829680', 'fi-cmxfico-gl-employee', 'AI员工信息表单', 'tabler-outline/users', NULL, 48, '{"caption":"AI员工信息表单","workspace":{"content":{"caption":"AI员工信息表单","icon":"tabler-outline/users","views":[{"tabLabel":"员工信息","icon":"tabler-outline/users","type":"html_pages","html_page":"fi.cmxfico.gl.employee"}]}},"type":"workspace-node","name":"employee"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829632', 'gl', 2, 1, '/gl/fi-cmxfico-gl-employee', '/7484975236341829632/7484975236341829680', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829681', 'grp-ignite-demo', 'Ignite 组件演示', 'tabler-outline/flame', NULL, 2, '{"caption":"Ignite 组件演示","expanded":true,"name":"ignite-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 0, '/grp-ignite-demo', '/7484975236341829681', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829682', 'fi-gl-ignite-combo-editor', 'Ignite 组合框编辑器', 'tabler-outline/select', NULL, 1, '{"caption":"Ignite 组合框编辑器","workspace":{"content":{"caption":"Ignite 组合框编辑器","icon":"form","views":[{"tabLabel":"编辑器演示","icon":"form","type":"html_pages","html_page":"fi.cmxfico.gl.ignite-combo-editor"}]}},"type":"workspace-node","name":"ignite-combo-editor"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829681', 'grp-ignite-demo', 2, 1, '/grp-ignite-demo/fi-gl-ignite-combo-editor', '/7484975236341829681/7484975236341829682', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829683', 'fi-gl-ignite-list', 'Ignite 列表主从', 'tabler-outline/list', NULL, 2, '{"caption":"Ignite 列表主从","workspace":{"content":{"caption":"Ignite 列表主从","icon":"table-view","views":[{"tabLabel":"主从演示","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.ignite-list"}]}},"type":"workspace-node","name":"ignite-list"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829681', 'grp-ignite-demo', 2, 1, '/grp-ignite-demo/fi-gl-ignite-list', '/7484975236341829681/7484975236341829683', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829684', 'grp-meta-def', '定义中心', 'tabler-outline/adjustments', NULL, 3, '{"caption":"定义中心","expanded":true,"name":"meta-def"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 0, '/grp-meta-def', '/7484975236341829684', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829685', 'flexible-combination-manager', '弹性组合管理', 'tabler-outline/adjustments-cog', NULL, 1, '{"caption":"弹性组合管理","workspace":{"content":{"caption":"弹性组合管理","icon":"settings","views":[{"tabLabel":"设计","icon":"settings","type":"flexible-combination-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"flexible-combination-source","data":{}},{"tabLabel":"Schema","icon":"database","type":"flexible-combination-schema","data":{}}]},"explorer":{"caption":"档案列表","icon":"tree","views":[{"tabLabel":"档案","icon":"list","type":"flexible-combination-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"flexible-combination-inspector","data":{}},{"tabLabel":"校验/预览","icon":"validate","type":"flexible-combination-verify","data":{}}]}},"type":"workspace-node","name":"flexible-combination-manager"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/flexible-combination-manager', '/7484975236341829684/7484975236341829685', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829686', 'dict-def-manager', '数据字典定义', 'tabler-outline/book', NULL, 2, '{"caption":"数据字典定义","workspace":{"content":{"caption":"数据字典定义","icon":"book","views":[{"tabLabel":"设计","icon":"book","type":"dct-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"dct-def-source","data":{}}]},"explorer":{"caption":"字典列表","icon":"tree","views":[{"tabLabel":"字典","icon":"list","type":"dct-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"dct-def-inspector","data":{}}]}},"type":"workspace-node","name":"dict-def-manager"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/dict-def-manager', '/7484975236341829684/7484975236341829686', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829687', 'doc-def-manager', '业务单据定义', 'tabler-outline/file-invoice', NULL, 3, '{"caption":"业务单据定义","workspace":{"content":{"caption":"业务单据定义","icon":"document","views":[{"tabLabel":"设计","icon":"document","type":"doc-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"doc-def-source","data":{}}]},"explorer":{"caption":"单据列表","icon":"tree","views":[{"tabLabel":"单据","icon":"list","type":"doc-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"doc-def-inspector","data":{}}]}},"type":"workspace-node","name":"doc-def-manager"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/doc-def-manager', '/7484975236341829684/7484975236341829687', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829688', 'base-dct-def-manager', '字典基础元数据', 'tabler-outline/template', NULL, 4, '{"caption":"字典基础元数据","workspace":{"content":{"caption":"字典基础元数据","icon":"template","views":[{"tabLabel":"设计","icon":"template","type":"base-dct-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"base-dct-def-source","data":{}}]},"explorer":{"caption":"基础文件","icon":"tree","views":[{"tabLabel":"字段集","icon":"list","type":"base-dct-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"base-dct-def-inspector","data":{}}]}},"type":"workspace-node","name":"base-dct-def-manager"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/base-dct-def-manager', '/7484975236341829684/7484975236341829688', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829689', 'base-dct-native-pages-manager', '字典基础元数据(native_pages)', 'tabler-outline/template', NULL, 5, '{"caption":"字典基础元数据(native_pages)","workspace":{"content":{"caption":"字典基础元数据(native_pages)","icon":"template","views":[{"tabLabel":"设计","icon":"template","type":"native_pages","native_page":"definition.base-dct-native","view":"manager","props":{}},{"tabLabel":"源码","icon":"source-code","type":"native_pages","native_page":"definition.base-dct-native","view":"source","props":{}}]},"explorer":{"caption":"基础文件","icon":"tree","views":[{"tabLabel":"字段集","icon":"list","type":"native_pages","native_page":"definition.base-dct-native","view":"list","props":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"native_pages","native_page":"definition.base-dct-native","view":"inspector","props":{}}]}},"type":"workspace-node","name":"base-dct-native-pages-manager"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/base-dct-native-pages-manager', '/7484975236341829684/7484975236341829689', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829690', 'native-pages-linkage-demo', '原生页面联动演示(native_pages·CE+context)', 'tabler-outline/topology-star-3', NULL, 6, '{"caption":"原生页面联动演示(native_pages·CE+context)","workspace":{"explorer":{"caption":"产品列表","icon":"list","views":[{"tabLabel":"产品","icon":"product","type":"native_pages","native_page":"demo.product-explorer","view":"default","props":{}}]},"content":{"caption":"产品详情 / 整页演示","icon":"detail-view","views":[{"tabLabel":"详情(content CE)","icon":"detail-view","type":"native_pages","native_page":"demo.product-content","view":"default","props":{}},{"tabLabel":"整页HTML(iframe)","icon":"internet-browser","type":"native_pages","native_page":"demo.interactive","view":"default","props":{}}]}},"type":"workspace-node","name":"native-pages-linkage-demo"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/native-pages-linkage-demo', '/7484975236341829684/7484975236341829690', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829691', 'base-doc-def-manager', '单据基础元数据', 'tabler-outline/template', NULL, 7, '{"caption":"单据基础元数据","workspace":{"content":{"caption":"单据基础元数据","icon":"template","views":[{"tabLabel":"设计","icon":"template","type":"base-doc-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"base-doc-def-source","data":{}}]},"explorer":{"caption":"基础文件","icon":"tree","views":[{"tabLabel":"字段集","icon":"list","type":"base-doc-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"base-doc-def-inspector","data":{}}]}},"type":"workspace-node","name":"base-doc-def-manager"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/base-doc-def-manager', '/7484975236341829684/7484975236341829691', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829692', 'custom-page-designer', '自定义页面设计', 'tabler-outline/layout-dashboard', NULL, 8, '{"caption":"自定义页面设计","workspace":{"content":{"caption":"自定义页面设计","icon":"business-objects-experience","views":[{"tabLabel":"页面","icon":"business-objects-experience","type":"custom-page-designer","data":{}}]}},"type":"workspace-node","name":"custom-page-designer"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/custom-page-designer', '/7484975236341829684/7484975236341829692', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829693', 'fi-gl-account-def', '科目弹性组合', 'tabler-outline/book', NULL, 9, '{"caption":"科目弹性组合","workspace":{"content":{"caption":"科目弹性组合","icon":"settings","views":[{"tabLabel":"定义","icon":"settings","type":"html_pages","html_page":"fi.cmxfico.gl.account-def"}]}},"type":"workspace-node","name":"account-def"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/fi-gl-account-def', '/7484975236341829684/7484975236341829693', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829694', 'fi-gl-trade-def', '交易弹性组合', 'tabler-outline/shopping-cart-cog', NULL, 10, '{"caption":"交易弹性组合","workspace":{"content":{"caption":"交易弹性组合","icon":"settings","views":[{"tabLabel":"定义","icon":"settings","type":"html_pages","html_page":"fi.cmxfico.gl.trade-def"}]}},"type":"workspace-node","name":"trade-def"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/fi-gl-trade-def', '/7484975236341829684/7484975236341829694', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829695', 'fi-gl-ccm-attrs-test', 'CCM全属性测试', 'tabler-outline/test-pipe', NULL, 11, '{"caption":"CCM全属性测试","workspace":{"content":{"caption":"CmxColumnModel 全属性 + select/date 编辑器 + 表单校验测试","icon":"table-view","views":[{"tabLabel":"测试","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.ccm-attrs-test"}]}},"type":"workspace-node","name":"ccm-attrs-test"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/fi-gl-ccm-attrs-test', '/7484975236341829684/7484975236341829695', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829696', 'workspace-node-editor', '工作区节点编辑', 'tabler-outline/sitemap', NULL, 12, '{"caption":"工作区节点编辑","type":"workspace-node-editor","name":"workspace-node-editor"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829684', 'grp-meta-def', 2, 1, '/grp-meta-def/workspace-node-editor', '/7484975236341829684/7484975236341829696', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829697', 'grp-pages', '页面管理', 'documents', NULL, 4, '{"caption":"页面管理","expanded":true,"name":"page-mgmt"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 0, '/grp-pages', '/7484975236341829697', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829698', 'page-home', '首页', NULL, NULL, 1, '{"caption":"首页","name":"home"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829697', 'grp-pages', 2, 1, '/grp-pages/page-home', '/7484975236341829697/7484975236341829698', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829699', 'page-users', '用户管理', NULL, 'MENU.PAGES.USERS', 2, '{"caption":"用户管理","name":"users"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829697', 'grp-pages', 2, 1, '/grp-pages/page-users', '/7484975236341829697/7484975236341829699', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829700', 'page-acl', '权限配置', NULL, 'MENU.PAGES.ACL', 3, '{"caption":"权限配置","name":"acl"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829697', 'grp-pages', 2, 1, '/grp-pages/page-acl', '/7484975236341829697/7484975236341829700', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829701', 'page-reports', '报表中心', NULL, NULL, 4, '{"caption":"报表中心","name":"reports"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829697', 'grp-pages', 2, 1, '/grp-pages/page-reports', '/7484975236341829697/7484975236341829701', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829702', 'portal-dam-registry', 'DAM 注册管理中心', 'tree', NULL, 5, '{"caption":"DAM 注册管理中心","workspace":{"explorer":{"caption":"DAM 导航","icon":"tree","views":[{"tabLabel":"DAM","icon":"tree","type":"native_pages","native_page":"portal.dam.registry-center","view":"explorer","props":{}}]},"content":{"caption":"DAM 注册管理中心","icon":"tree","views":[{"tabLabel":"注册中心","icon":"tree","type":"native_pages","native_page":"portal.dam.registry-center","view":"manager","props":{}}]},"property":{"caption":"资源","icon":"documents","views":[{"tabLabel":"资源","icon":"documents","type":"native_pages","native_page":"portal.dam.registry-center","view":"property","props":{}}]}},"type":"workspace-node","name":"dam-registry"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 1, '/portal-dam-registry', '/7484975236341829702', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829703', 'grp-components', '组件库', 'puzzle', NULL, 6, '{"caption":"组件库","expanded":false,"name":"component-lib"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 0, '/grp-components', '/7484975236341829703', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829704', 'comp-forms', '表单组件', NULL, NULL, 1, '{"caption":"表单组件","name":"form-widgets"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829703', 'grp-components', 2, 1, '/grp-components/comp-forms', '/7484975236341829703/7484975236341829704', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829705', 'comp-charts', '图表组件', NULL, NULL, 2, '{"caption":"图表组件","name":"charts"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829703', 'grp-components', 2, 1, '/grp-components/comp-charts', '/7484975236341829703/7484975236341829705', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829706', 'grp-services', '服务接入', 'world', NULL, 7, '{"caption":"服务接入","expanded":false,"name":"service-access"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 0, '/grp-services', '/7484975236341829706', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829707', 'svc-rest', 'REST 接口', NULL, NULL, 1, '{"caption":"REST 接口","name":"rest"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829706', 'grp-services', 2, 1, '/grp-services/svc-rest', '/7484975236341829706/7484975236341829707', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829708', 'svc-graphql', 'GraphQL', NULL, NULL, 2, '{"caption":"GraphQL","name":"graphql"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829706', 'grp-services', 2, 1, '/grp-services/svc-graphql', '/7484975236341829706/7484975236341829708', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829709', 'svc-ws', 'WebSocket', NULL, NULL, 3, '{"caption":"WebSocket","name":"websocket"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829706', 'grp-services', 2, 1, '/grp-services/svc-ws', '/7484975236341829706/7484975236341829709', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829710', 'grp-data', '数据资产', 'database', NULL, 8, '{"caption":"数据资产","expanded":false,"name":"data-assets"}'::jsonb, 'fi', 'cmxfico', 'gl', NULL, NULL, 1, 0, '/grp-data', '/7484975236341829710', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829711', 'data-sources', '数据源', NULL, NULL, 1, '{"caption":"数据源","name":"sources"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829710', 'grp-data', 2, 1, '/grp-data/data-sources', '/7484975236341829710/7484975236341829711', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236341829712', 'data-models', '数据模型', NULL, NULL, 2, '{"caption":"数据模型","name":"models"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484975236341829710', 'grp-data', 2, 1, '/grp-data/data-models', '/7484975236341829710/7484975236341829712', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023936', 'report', '报表数据管理', 'tabler-filled/home', NULL, 1, '{"caption":"报表数据管理","expanded":true,"name":"report"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 0, '/report', '/7484975236346023936', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023937', 'portal-console_dup', '系统控制', 'tabler-outline/load-balancer', NULL, 1, '{"caption":"系统控制","workspace":{"prepare":{"caption":"系统控制-原生模式","icon":"tabler-outline/wash-tumble-dry","width":"720px","height":"520px","views":[{"tabLabel":"概览","type":"placeholder","data":{"title":"控制台首页"}},{"tabLabel":"快捷数据","type":"json","data":{"value":{"region":"content","tabs":2}}}]},"content":{"caption":"系统控制-原生模式","icon":"edit","views":[{"tabLabel":"概览","type":"placeholder","data":{"title":"控制台首页"}},{"tabLabel":"快捷数据","type":"json","data":{"value":{"region":"content","tabs":2}}}]},"explorer":{"caption":"导航","icon":"detail-view","views":[{"tabLabel":"概览","icon":"detail-view","type":"placeholder","data":{"note":"Explorer 多视图 — 签 1"}},{"tabLabel":"模型","icon":"source-code","type":"code","data":{"code":"// 示例代码视图\nentity PortalPage {\n  id: string;\n  title: string;\n}"}}]},"property":{"caption":"智能体","icon":"documents","views":[{"tabLabel":"页面","icon":"document","type":"placeholder","data":{}},{"tabLabel":"说明","icon":"information","type":"markdown","data":{"text":"同一菜单可在四个区域各配置多个视图；底部 Tab 切换。"}}]},"bottom":{"caption":"日志","icon":"log","views":[{"tabLabel":"状态","icon":"log","type":"placeholder","data":{"title":"Bottom 第一视图（类型可与第一不同）"}},{"tabLabel":"概览","icon":"detail-view","type":"placeholder","data":{"note":"Bottom 多视图 — 签 2"}}]}},"name":"console"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/portal-console_dup', '/7484975236346023936/7484975236346023937', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023938', 'user-edit_dup', '编辑用户', 'edit', NULL, 2, '{"caption":"编辑用户","dialogspace":{"title":"编辑用户","icon":"person-placeholder","description":"修改基本信息","content":{"label":"用户信息","icon":"form","views":[{"htmlPageId":"user-form-page"}]}}}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/user-edit_dup', '/7484975236346023936/7484975236346023938', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023939', 'deploy-config_dup', '部署配置', 'shipping-status', NULL, 3, '{"caption":"部署配置","dialogspace":{"title":"部署配置向导","icon":"shipping-status","description":"v2.4.1 → 生产环境","buttons":[{"id":"help","text":"帮助","icon":"sys-help","design":"Transparent"},{"id":"preview","text":"预览","icon":"show","design":"Transparent"}],"explorer":{"label":"环境列表","icon":"navigation-down-arrow","views":[{"htmlPageId":"exp_view1","tabLabel":"环境树"},{"htmlPageId":"exp_view2","tabLabel":"最近"}]},"content":{"label":"配置项","icon":"settings","views":[{"htmlPageId":"ctn_view1","tabLabel":"基本","tabIcon":"form"},{"htmlPageId":"ctn_view2","tabLabel":"变量","tabIcon":"key-user-settings"},{"htmlPageId":"pre_view1","tabLabel":"密钥","tabIcon":"locked"}]},"property":{"label":"属性","icon":"hint","views":[{"htmlPageId":"pro_view1"},{"htmlPageId":"pro_view2"}]},"bottom":{"views":[{"htmlPageId":"btm_view1","tabLabel":"日志"},{"htmlPageId":"btm_view2","tabLabel":"检查"}]},"confirmText":"开始部署","cancelText":"取消","dialogWidth":"90vw","dialogHeight":"85vh","explorerWidth":240,"propertyWidth":300,"footerHeight":140}}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/deploy-config_dup', '/7484975236346023936/7484975236346023939', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023940', 'portal-overview_dup', '页面组装测试', 'tabler-outline/nut', NULL, 4, '{"caption":"页面组装测试","workspace":{"id":"test_workspace","params":{"userName":"Jeff","defaultMessage":"Hello from menu params"},"prepare":{"caption":"准备页面组装测试","icon":"home","height":"500px","width":"600px","views":[{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"pre_view1"},{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"pre_view2"}]},"model":{"type":"html_pages","html_page":"ws-shared-model","id":"ws-model"},"inner":{"caption":"内部用对话框表单，打开菜单时只加载，不显示","icon":"edit","views":[{"tabLabel":"inner1","icon":"detail-view","type":"html_pages","html_page":"inner_page2"},{"tabLabel":"inner2","icon":"detail-view","type":"html_pages","html_page":"inner_page1"}]},"embed":{"caption":"内部嵌入表单，打开菜单时在页面中的embed_page组件展示","icon":"edit","views":[{"tabLabel":"embed1","icon":"detail-view","type":"html_pages","html_page":"embed_page1"},{"tabLabel":"测试页面","icon":"detail-view","type":"html_pages","html_page":"embed_page2"}]},"content":{"caption":"系统概览-页面模式","icon":"edit","views":[{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"ctn_view1"},{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"ctn_view2"}]},"explorer":{"caption":"导航","icon":"detail-view","views":[{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"exp_view1"},{"tabLabel":"模型","icon":"source-code","type":"html_pages","html_page":"exp_view2"}]},"property":{"caption":"智能体","icon":"documents","views":[{"tabLabel":"页面","icon":"document","type":"html_pages","html_page":"pro_view1"},{"tabLabel":"说明","icon":"information","type":"html_pages","html_page":"pro_view2"}]},"bottom":{"caption":"日志","icon":"log","views":[{"tabLabel":"状态","icon":"log","type":"html_pages","html_page":"btm_view1"},{"tabLabel":"概览","icon":"detail-view","type":"html_pages","html_page":"btm_view2"}]},"float":{"views":[{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"flt_view1"},{"tabLabel":"快捷","icon":"detail-view","type":"html_pages","html_page":"flt_view2"}]}},"type":"workspace-node","name":"overview"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/portal-overview_dup', '/7484975236346023936/7484975236346023940', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023941', 'outlook-mailbox_dup', 'Outlook邮箱', 'email', NULL, 5, '{"caption":"Outlook邮箱","workspace":{"explorer":{"caption":"邮箱目录","icon":"email","views":[{"tabLabel":"目录","icon":"email","type":"html_pages","html_page":"outlook-mail-explorer"}]},"content":{"caption":"邮件列表与正文","icon":"list","views":[{"tabLabel":"邮件","icon":"list","type":"html_pages","html_page":"outlook-mail-content"}]}},"type":"workspace-node","name":"outlook-mailbox"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/outlook-mailbox_dup', '/7484975236346023936/7484975236346023941', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023942', 'demo-master-slave_dup', '主从表单生成测试', 'table-view', NULL, 6, '{"caption":"主从表单生成测试","workspace":{"content":{"caption":"主从表格示例","icon":"table-view","views":[{"tabLabel":"主从表格","icon":"table-view","type":"html_pages","html_page":"demo-master-slave"}]}},"type":"workspace-node","name":"master-slave-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/demo-master-slave_dup', '/7484975236346023936/7484975236346023942', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023943', 'combo-box-demo_dup', 'cmx-combo-box 三模式演示', 'tabler-outline/select', NULL, 7, '{"caption":"cmx-combo-box 三模式演示","workspace":{"content":{"caption":"cmx-combo-box 三模式演示","icon":"tabler-outline/select","views":[{"tabLabel":"combo-box 演示","icon":"tabler-outline/select","type":"html_pages","html_page":"combo-box-demo"}]}},"type":"workspace-node","name":"combo-box-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/combo-box-demo_dup', '/7484975236346023936/7484975236346023943', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023944', 'tabulator-treegrid-demo_dup', 'cmx-tabulator 会计科目树', 'tabler-outline/binary-tree', NULL, 8, '{"caption":"cmx-tabulator 会计科目树","workspace":{"content":{"caption":"cmx-tabulator 会计科目树（cf_gl_account 自分级字典）","icon":"tabler-outline/binary-tree","views":[{"tabLabel":"会计科目树","icon":"tabler-outline/binary-tree","type":"html_pages","html_page":"tabulator-treegrid-demo"}]}},"type":"workspace-node","name":"tabulator-treegrid-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/tabulator-treegrid-demo_dup', '/7484975236346023936/7484975236346023944', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023945', 'dict-select-demo_dup', 'cmx-dict-select 数据字典选择演示', 'tabler-outline/list-search', NULL, 9, '{"caption":"cmx-dict-select 数据字典选择演示","workspace":{"content":{"caption":"cmx-dict-select 数据字典选择演示","icon":"tabler-outline/list-search","views":[{"tabLabel":"字典选择演示","icon":"tabler-outline/list-search","type":"html_pages","html_page":"dict-select-demo"}]}},"type":"workspace-node","name":"dict-select-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/dict-select-demo_dup', '/7484975236346023936/7484975236346023945', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023946', 'cmx-input-editors-demo_dup', '录入控件演示（文本/数字/日期/日期时间）', 'tabler-outline/forms', NULL, 10, '{"caption":"录入控件演示（文本/数字/日期/日期时间）","workspace":{"content":{"caption":"录入控件演示（文本/数字/日期/日期时间）","icon":"tabler-outline/forms","views":[{"tabLabel":"录入控件演示","icon":"tabler-outline/forms","type":"html_pages","html_page":"cmx-input-editors-demo"}]}},"type":"workspace-node","name":"cmx-input-editors-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/cmx-input-editors-demo_dup', '/7484975236346023936/7484975236346023946', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023947', 'cmx-grid-form-linkage-demo_dup', 'grid↔form 联动（四录入组件·模型面板）', 'tabler-outline/table-options', NULL, 11, '{"caption":"grid↔form 联动（四录入组件·模型面板）","workspace":{"content":{"caption":"grid↔form 联动（四录入组件·模型面板）","icon":"tabler-outline/table-options","views":[{"tabLabel":"grid↔form 联动","icon":"tabler-outline/table-options","type":"html_pages","html_page":"cmx-grid-form-linkage-demo"}]}},"type":"workspace-node","name":"cmx-grid-form-linkage-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/cmx-grid-form-linkage-demo_dup', '/7484975236346023936/7484975236346023947', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023948', 'fi-gl-voucher_dup', '财务会计凭证', 'tabler-outline/file-spreadsheet', NULL, 12, '{"caption":"财务会计凭证","workspace":{"content":{"caption":"财务会计凭证","icon":"table-view","views":[{"tabLabel":"凭证","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.voucher-neo"}]}},"type":"workspace-node","name":"voucher"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-voucher_dup', '/7484975236346023936/7484975236346023948', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023949', 'fi-gl-voucher-demo_dup', '会计凭证（三层·维度选择）', 'tabler-outline/stack-2', NULL, 13, '{"caption":"会计凭证（三层·维度选择）","workspace":{"content":{"caption":"会计凭证（头/分录/明细 三层）","icon":"table-view","views":[{"tabLabel":"凭证","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.gl-voucher-demo"}]}},"type":"workspace-node","name":"gl-voucher-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-voucher-demo_dup', '/7484975236346023936/7484975236346023949', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023950', 'fi-gl-voucher-v1-demo_dup', '会计凭证（v1 单表多字典）', 'tabler-outline/database', NULL, 14, '{"caption":"会计凭证（v1 单表多字典）","workspace":{"content":{"caption":"会计凭证 v1(单表多字典)","icon":"table-view","views":[{"tabLabel":"凭证","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.gl-voucher-v1-demo"}]}},"type":"workspace-node","name":"gl-voucher-v1-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-voucher-v1-demo_dup', '/7484975236346023936/7484975236346023950', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023951', 'fi-gl-trade_dup', '交易单据', 'tabler-outline/shopping-cart', NULL, 15, '{"caption":"交易单据","workspace":{"content":{"caption":"交易单据","icon":"table-view","views":[{"tabLabel":"单据","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.trade-neo"}]}},"type":"workspace-node","name":"trade"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-trade_dup', '/7484975236346023936/7484975236346023951', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023952', 'fi-gl-travel-expense_dup', '差旅费报销', 'tabler-outline/plane-departure', NULL, 16, '{"caption":"差旅费报销","workspace":{"content":{"caption":"差旅费报销","icon":"table-view","views":[{"tabLabel":"报销单","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.travel-expense-neo"}]}},"type":"workspace-node","name":"travel-expense"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-travel-expense_dup', '/7484975236346023936/7484975236346023952', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023953', 'fi-gl-trade-form_dup', '交易单据（表单视图）', 'tabler-outline/forms', NULL, 17, '{"caption":"交易单据（表单视图）","workspace":{"content":{"caption":"交易单据（表单视图）","icon":"form","views":[{"tabLabel":"单据","icon":"form","type":"html_pages","html_page":"fi.cmxfico.gl.trade-form"}]}},"type":"workspace-node","name":"trade-form"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-trade-form_dup', '/7484975236346023936/7484975236346023953', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023954', 'fi-gl-dict-editor-demo_dup', '字典选择编辑器演示', 'tabler-outline/edit', NULL, 18, '{"caption":"字典选择编辑器演示","workspace":{"content":{"caption":"cmx-dict-select 编辑器演示（grid + form）","icon":"value-help","views":[{"tabLabel":"编辑器演示","icon":"value-help","type":"html_pages","html_page":"fi.cmxfico.gl.dict-editor-demo"}]}},"type":"workspace-node","name":"dict-editor-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-dict-editor-demo_dup', '/7484975236346023936/7484975236346023954', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023955', 'fi-gl-master-data-demo_dup', '总账主数据演示', 'tabler-outline/database-search', NULL, 19, '{"caption":"总账主数据演示","workspace":{"content":{"caption":"总账主数据演示","icon":"value-help","views":[{"tabLabel":"主数据演示","icon":"value-help","type":"html_pages","html_page":"fi.cmxfico.gl.gl-master-data-demo"}]}},"type":"workspace-node","name":"gl-master-data-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-master-data-demo_dup', '/7484975236346023936/7484975236346023955', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023956', 'fi-gl-dict-help-test_dup', '主数据帮助测试', 'tabler-outline/test-pipe', NULL, 20, '{"caption":"主数据帮助测试","workspace":{"content":{"caption":"主数据帮助测试","icon":"value-help","views":[{"tabLabel":"字典帮助测试","icon":"search","type":"html_pages","html_page":"fi.cmxfico.gl.dict-help-test"}]}},"type":"workspace-node","name":"dict-help-test"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-dict-help-test_dup', '/7484975236346023936/7484975236346023956', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023957', 'fi-gl-dict-help-lab_dup', '主数据帮助实验室', 'tabler-outline/flask', NULL, 21, '{"caption":"主数据帮助实验室","workspace":{"content":{"caption":"主数据帮助实验室","icon":"simulate","views":[{"tabLabel":"帮助实验室","icon":"simulate","type":"html_pages","html_page":"fi.cmxfico.gl.dict-help-lab"}]}},"type":"workspace-node","name":"dict-help-lab"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-dict-help-lab_dup', '/7484975236346023936/7484975236346023957', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023958', 'fi-gl-meta-model-service-test_dup', '元数据模型服务测试', 'database', NULL, 22, '{"caption":"元数据模型服务测试","workspace":{"content":{"caption":"元数据模型服务测试","icon":"database","views":[{"tabLabel":"功能测试","icon":"database","type":"html_pages","html_page":"fi.cmxfico.gl.meta-model-service-test"}]}},"type":"workspace-node","name":"meta-model-service-test"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-meta-model-service-test_dup', '/7484975236346023936/7484975236346023958', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023959', 'fi-gl-dict-grid-meta_dup', '数据字典动态表格', 'grid', NULL, 23, '{"caption":"数据字典动态表格","workspace":{"content":{"caption":"数据字典动态表格","icon":"grid","views":[{"tabLabel":"字典内容","icon":"grid","type":"html_pages","html_page":"fi.cmxfico.gl.dict-grid-meta"}]}},"type":"workspace-node","name":"dict-grid-meta"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-dict-grid-meta_dup', '/7484975236346023936/7484975236346023959', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023960', 'fi-gl-erp-voucher-cnpc_dup', 'ERP凭证样例(CNPC)', 'receipt', NULL, 24, '{"caption":"ERP凭证样例(CNPC)","workspace":{"content":{"caption":"ERP凭证样例(CNPC)","icon":"receipt","views":[{"tabLabel":"凭证对比","icon":"receipt","type":"html_pages","html_page":"fi.cmxfico.gl.erp-voucher-cnpc"}]}},"type":"workspace-node","name":"erp-voucher-cnpc"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-erp-voucher-cnpc_dup', '/7484975236346023936/7484975236346023960', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023961', 'fi-gl-erp-voucher-cnpc-ms_dup', 'ERP凭证样例(MasterSlave模型版)', 'org-chart', NULL, 25, '{"caption":"ERP凭证样例(MasterSlave模型版)","workspace":{"content":{"caption":"ERP凭证样例(MasterSlave模型版)","icon":"org-chart","views":[{"tabLabel":"四层主从凭证","icon":"org-chart","type":"html_pages","html_page":"fi.cmxfico.gl.erp-voucher-cnpc-ms"}]}},"type":"workspace-node","name":"erp-voucher-cnpc-ms"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-erp-voucher-cnpc-ms_dup', '/7484975236346023936/7484975236346023961', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023962', 'fi-gl-fico-ws_dup', 'ERP凭证(三区工作台)', 'business-objects-experience', NULL, 26, '{"caption":"ERP凭证(三区工作台)","workspace":{"id":"fico_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-model","id":"fico-ws-model"},"explorer":{"caption":"凭证批","icon":"list","views":[{"id":"fico-ws-explorer","tabLabel":"凭证批","icon":"list","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-explorer"}]},"content":{"caption":"凭证","icon":"document-text","views":[{"id":"fico-ws-content","tabLabel":"凭证视图","icon":"document-text","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-content"},{"id":"fico-ws-source","tabLabel":"凭证数据","icon":"source-code","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-source"}]},"property":{"caption":"属性/操作","icon":"detail-view","views":[{"id":"fico-ws-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-detail"},{"id":"fico-ws-prop-actions","tabLabel":"扩展操作","icon":"action-settings","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-actions"},{"id":"fico-ws-prop-fx","tabLabel":"外币管理","icon":"currency","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-fx"},{"id":"fico-ws-prop-budget","tabLabel":"预算管理","icon":"money-bills","type":"html_pages","html_page":"fi.cmxfico.gl.fico-ws-prop-budget"}]}},"type":"workspace-node","name":"fico-ws"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-fico-ws_dup', '/7484975236346023936/7484975236346023962', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023963', 'fi-gl-rpt-designer_dup', '报表设计器', 'table-chart', NULL, 27, '{"caption":"报表设计器","workspace":{"id":"rpt_designer","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-model","id":"rpt-designer-model"},"explorer":{"caption":"模板目录","icon":"tree","views":[{"id":"rpt-designer-explorer","tabLabel":"模板","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-explorer"}]},"content":{"caption":"设计画布","icon":"edit","views":[{"id":"rpt-designer-content","tabLabel":"设计","icon":"edit","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-content"}]},"property":{"caption":"公式向导/属性","icon":"detail-view","views":[{"id":"rpt-designer-prop","tabLabel":"配置","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-designer-prop"}]}},"type":"workspace-node","name":"rpt-designer"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-rpt-designer_dup', '/7484975236346023936/7484975236346023963', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023964', 'fi-gl-rpt-spreadjs-designer_dup', '报表设计器（SpreadJS）', 'tabler-outline/file-spreadsheet', NULL, 28, '{"caption":"报表设计器（SpreadJS）","workspace":{"id":"rpt_spreadjs_designer","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-model","id":"rpt-spreadjs-designer-model"},"explorer":{"caption":"报表模板","icon":"tree","views":[{"id":"rpt-spreadjs-designer-explorer","tabLabel":"模板","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-explorer"}]},"content":{"caption":"设计画布","icon":"edit","views":[{"id":"rpt-spreadjs-designer-content","tabLabel":"设计","icon":"edit","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-content"}]},"property":{"caption":"公式向导/样式","icon":"detail-view","views":[{"id":"rpt-spreadjs-designer-prop","tabLabel":"配置","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.rpt-spreadjs-designer-prop"}]}},"type":"workspace-node","name":"rpt-spreadjs-designer"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-rpt-spreadjs-designer_dup', '/7484975236346023936/7484975236346023964', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023965', 'fi-gl-rpt-design-workbench_dup', '报表设计工作台', 'table-chart', NULL, 29, '{"caption":"报表设计工作台","workspace":{"id":"rpt_design_workbench","model":{"type":"native_pages","native_page":"portal.rpt.design-workbench","id":"rpt-design-workbench-model","view":"content","props":{"mode":"model"}},"explorer":{"caption":"报表类别","icon":"folder","views":[{"id":"rpt-design-workbench-explorer","tabLabel":"类别","icon":"folder","type":"native_pages","native_page":"portal.rpt.design-workbench","view":"explorer"}]},"content":{"caption":"报表设计工作台","icon":"table-chart","views":[{"id":"rpt-design-workbench-content","tabLabel":"报表","icon":"table-chart","type":"native_pages","native_page":"portal.rpt.design-workbench","view":"content"}]},"property":{"caption":"报表属性","icon":"detail-view","views":[{"id":"rpt-design-workbench-prop","tabLabel":"属性","icon":"detail-view","type":"native_pages","native_page":"portal.rpt.design-workbench","view":"property"}]}},"type":"workspace-node","name":"rpt-design-workbench"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-rpt-design-workbench_dup', '/7484975236346023936/7484975236346023965', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023966', 'fi-gl-rpt-app-workbench_dup', '报表应用工作台', 'table-chart', NULL, 30, '{"caption":"报表应用工作台","workspace":{"id":"rpt_app_workbench","model":{"type":"native_pages","native_page":"portal.rpt.report-app-workbench","id":"rpt-app-workbench-model","view":"content","props":{"mode":"data"}},"explorer":{"caption":"组织与期间","icon":"tree","views":[{"id":"rpt-app-workbench-explorer","tabLabel":"组织/期间","icon":"tree","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"explorer"}]},"content":{"caption":"报表应用工作台","icon":"table-chart","views":[{"id":"rpt-app-workbench-content","tabLabel":"报表","icon":"table-chart","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"content"}]},"property":{"caption":"属性","icon":"detail-view","views":[{"id":"rpt-app-workbench-prop","tabLabel":"报表属性","icon":"detail-view","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"property"},{"id":"rpt-app-workbench-prop-org","tabLabel":"组织机构","icon":"tree","type":"native_pages","native_page":"portal.rpt.report-app-workbench","view":"propertyOrg"}]}},"type":"workspace-node","name":"rpt-app-workbench"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-rpt-app-workbench_dup', '/7484975236346023936/7484975236346023966', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023967', 'fi-gl-dict-ws_dup', '合并组织机构维护', 'org-chart', NULL, 31, '{"caption":"合并组织机构维护","workspace":{"id":"dict_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.dictws-model","id":"dictws-model"},"explorer":{"caption":"层级树","icon":"tree","views":[{"id":"dictws-explorer","tabLabel":"层级树","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.dictws-explorer"}]},"content":{"caption":"节点列表","icon":"table-view","views":[{"id":"dictws-content","tabLabel":"节点表格","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictws-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"dictws-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.dictws-prop-detail"}]}},"type":"workspace-node","name":"dict-ws"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-dict-ws_dup', '/7484975236346023936/7484975236346023967', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023968', 'fi-gl-acct-ws_dup', '会计核算管理', 'business-objects-experience', NULL, 32, '{"caption":"会计核算管理","workspace":{"id":"acct_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.acctws-model","id":"acctws-model"},"explorer":{"caption":"合并组织","icon":"org-chart","views":[{"id":"acctws-explorer","tabLabel":"合并组织","icon":"org-chart","type":"html_pages","html_page":"fi.cmxfico.gl.acctws-explorer"}]},"content":{"caption":"科目核算","icon":"business-objects-experience","views":[{"id":"acctws-content","tabLabel":"科目核算","icon":"business-objects-experience","type":"html_pages","html_page":"fi.cmxfico.gl.acctws-content"}]},"property":{"caption":"科目详情","icon":"detail-view","views":[{"id":"acctws-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.acctws-prop-detail"}]}},"type":"workspace-node","name":"acct-ws"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-acct-ws_dup', '/7484975236346023936/7484975236346023968', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023969', 'fi-gl-glacct-ws_dup', '总账科目维护', 'account', NULL, 33, '{"caption":"总账科目维护","workspace":{"id":"glacct_ws","model":{"type":"html_pages","html_page":"fi.cmxfico.gl.glacct-model","id":"glacct-model"},"explorer":{"caption":"科目树","icon":"tree","views":[{"id":"glacct-explorer","tabLabel":"科目树","icon":"tree","type":"html_pages","html_page":"fi.cmxfico.gl.glacct-explorer"}]},"content":{"caption":"子科目","icon":"table-view","views":[{"id":"glacct-content","tabLabel":"子科目表格","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.glacct-content"}]},"property":{"caption":"详情","icon":"detail-view","views":[{"id":"glacct-prop-detail","tabLabel":"详情","icon":"detail-view","type":"html_pages","html_page":"fi.cmxfico.gl.glacct-prop-detail"}]}},"type":"workspace-node","name":"glacct-ws"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023936', 'report', 2, 1, '/report/fi-gl-glacct-ws_dup', '/7484975236346023936/7484975236346023969', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023970', 'grp-ignite-demo_dup', 'Ignite 组件演示', 'tabler-outline/flame', NULL, 2, '{"caption":"Ignite 组件演示","expanded":true,"name":"ignite-demo"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 0, '/grp-ignite-demo_dup', '/7484975236346023970', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023971', 'fi-gl-ignite-combo-editor_dup', 'Ignite 组合框编辑器', 'tabler-outline/select', NULL, 1, '{"caption":"Ignite 组合框编辑器","workspace":{"content":{"caption":"Ignite 组合框编辑器","icon":"form","views":[{"tabLabel":"编辑器演示","icon":"form","type":"html_pages","html_page":"fi.cmxfico.gl.ignite-combo-editor"}]}},"type":"workspace-node","name":"ignite-combo-editor"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023970', 'grp-ignite-demo_dup', 2, 1, '/grp-ignite-demo_dup/fi-gl-ignite-combo-editor_dup', '/7484975236346023970/7484975236346023971', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023972', 'fi-gl-ignite-list_dup', 'Ignite 列表主从', 'tabler-outline/list', NULL, 2, '{"caption":"Ignite 列表主从","workspace":{"content":{"caption":"Ignite 列表主从","icon":"table-view","views":[{"tabLabel":"主从演示","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.ignite-list"}]}},"type":"workspace-node","name":"ignite-list"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023970', 'grp-ignite-demo_dup', 2, 1, '/grp-ignite-demo_dup/fi-gl-ignite-list_dup', '/7484975236346023970/7484975236346023972', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023973', 'grp-meta-def_dup', '定义中心', 'tabler-outline/adjustments', NULL, 3, '{"caption":"定义中心","expanded":true,"name":"meta-def"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 0, '/grp-meta-def_dup', '/7484975236346023973', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023974', 'flexible-combination-manager_dup', '弹性组合管理', 'tabler-outline/adjustments-cog', NULL, 1, '{"caption":"弹性组合管理","workspace":{"content":{"caption":"弹性组合管理","icon":"settings","views":[{"tabLabel":"设计","icon":"settings","type":"flexible-combination-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"flexible-combination-source","data":{}},{"tabLabel":"Schema","icon":"database","type":"flexible-combination-schema","data":{}}]},"explorer":{"caption":"档案列表","icon":"tree","views":[{"tabLabel":"档案","icon":"list","type":"flexible-combination-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"flexible-combination-inspector","data":{}},{"tabLabel":"校验/预览","icon":"validate","type":"flexible-combination-verify","data":{}}]}},"type":"workspace-node","name":"flexible-combination-manager"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/flexible-combination-manager_dup', '/7484975236346023973/7484975236346023974', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023975', 'dict-def-manager_dup', '数据字典定义', 'tabler-outline/book', NULL, 2, '{"caption":"数据字典定义","workspace":{"content":{"caption":"数据字典定义","icon":"book","views":[{"tabLabel":"设计","icon":"book","type":"dct-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"dct-def-source","data":{}}]},"explorer":{"caption":"字典列表","icon":"tree","views":[{"tabLabel":"字典","icon":"list","type":"dct-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"dct-def-inspector","data":{}}]}},"type":"workspace-node","name":"dict-def-manager"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/dict-def-manager_dup', '/7484975236346023973/7484975236346023975', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023976', 'doc-def-manager_dup', '业务单据定义', 'tabler-outline/file-invoice', NULL, 3, '{"caption":"业务单据定义","workspace":{"content":{"caption":"业务单据定义","icon":"document","views":[{"tabLabel":"设计","icon":"document","type":"doc-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"doc-def-source","data":{}}]},"explorer":{"caption":"单据列表","icon":"tree","views":[{"tabLabel":"单据","icon":"list","type":"doc-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"doc-def-inspector","data":{}}]}},"type":"workspace-node","name":"doc-def-manager"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/doc-def-manager_dup', '/7484975236346023973/7484975236346023976', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023977', 'base-dct-def-manager_dup', '字典基础元数据', 'tabler-outline/template', NULL, 4, '{"caption":"字典基础元数据","workspace":{"content":{"caption":"字典基础元数据","icon":"template","views":[{"tabLabel":"设计","icon":"template","type":"base-dct-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"base-dct-def-source","data":{}}]},"explorer":{"caption":"基础文件","icon":"tree","views":[{"tabLabel":"字段集","icon":"list","type":"base-dct-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"base-dct-def-inspector","data":{}}]}},"type":"workspace-node","name":"base-dct-def-manager"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/base-dct-def-manager_dup', '/7484975236346023973/7484975236346023977', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023978', 'base-dct-native-pages-manager_dup', '字典基础元数据(native_pages)', 'tabler-outline/template', NULL, 5, '{"caption":"字典基础元数据(native_pages)","workspace":{"content":{"caption":"字典基础元数据(native_pages)","icon":"template","views":[{"tabLabel":"设计","icon":"template","type":"native_pages","native_page":"definition.base-dct-native","view":"manager","props":{}},{"tabLabel":"源码","icon":"source-code","type":"native_pages","native_page":"definition.base-dct-native","view":"source","props":{}}]},"explorer":{"caption":"基础文件","icon":"tree","views":[{"tabLabel":"字段集","icon":"list","type":"native_pages","native_page":"definition.base-dct-native","view":"list","props":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"native_pages","native_page":"definition.base-dct-native","view":"inspector","props":{}}]}},"type":"workspace-node","name":"base-dct-native-pages-manager"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/base-dct-native-pages-manager_dup', '/7484975236346023973/7484975236346023978', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023979', 'native-pages-linkage-demo_dup', '原生页面联动演示(native_pages·CE+context)', 'tabler-outline/topology-star-3', NULL, 6, '{"caption":"原生页面联动演示(native_pages·CE+context)","workspace":{"explorer":{"caption":"产品列表","icon":"list","views":[{"tabLabel":"产品","icon":"product","type":"native_pages","native_page":"demo.product-explorer","view":"default","props":{}}]},"content":{"caption":"产品详情 / 整页演示","icon":"detail-view","views":[{"tabLabel":"详情(content CE)","icon":"detail-view","type":"native_pages","native_page":"demo.product-content","view":"default","props":{}},{"tabLabel":"整页HTML(iframe)","icon":"internet-browser","type":"native_pages","native_page":"demo.interactive","view":"default","props":{}}]}},"type":"workspace-node","name":"native-pages-linkage-demo"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/native-pages-linkage-demo_dup', '/7484975236346023973/7484975236346023979', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023980', 'base-doc-def-manager_dup', '单据基础元数据', 'tabler-outline/template', NULL, 7, '{"caption":"单据基础元数据","workspace":{"content":{"caption":"单据基础元数据","icon":"template","views":[{"tabLabel":"设计","icon":"template","type":"base-doc-def-manager","data":{}},{"tabLabel":"源码","icon":"source-code","type":"base-doc-def-source","data":{}}]},"explorer":{"caption":"基础文件","icon":"tree","views":[{"tabLabel":"字段集","icon":"list","type":"base-doc-def-list","data":{}}]},"property":{"caption":"属性","icon":"settings","views":[{"tabLabel":"检查器","icon":"detail-view","type":"base-doc-def-inspector","data":{}}]}},"type":"workspace-node","name":"base-doc-def-manager"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/base-doc-def-manager_dup', '/7484975236346023973/7484975236346023980', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023981', 'custom-page-designer_dup', '自定义页面设计', 'tabler-outline/layout-dashboard', NULL, 8, '{"caption":"自定义页面设计","workspace":{"content":{"caption":"自定义页面设计","icon":"business-objects-experience","views":[{"tabLabel":"页面","icon":"business-objects-experience","type":"custom-page-designer","data":{}}]}},"type":"workspace-node","name":"custom-page-designer"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/custom-page-designer_dup', '/7484975236346023973/7484975236346023981', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023982', 'fi-gl-account-def_dup', '科目弹性组合', 'tabler-outline/book', NULL, 9, '{"caption":"科目弹性组合","workspace":{"content":{"caption":"科目弹性组合","icon":"settings","views":[{"tabLabel":"定义","icon":"settings","type":"html_pages","html_page":"fi.cmxfico.gl.account-def"}]}},"type":"workspace-node","name":"account-def"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/fi-gl-account-def_dup', '/7484975236346023973/7484975236346023982', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023983', 'fi-gl-trade-def_dup', '交易弹性组合', 'tabler-outline/shopping-cart-cog', NULL, 10, '{"caption":"交易弹性组合","workspace":{"content":{"caption":"交易弹性组合","icon":"settings","views":[{"tabLabel":"定义","icon":"settings","type":"html_pages","html_page":"fi.cmxfico.gl.trade-def"}]}},"type":"workspace-node","name":"trade-def"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/fi-gl-trade-def_dup', '/7484975236346023973/7484975236346023983', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023984', 'fi-gl-ccm-attrs-test_dup', 'CCM全属性测试', 'tabler-outline/test-pipe', NULL, 11, '{"caption":"CCM全属性测试","workspace":{"content":{"caption":"CmxColumnModel 全属性 + select/date 编辑器 + 表单校验测试","icon":"table-view","views":[{"tabLabel":"测试","icon":"table-view","type":"html_pages","html_page":"fi.cmxfico.gl.ccm-attrs-test"}]}},"type":"workspace-node","name":"ccm-attrs-test"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/fi-gl-ccm-attrs-test_dup', '/7484975236346023973/7484975236346023984', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023985', 'workspace-node-editor_dup', '工作区节点编辑', 'tabler-outline/sitemap', NULL, 12, '{"caption":"工作区节点编辑","type":"workspace-node-editor","name":"workspace-node-editor"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023973', 'grp-meta-def_dup', 2, 1, '/grp-meta-def_dup/workspace-node-editor_dup', '/7484975236346023973/7484975236346023985', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023986', 'grp-pages_dup', '页面管理', 'documents', NULL, 4, '{"caption":"页面管理","expanded":true,"name":"page-mgmt"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 0, '/grp-pages_dup', '/7484975236346023986', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023987', 'page-home_dup', '首页', NULL, NULL, 1, '{"caption":"首页","name":"home"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023986', 'grp-pages_dup', 2, 1, '/grp-pages_dup/page-home_dup', '/7484975236346023986/7484975236346023987', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023988', 'page-users_dup', '用户管理', NULL, 'MENU.PAGES.USERS', 2, '{"caption":"用户管理","name":"users"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023986', 'grp-pages_dup', 2, 1, '/grp-pages_dup/page-users_dup', '/7484975236346023986/7484975236346023988', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023989', 'page-acl_dup', '权限配置', NULL, 'MENU.PAGES.ACL', 3, '{"caption":"权限配置","name":"acl"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023986', 'grp-pages_dup', 2, 1, '/grp-pages_dup/page-acl_dup', '/7484975236346023986/7484975236346023989', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023990', 'page-reports_dup', '报表中心', NULL, NULL, 4, '{"caption":"报表中心","name":"reports"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023986', 'grp-pages_dup', 2, 1, '/grp-pages_dup/page-reports_dup', '/7484975236346023986/7484975236346023990', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023991', 'portal-dam-registry_dup', 'DAM 注册管理中心', 'tree', NULL, 5, '{"caption":"DAM 注册管理中心","workspace":{"explorer":{"caption":"DAM 导航","icon":"tree","views":[{"tabLabel":"DAM","icon":"tree","type":"native_pages","native_page":"portal.dam.registry-center","view":"explorer","props":{}}]},"content":{"caption":"DAM 注册管理中心","icon":"tree","views":[{"tabLabel":"注册中心","icon":"tree","type":"native_pages","native_page":"portal.dam.registry-center","view":"manager","props":{}}]},"property":{"caption":"资源","icon":"documents","views":[{"tabLabel":"资源","icon":"documents","type":"native_pages","native_page":"portal.dam.registry-center","view":"property","props":{}}]}},"type":"workspace-node","name":"dam-registry"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 1, '/portal-dam-registry_dup', '/7484975236346023991', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023992', 'grp-components_dup', '组件库', 'puzzle', NULL, 6, '{"caption":"组件库","expanded":false,"name":"component-lib"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 0, '/grp-components_dup', '/7484975236346023992', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023993', 'comp-forms_dup', '表单组件', NULL, NULL, 1, '{"caption":"表单组件","name":"form-widgets"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023992', 'grp-components_dup', 2, 1, '/grp-components_dup/comp-forms_dup', '/7484975236346023992/7484975236346023993', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023994', 'comp-charts_dup', '图表组件', NULL, NULL, 2, '{"caption":"图表组件","name":"charts"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023992', 'grp-components_dup', 2, 1, '/grp-components_dup/comp-charts_dup', '/7484975236346023992/7484975236346023994', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023995', 'grp-services_dup', '服务接入', 'world', NULL, 7, '{"caption":"服务接入","expanded":false,"name":"service-access"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 0, '/grp-services_dup', '/7484975236346023995', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023996', 'svc-rest_dup', 'REST 接口', NULL, NULL, 1, '{"caption":"REST 接口","name":"rest"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023995', 'grp-services_dup', 2, 1, '/grp-services_dup/svc-rest_dup', '/7484975236346023995/7484975236346023996', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023997', 'svc-graphql_dup', 'GraphQL', NULL, NULL, 2, '{"caption":"GraphQL","name":"graphql"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023995', 'grp-services_dup', 2, 1, '/grp-services_dup/svc-graphql_dup', '/7484975236346023995/7484975236346023997', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023998', 'svc-ws_dup', 'WebSocket', NULL, NULL, 3, '{"caption":"WebSocket","name":"websocket"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023995', 'grp-services_dup', 2, 1, '/grp-services_dup/svc-ws_dup', '/7484975236346023995/7484975236346023998', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346023999', 'grp-data_dup', '数据资产', 'database', NULL, 8, '{"caption":"数据资产","expanded":false,"name":"data-assets"}'::jsonb, 'fi', 'cmxfico', 'report', NULL, NULL, 1, 0, '/grp-data_dup', '/7484975236346023999', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346024000', 'data-sources_dup', '数据源', NULL, NULL, 1, '{"caption":"数据源","name":"sources"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023999', 'grp-data_dup', 2, 1, '/grp-data_dup/data-sources_dup', '/7484975236346023999/7484975236346024000', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484975236346024001', 'data-models_dup', '数据模型', NULL, NULL, 2, '{"caption":"数据模型","name":"models"}'::jsonb, 'fi', 'cmxfico', 'report', '7484975236346023999', 'grp-data_dup', 2, 1, '/grp-data_dup/data-models_dup', '/7484975236346023999/7484975236346024001', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;


-- ---- 追加菜单（init_menu.sql 快照未含的 5 条） ----
-- 待办中心（migrations/20260801_002_flow_todo_center_menu.up.sql）
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7485938639050227713', 'fi-gl-flow-todo-center', '待办中心', 'task', NULL, 32,
  '{"caption": "待办中心", "workspace": {"id": "flow_todo_center", "explorer": {"caption": "待办分类", "icon": "tree", "views": [{"id": "flow-todo-center-explorer", "tabLabel": "分类", "icon": "tree", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "explorer"}]}, "content": {"caption": "待办中心", "icon": "task", "views": [{"id": "flow-todo-center-content", "tabLabel": "待办", "icon": "task", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "content"}]}, "property": {"caption": "流程轨迹", "icon": "detail-view", "views": [{"id": "flow-todo-center-prop", "tabLabel": "轨迹", "icon": "detail-view", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "property"}]}}, "type": "workspace-node", "name": "flow-todo-center"}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-flow-todo-center', p.id_path || '/7485938639050227713', 1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

-- 决策集三工作台（migrations/20260815_001_rules_menu.up.sql）
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7490000000000000101', 'fi-gl-rules-design-workbench', '决策集设计工作台', 'table-view', NULL, 33,
  '{"caption":"决策集设计工作台","type":"workspace-node","name":"rules-design-workbench","workspace":{"id":"rules_design_workbench","explorer":{"caption":"决策集","icon":"tree","views":[{"id":"rules-design-explorer","tabLabel":"决策集","icon":"tree","type":"native_pages","native_page":"portal.rules.design-workbench","view":"explorer"}]},"content":{"caption":"决策表","icon":"table-view","views":[{"id":"rules-design-content","tabLabel":"决策表","icon":"table-view","type":"native_pages","native_page":"portal.rules.design-workbench","view":"content"}]},"property":{"caption":"完整性","icon":"detail-view","views":[{"id":"rules-design-prop","tabLabel":"完整性","icon":"detail-view","type":"native_pages","native_page":"portal.rules.design-workbench","view":"property"}]}}}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-rules-design-workbench', p.id_path || '/7490000000000000101',
  1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7490000000000000102', 'fi-gl-rules-sim-workbench', '决策应用工作台', 'play', NULL, 34,
  '{"caption":"决策应用工作台","type":"workspace-node","name":"rules-sim-workbench","workspace":{"id":"rules_sim_workbench","explorer":{"caption":"决策集","icon":"tree","views":[{"id":"rules-sim-explorer","tabLabel":"决策集","icon":"tree","type":"native_pages","native_page":"portal.rules.sim-workbench","view":"explorer"}]},"content":{"caption":"求值","icon":"play","views":[{"id":"rules-sim-content","tabLabel":"求值","icon":"play","type":"native_pages","native_page":"portal.rules.sim-workbench","view":"content"}]},"property":{"caption":"决策轨迹","icon":"detail-view","views":[{"id":"rules-sim-prop","tabLabel":"trace","icon":"detail-view","type":"native_pages","native_page":"portal.rules.sim-workbench","view":"property"}]}}}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-rules-sim-workbench', p.id_path || '/7490000000000000102',
  1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7490000000000000103', 'fi-gl-rules-logs', '决策审计中心', 'history', NULL, 35,
  '{"caption":"决策审计中心","type":"workspace-node","name":"rules-logs","workspace":{"id":"rules_logs","explorer":{"caption":"决策集","icon":"tree","views":[{"id":"rules-logs-explorer","tabLabel":"决策集","icon":"tree","type":"native_pages","native_page":"portal.rules.logs","view":"explorer"}]},"content":{"caption":"决策日志","icon":"history","views":[{"id":"rules-logs-content","tabLabel":"日志","icon":"history","type":"native_pages","native_page":"portal.rules.logs","view":"content"}]},"property":{"caption":"归因","icon":"detail-view","views":[{"id":"rules-logs-prop","tabLabel":"trace","icon":"detail-view","type":"native_pages","native_page":"portal.rules.logs","view":"property"}]}}}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-rules-logs', p.id_path || '/7490000000000000103',
  1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

-- 任务中心（migrations/20260815_002_job_center_menu.up.sql）
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7485938639050227714', 'fi-gl-job-center', '任务中心', 'process', NULL, 36,
  $def${"name":"job-center","caption":"任务中心","type":"workspace-node","icon":"process","workspace":{"id":"job_center","explorer":{"caption":"概览 / 新建 / 过滤","icon":"detail-view","views":[{"id":"job-center-explorer","tabLabel":"任务台","icon":"detail-view","type":"native_pages","native_page":"portal.job.monitor","view":"explorer"}]},"content":{"caption":"任务列表与监控","icon":"process","views":[{"id":"job-center-active","tabLabel":"活跃作业","icon":"process","type":"native_pages","native_page":"portal.job.monitor","view":"active"},{"id":"job-center-history","tabLabel":"历史作业","icon":"history","type":"native_pages","native_page":"portal.job.monitor","view":"history"}]},"property":{"caption":"作业属性","icon":"detail-view","views":[{"id":"job-center-prop","tabLabel":"属性","icon":"detail-view","type":"native_pages","native_page":"portal.job.monitor","view":"property"}]}}}$def$::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-job-center', p.id_path || '/7485938639050227714', 1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;


-- ============================================================
-- 4. 编码规则（cmx_code_rule）
-- 来源：迁移 20260818_001 段1（MDM 多域 14 条）
--       + 迁移 20260813_002（MDM_BILL 单据号保底，排段尾防与 14 条顺排 id 混淆）
-- 幂等：ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING
-- ============================================================

-- 1. 编码规则 cmx_code_rule（id 9000000000000002~0015 顺排，MDM_BILL=…0001 已占）
--    字典 code 铸号：激活器读 dictMeta.codeRule.ruleCode。漏配不报错，code 退化为占位码——故必须 seed。
-- ─────────────────────────────────────────────


INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000002, 'MDM_KH', '客户主数据编码（CUS+日期+流水）', 'auto',
        '[{"type":"const","value":"CUS"},{"type":"dateSerial","format":"YYYYMMDD","width":4,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

-- 物料主数据编码：MAT + YYYYMMDD + 4位日流水 → MAT202608180001
INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000003, 'MDM_WL', '物料主数据编码（MAT+日期+流水）', 'auto',
        '[{"type":"const","value":"MAT"},{"type":"dateSerial","format":"YYYYMMDD","width":4,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

-- 会计科目编码：ref 段取行字段 acct_no（用户在 CR 填的科目号，如 1001 / 100101）→ code = 科目号。
-- 说明：激活器 create 分支会先用占位码覆盖 header_row.code，仅当 dictMeta.codeRule 铸号成功才能再覆盖，
--       故科目号走 ref 段「借铸号通道」写入 code 列——code 与 acct_no 恒等，无需改 Rust 代码。
--       acct_no 为空时铸出空串（NOT NULL 允许），科目号在 CR 表单为必填，正常不会发生。
INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000004, 'MDM_KJ', '会计科目编码（取科目号 acct_no 原值）', 'auto',
        '[{"type":"ref","field":"acct_no"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;




INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000005, 'MDM_BZ', '币种编码（取 ISO 币种码）', 'auto',
        '[{"type":"ref","field":"currency_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000006, 'MDM_JLDW', '计量单位编码（取单位编码）', 'auto',
        '[{"type":"ref","field":"uom_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000007, 'MDM_WLDL', '物料分类编码（取分类编码）', 'auto',
        '[{"type":"ref","field":"class_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000008, 'MDM_CBZX', '成本中心编码（取中心编码）', 'auto',
        '[{"type":"ref","field":"cost_center_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000009, 'MDM_LRZX', '利润中心编码（取中心编码）', 'auto',
        '[{"type":"ref","field":"profit_center_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000010, 'MDM_GS', '公司编码（取公司编码）', 'auto',
        '[{"type":"ref","field":"company_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000011, 'MDM_ZZ', '组织编码（取组织编码）', 'auto',
        '[{"type":"ref","field":"org_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000012, 'MDM_BM', '部门编码（取部门编码）', 'auto',
        '[{"type":"ref","field":"dept_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000013, 'MDM_GW', '岗位编码（取岗位编码）', 'auto',
        '[{"type":"ref","field":"position_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000014, 'MDM_YG', '员工编码（取工号）', 'auto',
        '[{"type":"ref","field":"emp_no"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;




INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000015, 'MDM_GYS', '供应商主数据编码（SUP+日期+流水）', 'auto',
        '[{"type":"const","value":"SUP"},{"type":"dateSerial","format":"YYYYMMDD","width":4,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

-- ─────────────────────────────────────────────


-- MDM 变更申请单据号保底规则（20260813_002）
INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000001, 'MDM_BILL', 'MDM 变更申请单据号（CR+日期+流水）', 'auto',
        '[{"type":"const","value":"CR"},{"type":"dateSerial","format":"YYYYMMDD","width":6,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;
