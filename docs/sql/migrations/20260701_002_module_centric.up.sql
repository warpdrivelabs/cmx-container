-- =============================================
-- 模块为中心的插件解耦批次(20260701)
-- 包含：cmx_form / cmx_menu / cmx_module_current_version / cmx_module_version_history
-- 说明：cmx_module 字典表保持纯净，版本运行态由独立表承载
-- =============================================

-- =============================================
-- 1. 表单定义表 (cmx_form)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_form (
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
    CONSTRAINT pk_cmx_form PRIMARY KEY (id),
    CONSTRAINT uk_cmx_form_code UNIQUE (code)
);

CREATE INDEX IF NOT EXISTS idx_cmx_form_module ON cmx_form (domain_code, application_code, module_code);

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

-- =============================================
-- 2. 菜单定义表 (cmx_menu，树形结构使用标准分级字段)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_menu (
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
    -- 标准分级字段
    leaf             INT4       DEFAULT 1,
    depth            INT4       DEFAULT 1,
    parent_id        VARCHAR(64),
    parent_code      VARCHAR(128),
    id_path          VARCHAR(1000),
    code_path        VARCHAR(1000),
    -- 标准审计字段
    archived         INT4       DEFAULT 0,
    create_time      TIMESTAMP  DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP  DEFAULT CURRENT_TIMESTAMP,
    create_by        VARCHAR(100),
    create_name      VARCHAR(100),
    update_by        VARCHAR(100),
    update_name      VARCHAR(100),
    -- 标准扩展信息字段
    ext_attributes   TEXT,
    PRIMARY KEY (id)
);
CREATE UNIQUE INDEX uk_cmx_menu_code ON cmx_menu (code);

CREATE INDEX IF NOT EXISTS idx_cmx_menu_module ON cmx_menu (domain_code, application_code, module_code);
CREATE INDEX IF NOT EXISTS idx_cmx_menu_parent_id ON cmx_menu (parent_id);
-- 级联操作(移动/删除/树查询)按 code_path/id_path 前缀匹配,需索引支撑
CREATE INDEX IF NOT EXISTS idx_cmx_menu_code_path ON cmx_menu (code_path);
CREATE INDEX IF NOT EXISTS idx_cmx_menu_id_path ON cmx_menu (id_path);

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

-- =============================================
-- 3. 模块当前版本表 (cmx_module_current_version，每模块一行)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_module_current_version (
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
    CONSTRAINT pk_cmx_module_current_version PRIMARY KEY (id),
    CONSTRAINT uk_cmx_module_current_version_module UNIQUE (module_code)
);

CREATE INDEX IF NOT EXISTS idx_cmx_module_current_version_dom_app_mod
    ON cmx_module_current_version (domain_code, application_code, module_code);

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

-- =============================================
-- 4. 模块版本历史表 (cmx_module_version_history，对齐 cmx_plugin_versions)
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_module_version_history (
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
    CONSTRAINT pk_cmx_module_version_history PRIMARY KEY (id),
    CONSTRAINT uk_cmx_module_version_history_pkg UNIQUE (module_code, package_version)
);

CREATE INDEX IF NOT EXISTS idx_cmx_module_version_history_module ON cmx_module_version_history (module_id);
CREATE INDEX IF NOT EXISTS idx_cmx_module_version_history_pkg ON cmx_module_version_history (module_code, package_version);

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
