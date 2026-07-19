-- cmx-flow 流程定义补 DAM 三段列（domain / application / module）
-- 幂等：ADD COLUMN IF NOT EXISTS。
-- 依赖：20260718_007_cmx_flow_definition（cmx_flow_definition 表）。
-- 涉及变更：流程定义按 域/应用/模块 归属与过滤（对齐数据字典定义的 DAM 三段选择器）。
-- 说明：module 列 007 已有，此处仅补 domain / application 两列 + DAM 组合索引。

ALTER TABLE cmx_flow_definition ADD COLUMN IF NOT EXISTS domain      VARCHAR(64);
ALTER TABLE cmx_flow_definition ADD COLUMN IF NOT EXISTS application VARCHAR(64);
COMMENT ON COLUMN cmx_flow_definition.domain      IS '所属域（DAM 三段之一，如 fi）';
COMMENT ON COLUMN cmx_flow_definition.application IS '所属应用（DAM 三段之一，如 cmxfico）';
COMMENT ON COLUMN cmx_flow_definition.module      IS '所属模块（DAM 三段之一，如 gl）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_dam ON cmx_flow_definition (domain, application, module);
