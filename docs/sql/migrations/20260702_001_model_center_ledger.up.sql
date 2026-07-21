-- 模型中心 · 数据库初始化与模块部署台账（目标库内系统表 + 主控库汇总表）
-- 幂等：全部 CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS。
-- 说明：目标业务库的这几张表由后端 PgTableDefineExecutor 编程建立（支持任意 db_id）；
--       本迁移文件用于「默认/主控库」在启动时也拥有同名结构 + 主控库专属的 cmx_model_registry。

-- =============================================
-- 1. 台账自描述（每库单例）：判断是否已初始化 / 系统表是否需升级
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
COMMENT ON TABLE  cmx_model_meta              IS '模型中心台账自描述（每库单例）';
COMMENT ON COLUMN cmx_model_meta.meta_version IS '台账 schema 版本，用于判定是否需要升级系统表';
COMMENT ON COLUMN cmx_model_meta.status       IS '台账状态: ready / upgrading / failed';
CREATE UNIQUE INDEX IF NOT EXISTS uk_model_meta_db_app ON cmx_model_meta (db_id, app_id);

-- =============================================
-- 2. 每模块当前态（工作台「已部署」来源；字典/单据/初始数据各自版本+状态）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_model_module
(
    id                   VARCHAR(64) NOT NULL,
    db_id                VARCHAR(100),
    app_id               VARCHAR(64) NOT NULL,
    domain_code          VARCHAR(100),
    application_code     VARCHAR(100),
    module_code          VARCHAR(100),
    module_name          VARCHAR(200),
    overall_status       VARCHAR(20) DEFAULT 'active',
    table_count          INT4        DEFAULT 0,
    def_source           VARCHAR(300),
    def_checksum         VARCHAR(64),
    first_deployed_at    TIMESTAMP,
    current_deployed_at  TIMESTAMP,
    deployed_by          VARCHAR(100),
    deployed_name        VARCHAR(100),
    archived             INT4        DEFAULT 0,
    create_time          TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    update_time          TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_model_module            IS '模型中心-模块部署当前态主表（每模块一行；类型状态见 cmx_model_module_kind）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_key
    ON cmx_model_module (db_id, app_id, domain_code, application_code, module_code);

CREATE TABLE IF NOT EXISTS cmx_model_module_kind
(
    id                   VARCHAR(64) NOT NULL,
    db_id                VARCHAR(100),
    app_id               VARCHAR(64) NOT NULL,
    domain_code          VARCHAR(100),
    application_code     VARCHAR(100),
    module_code          VARCHAR(100),
    kind                 VARCHAR(20) NOT NULL,
    version              VARCHAR(50),
    status               VARCHAR(20) DEFAULT 'none',
    table_count          INT4        DEFAULT 0,
    def_source           VARCHAR(300),
    def_checksum         VARCHAR(64),
    deployed_at          TIMESTAMP,
    deployed_by          VARCHAR(100),
    deployed_name        VARCHAR(100),
    error_message        TEXT,
    archived             INT4        DEFAULT 0,
    create_time          TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    update_time          TIMESTAMP   DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_model_module_kind        IS '模型中心-模块类型当前态（每模块每 kind 一行；新增类型不改表结构）';
COMMENT ON COLUMN cmx_model_module_kind.kind   IS '模块类型: DCT/DOC/RPT/SEED/...';
COMMENT ON COLUMN cmx_model_module_kind.status IS '类型状态: none/current/failed/upgrading';
CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_kind_key
    ON cmx_model_module_kind (db_id, app_id, domain_code, application_code, module_code, kind);
CREATE INDEX IF NOT EXISTS idx_model_module_kind_module
    ON cmx_model_module_kind (db_id, domain_code, application_code, module_code);

-- =============================================
-- 3. 追加式部署/升级历史（时间线，永不改写）
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
COMMENT ON TABLE  cmx_model_deploy_history        IS '模型中心-部署/升级历史（追加式，永不改写）';
COMMENT ON COLUMN cmx_model_deploy_history.kind   IS '操作类别: INIT/META_UPGRADE/DCT/DOC/SEED';
COMMENT ON COLUMN cmx_model_deploy_history.status IS '状态机: pending→executing→success/failed/skipped';
CREATE INDEX IF NOT EXISTS idx_model_history_module ON cmx_model_deploy_history (db_id, domain_code, application_code, module_code);
CREATE INDEX IF NOT EXISTS idx_model_history_batch  ON cmx_model_deploy_history (batch_id);
CREATE INDEX IF NOT EXISTS idx_model_history_time   ON cmx_model_deploy_history (create_time);

-- =============================================
-- 4. 源定义/初始数据 JSON 完整留档（每模块×kind×版本一行）
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
COMMENT ON TABLE  cmx_model_source             IS '模型中心-源定义/初始数据 JSON 完整留档';
COMMENT ON COLUMN cmx_model_source.source_json IS '源定义或初始数据 JSON 原文（完整保存，可复现/审计）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_model_source_ver
    ON cmx_model_source (db_id, app_id, domain_code, application_code, module_code, kind, version);
CREATE INDEX IF NOT EXISTS idx_model_source_current
    ON cmx_model_source (db_id, domain_code, application_code, module_code, kind, is_current);

-- =============================================
-- 5. 主控库跨库总览（区分不同数据库；仅主控/默认库需要）
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
COMMENT ON TABLE  cmx_model_registry                 IS '主控库-各目标数据库模型部署总览（区分不同数据库）';
COMMENT ON COLUMN cmx_model_registry.modules_summary IS '各模块×kind 版本与状态摘要（供总览页免逐库查）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_model_registry_db ON cmx_model_registry (db_id, app_id);
