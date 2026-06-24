-- =====================================================
-- 通用审计日志表 (cmx_audit_log)
-- 记录 Auth/Iam/Plugin/Biz 四个域的通用审计事件
-- =====================================================
CREATE TABLE IF NOT EXISTS cmx_audit_log (
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
    CONSTRAINT pk_cmx_audit_log PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_app_id    ON cmx_audit_log (app_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_domain    ON cmx_audit_log (domain);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_actor     ON cmx_audit_log (actor_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_target    ON cmx_audit_log (target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_request   ON cmx_audit_log (request_id);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_started   ON cmx_audit_log (started_at);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_archived  ON cmx_audit_log (archived);
CREATE INDEX IF NOT EXISTS idx_cmx_audit_log_result    ON cmx_audit_log (result);
-- 注：details 字段类型为 TEXT（JSON 字符串），不支持 GIN 索引；
-- 若需按 details 内部字段检索，可在应用层反序列化后内存过滤，或扩展为 jsonb 独立方案。

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
