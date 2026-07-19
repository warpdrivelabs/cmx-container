-- cmx-flow 流程定义版本补 note 列（变更说明，对标报表版本 change_summary）
-- 幂等：ADD COLUMN IF NOT EXISTS。
-- 依赖：20260718_007_cmx_flow_definition（cmx_flow_definition_version 表）。
-- 涉及变更：发布时可填写变更说明；版本管理界面展示每个版本的说明。

ALTER TABLE cmx_flow_definition_version ADD COLUMN IF NOT EXISTS note VARCHAR(512);
COMMENT ON COLUMN cmx_flow_definition_version.note IS '本版本变更说明（发布时填写，可空）';
