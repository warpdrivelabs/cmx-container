-- MDM 激活映射新增关键信息字段列
-- cmx_mdm_activation.key_fields：[{field,weight,kind,dedup}]
-- cr-form 步骤①「关键信息」表单按此渲染多字段；dedup=true 的字段构造 /mdm/check-key
-- 多字段加权查重（综合分 ≥80 阻断录入），dedup=false 仅展示采集不查重；
-- 空数组则无步骤①（新增直接完整表单，不查重），存量配置零影响。
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS key_fields JSONB NOT NULL DEFAULT '[]'::jsonb;

COMMENT ON COLUMN cmx_mdm_activation.key_fields IS '关键信息字段 [{field,weight,kind,dedup}];field=目标字典列名,数组序=簇键优先级;cr-form 据此渲染步骤①关键信息表单,dedup=true 的字段构造 /mdm/check-key 多字段加权查重,dedup=false 仅展示采集不查重;空则无步骤①(直接完整表单,不查重)';
