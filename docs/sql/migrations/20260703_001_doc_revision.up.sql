-- 业务单据版本化台账（方案 §6A · 落地 Phase 8）
-- 单据每次保存产生一个不可变历史版本（append-only），支持 可记录/可查询/可追溯/可审计。
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。通用表（所有开启版本化的单据共用，按 doc_file+root_id 区分）。

-- =============================================
-- 1. cmx_doc_revision —— 整单 JSONB 快照版本表（方案 §6A.2 方案 A，默认）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_doc_revision
(
    id              BIGINT       NOT NULL,          -- 版本记录主键（雪花）
    doc_file        VARCHAR(200) NOT NULL,          -- 单据定义（哪种单据）
    root_table      VARCHAR(100) NOT NULL,          -- 根层表名（如 cv_batch）
    root_id         VARCHAR(64)  NOT NULL,          -- 单据根行 id（哪一张单，统一字符串化）
    rev_no          INT4         NOT NULL,          -- 版本号：该单第几版（1,2,3...）
    is_current      INT4         NOT NULL DEFAULT 1,-- 是否当前版（同 root 仅一行为 1）
    op              VARCHAR(16),                     -- create / update / delete / restore
    snapshot        JSONB        NOT NULL,          -- 整单列式包快照（§5.2 结构）
    change_summary  JSONB,                           -- 可选：本版变更摘要
    reason          VARCHAR(500),                    -- 变更原因（reason_required 时必填）
    actor_id        VARCHAR(64),                     -- 操作者 id
    actor_name      VARCHAR(100),                    -- 操作者名
    biz_status      VARCHAR(32),                     -- 冗余当时单据状态，便于按态检索
    created_at      TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_doc_revision            IS '业务单据版本化：整单 JSONB 快照（append-only，方案 §6A）';
COMMENT ON COLUMN cmx_doc_revision.root_id    IS '单据根行 id（字符串化）';
COMMENT ON COLUMN cmx_doc_revision.rev_no     IS '该单第几版';
COMMENT ON COLUMN cmx_doc_revision.is_current IS '是否当前版（同 root 仅一行为 1）';
COMMENT ON COLUMN cmx_doc_revision.snapshot   IS '整单列式包快照（前端 fromJSON 可直接还原）';
COMMENT ON COLUMN cmx_doc_revision.op         IS '操作: create/update/delete/restore';

CREATE UNIQUE INDEX IF NOT EXISTS uk_doc_rev
    ON cmx_doc_revision (doc_file, root_id, rev_no);
CREATE INDEX IF NOT EXISTS idx_doc_rev_cur
    ON cmx_doc_revision (doc_file, root_id, is_current);
CREATE INDEX IF NOT EXISTS idx_doc_rev_time
    ON cmx_doc_revision (root_id, created_at);

-- =============================================
-- 2. cmx_doc_change —— 字段级变更明细（方案 §6A.3，diff:true 时写；审计增强）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_doc_change
(
    id          BIGINT       NOT NULL,
    rev_id      BIGINT       NOT NULL,               -- → cmx_doc_revision.id（属于哪一版）
    root_id     VARCHAR(64)  NOT NULL,
    layer       VARCHAR(100),                          -- 层表名
    row_id      VARCHAR(64),                           -- 变更的行
    op          VARCHAR(8),                            -- I/U/D
    field       VARCHAR(100),                          -- 变更字段（U 时逐字段一行）
    old_value   JSONB,
    new_value   JSONB,
    actor_id    VARCHAR(64),
    created_at  TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_doc_change      IS '业务单据字段级变更明细（审计，方案 §6A.3）';
COMMENT ON COLUMN cmx_doc_change.rev_id IS '所属版本 cmx_doc_revision.id';

CREATE INDEX IF NOT EXISTS idx_doc_change_rev ON cmx_doc_change (rev_id);
CREATE INDEX IF NOT EXISTS idx_doc_change_row ON cmx_doc_change (root_id, row_id, field);
