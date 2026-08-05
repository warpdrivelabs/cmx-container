-- MDM 查重规则配置（治理表，md_ 前缀 + BIGINT 主键，对齐 md_match_group 规范）
-- 约定:无 FOREIGN KEY(关联字段+索引替代);md_ 治理表主键 BIGINT(承接 cm_*.id BIGINT 外键);时间戳 TIMESTAMPTZ
-- 规则维护发生在查重界面内（选字典→选已有规则/新建/编辑弹框），无独立管理页。

CREATE TABLE IF NOT EXISTS md_match_config
(
    id             BIGINT       NOT NULL,
    rule_name      VARCHAR(128) NOT NULL,
    dict_code      VARCHAR(64)  NOT NULL,
    target_table   VARCHAR(64)  NOT NULL,
    specs          JSONB        NOT NULL DEFAULT '[]'::jsonb,
    cluster_keys   JSONB        NOT NULL DEFAULT '[]'::jsonb,
    survive_fields JSONB        NOT NULL DEFAULT '[]'::jsonb,
    thresholds     JSONB,
    is_active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_match_config IS 'MDM 查重规则配置(按字典维度),查重界面内维护,find-duplicates 读取执行';
COMMENT ON COLUMN md_match_config.id IS '主键(应用层 next_pk_id 生成)';
COMMENT ON COLUMN md_match_config.rule_name IS '规则名称(用户起名,如「供应商默认查重」)';
COMMENT ON COLUMN md_match_config.dict_code IS '适用字典码(如 supplier)';
COMMENT ON COLUMN md_match_config.target_table IS '目标头物理表名(如 cm_supplier,从 dct/meta tableName 带入)';
COMMENT ON COLUMN md_match_config.specs IS '比较字段 [{field,weight,kind:Exact|EditDistance}]';
COMMENT ON COLUMN md_match_config.cluster_keys IS '分块簇键 [字段名](通常从 specs 派生)';
COMMENT ON COLUMN md_match_config.survive_fields IS '存活字段 [字段名](合并时按 survivorship 规则取值)';
COMMENT ON COLUMN md_match_config.thresholds IS '双阈值 {auto_merge:95,review:80}(可空,用默认)';
COMMENT ON COLUMN md_match_config.is_active IS '是否启用';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_match_config_dict_rule ON md_match_config (dict_code, rule_name);
CREATE        INDEX IF NOT EXISTS idx_md_match_config_dict      ON md_match_config (dict_code);

-- Seed:supplier 默认查重规则（id 用固定值 1，应用层 next_pk_id 不会冲突到此处）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 1, '供应商默认查重', 'supplier', 'cm_supplier',
       '[{"field":"credit_code","weight":40,"kind":"Exact"},{"field":"tax_no","weight":30,"kind":"Exact"},{"field":"name","weight":30,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","tax_no","name"]'::jsonb,
       '["name","tax_no","credit_code","short_name","phone"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'supplier' AND rule_name = '供应商默认查重');
