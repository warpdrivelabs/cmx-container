-- 激活映射新增「单据字段铸号规则覆盖」列 doc_code_rules（JSONB）。
-- 结构：{单据字段名: ruleCode}，如 {"doc_no":"MDM_GYS"}。
-- 语义：单据保存铸号时，覆盖单据元数据 layers[].code_rule 同名字段（field 匹配）的 ruleCode
--       ——激活配置优先于单据元数据。空对象 {} 则不覆盖，回退用元数据原 ruleCode。
-- 这是「单据号规则由激活映射配置器动态指定」的载体：不同变更单类型（按 source_doc_type+cr_type）
-- 可配不同的单据字段铸号规则，无需改单据元数据 JSON。
--
-- 配套：字典 code 铸号改走字典自身 dictMeta.codeRule（激活器不再读 code_rule_code 列），
--       code_rule_code 列保留但废弃（不删，避免迁移风险）。
--
-- 部署须先跑本迁移再重启 web-server（activation_store 的 list/upsert/find SQL 已含此列）。
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS doc_code_rules JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN mdm_activation.doc_code_rules IS '单据字段铸号规则覆盖 {单据字段:ruleCode}，单据保存铸号时覆盖单据元数据 codeRule 同名字段（激活配置优先）';
