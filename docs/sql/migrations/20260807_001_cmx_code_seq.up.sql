-- =====================================================
-- cmx_code_seq 编码发号序列表
-- 作为 serial/dateSerial 段的集群安全发号源。
-- 一个 (rule_code, prefix) 一行，存当前已发到的最大流水值。
-- 发号时 SELECT ... FOR UPDATE SKIP LOCKED 行级锁取号段，集群安全（AGENTS.md §五红线）。
-- 由 cmx_code_rule.use_sequence=true 开启；默认 false 走"反查业务表 max"老路径。
-- =====================================================

CREATE TABLE IF NOT EXISTS cmx_code_seq (
    id              BIGINT                  NOT NULL,
    rule_code       VARCHAR(64)             NOT NULL,
    prefix          VARCHAR(128)            NOT NULL,
    current_val     BIGINT                  NOT NULL DEFAULT 0,
    width           INT4                    NOT NULL DEFAULT 4,
    update_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id)
);

-- 发号分组键唯一：一个 (规则, 前缀) 只有一行当前值
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_code_seq_prefix ON cmx_code_seq (rule_code, prefix);

COMMENT ON TABLE cmx_code_seq IS '编码发号序列表（集群安全发号源，use_sequence=true 才启用）';
COMMENT ON COLUMN cmx_code_seq.id IS '主键ID（pk52）';
COMMENT ON COLUMN cmx_code_seq.rule_code IS '关联 cmx_code_rule.rule_code';
COMMENT ON COLUMN cmx_code_seq.prefix IS '发号分组键（含 reset_key，如 FV20260804）';
COMMENT ON COLUMN cmx_code_seq.current_val IS '已发到的最大流水值（0=首启未探测）';
COMMENT ON COLUMN cmx_code_seq.width IS '流水宽度（补零用，记录首次发号时的宽度）';
