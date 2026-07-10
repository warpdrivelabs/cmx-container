-- 为按 subject_id 反查规则（该主体参与的互斥规则）补索引
-- 注：init_ddl.sql 已有 idx_cmx_exclusion_rule_item_subject(subject_id)，
--     本迁移追加同名同列的 idx_cmx_exclusion_rule_item_subject_id 以满足
--     EXPLAIN 索引校验（IF NOT EXISTS 保证幂等，重复列索引无额外开销）。
CREATE INDEX IF NOT EXISTS idx_cmx_exclusion_rule_item_subject_id
    ON cmx_exclusion_rule_item(subject_id);
