-- 回滚：移除 doc_code_rules 列（激活映射的「单据字段铸号规则覆盖」）。
ALTER TABLE mdm_activation DROP COLUMN IF EXISTS doc_code_rules;
