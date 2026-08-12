-- 头表映射分组（纯 UI 展示用）：cmx_mdm_activation 加 header_groups JSONB 列。
-- 存 [{groupCode,groupName,fields:[源字段名]}]，把扁平 header_mapping 按业务分组展示。
-- 激活器（find_by_doc_type / plan_create）不读此列——header_mapping 落库仍扁平 {源:目标}。
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS header_groups JSONB NOT NULL DEFAULT '[]'::jsonb;
COMMENT ON COLUMN cmx_mdm_activation.header_groups IS '头映射分组(UI 展示用,[{groupCode,groupName,fields:[源字段名]}]);激活器不读,header_mapping 落库仍扁平';
