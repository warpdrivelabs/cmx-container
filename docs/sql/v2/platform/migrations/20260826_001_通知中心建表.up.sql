-- =============================================
-- 迁移说明：通知中心建表（主体 cmx_notification + 收件明细 cmx_notification_recipient，写扩散模型）
-- 影响表：cmx_notification, cmx_notification_recipient
-- 操作类型：CREATE TABLE / CREATE INDEX
-- 回滚方式：无（新表；如需清理见 DROP 归档说明）
-- =============================================

CREATE TABLE IF NOT EXISTS cmx_notification
(
    id              BIGINT        NOT NULL,
    center          VARCHAR(16)   NOT NULL,
    type            VARCHAR(64)   NOT NULL DEFAULT 'system',
    level           VARCHAR(16)   NOT NULL DEFAULT 'info',
    title           VARCHAR(500)  NOT NULL,
    body            TEXT          NOT NULL DEFAULT '',
    link            VARCHAR(1000) NOT NULL DEFAULT '',
    ext             JSONB         NOT NULL DEFAULT '{}'::jsonb,
    agg_key         VARCHAR(128)  NOT NULL DEFAULT '',
    sender_id       VARCHAR(64)   NOT NULL DEFAULT '',
    sender_name     VARCHAR(100)  NOT NULL DEFAULT '',
    source          VARCHAR(64)   NOT NULL DEFAULT '',
    target_type     VARCHAR(16)   NOT NULL DEFAULT 'user',
    target_refs     JSONB         NOT NULL DEFAULT '[]'::jsonb,
    recipient_count INT4          NOT NULL DEFAULT 0,
    status          VARCHAR(16)   NOT NULL DEFAULT 'done',
    created_at      BIGINT        NOT NULL,
    expire_at       BIGINT        NOT NULL DEFAULT 0,
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_notification IS '通知主体表（一条通知一行；群发收件明细在 cmx_notification_recipient）';
COMMENT ON COLUMN cmx_notification.id IS '主键（雪花号）';
COMMENT ON COLUMN cmx_notification.center IS '所属中心：task-任务中心，message-消息中心，log-日志中心';
COMMENT ON COLUMN cmx_notification.type IS '业务类型：system/mdm.dead_letter/flow.approval/job.finished 等，自由字符串';
COMMENT ON COLUMN cmx_notification.level IS '业务等级：info-success-warning-error';
COMMENT ON COLUMN cmx_notification.title IS '通知标题';
COMMENT ON COLUMN cmx_notification.body IS '通知正文';
COMMENT ON COLUMN cmx_notification.link IS '点击跳转目标：node:<工作区节点id> / menu:<菜单key> / https URL';
COMMENT ON COLUMN cmx_notification.ext IS '扩展负载 JSON（业务单据 id 等；聚合命中时含 count 计数）';
COMMENT ON COLUMN cmx_notification.agg_key IS '聚合键（如 subscription_id）：同键同收件人时间窗内合并计数，防通知风暴';
COMMENT ON COLUMN cmx_notification.sender_id IS '发送者用户 id（服务代发为空）';
COMMENT ON COLUMN cmx_notification.sender_name IS '发送者显示名（服务名/用户名）';
COMMENT ON COLUMN cmx_notification.source IS '来源服务标识：portal/mdm/flow 等';
COMMENT ON COLUMN cmx_notification.target_type IS '目标类型：user-指定人，org-部门，role-角色，all-全员（审计冗余）';
COMMENT ON COLUMN cmx_notification.target_refs IS '原始目标引用 JSON 数组（审计）';
COMMENT ON COLUMN cmx_notification.recipient_count IS '收件人数（异步展开完成时回写实插行数）';
COMMENT ON COLUMN cmx_notification.status IS '状态：pending-异步展开中，done-完成';
COMMENT ON COLUMN cmx_notification.created_at IS '创建时间（epoch 毫秒）';
COMMENT ON COLUMN cmx_notification.expire_at IS '过期时间（epoch 毫秒，0=按默认保留期）；过期由清理任务删除';

CREATE INDEX IF NOT EXISTS ix_cmx_notification_created ON cmx_notification (created_at DESC);
CREATE INDEX IF NOT EXISTS ix_cmx_notification_expire ON cmx_notification (expire_at) WHERE expire_at > 0;
CREATE INDEX IF NOT EXISTS ix_cmx_notification_pending ON cmx_notification (created_at) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS ix_cmx_notification_agg ON cmx_notification (agg_key, created_at DESC) WHERE agg_key <> '';

CREATE TABLE IF NOT EXISTS cmx_notification_recipient
(
    id              BIGINT      NOT NULL,
    notification_id BIGINT      NOT NULL,
    user_id         VARCHAR(64) NOT NULL,
    center          VARCHAR(16) NOT NULL,
    is_read         BOOLEAN     NOT NULL DEFAULT FALSE,
    read_at         BIGINT      NOT NULL DEFAULT 0,
    created_at      BIGINT      NOT NULL,
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_notification_recipient IS '通知收件明细表（写扩散：每收件人一行；已读态/未读统计落此表）';
COMMENT ON COLUMN cmx_notification_recipient.id IS '主键（雪花号）';
COMMENT ON COLUMN cmx_notification_recipient.notification_id IS '通知主体 id（cmx_notification.id）';
COMMENT ON COLUMN cmx_notification_recipient.user_id IS '收件用户 id（cmx_user.id，雪花 id 字符串）';
COMMENT ON COLUMN cmx_notification_recipient.center IS '冗余主体 center（未读统计免 join）';
COMMENT ON COLUMN cmx_notification_recipient.is_read IS '是否已读：false-未读，true-已读';
COMMENT ON COLUMN cmx_notification_recipient.read_at IS '已读时间（epoch 毫秒，0=未读）';
COMMENT ON COLUMN cmx_notification_recipient.created_at IS '收件时间（epoch 毫秒，= 主体 created_at）';

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_notif_recv ON cmx_notification_recipient (notification_id, user_id);
CREATE INDEX IF NOT EXISTS ix_cmx_notif_recv_user ON cmx_notification_recipient (user_id, center, is_read, created_at DESC);
