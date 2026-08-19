-- ============================================================
-- CMX 业务库内置数据（DML）— docs/sql/v2/biz/init_dml.sql
--
-- 目标库：业务数据源（source_type = "biz"）
-- 内容：MDM 治理种子（查重规则 + 分发水位）
-- 风格：无损幂等，全部 ON CONFLICT / NOT EXISTS 防重，可重复执行
-- 来源：迁移 20260812_001（supplier 默认查重）+ 20260818_001 段3（13 条）
-- ============================================================

-- ============================================================
-- 1. 查重规则（md_match_config）
-- ============================================================

-- Seed: supplier 默认查重规则（id 固定值 1，应用层 next_pk_id 不会冲突）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 1, '供应商默认查重', 'supplier', 'cm_supplier',
       '[{"field":"credit_code","weight":40,"kind":"Exact"},{"field":"tax_no","weight":30,"kind":"Exact"},{"field":"name","weight":30,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","tax_no","name"]'::jsonb,
       '["name","tax_no","credit_code","short_name","phone"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'supplier' AND rule_name = '供应商默认查重');

-- ---- MDM 多域查重规则 13 条（20260818_001 段3） ----
-- ─────────────────────────────────────────────
-- 4. 查重规则 md_match_config（id 2~14 顺排；NOT EXISTS 防重）
-- ─────────────────────────────────────────────


INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 2, '客户默认查重', 'customer', 'cm_customer',
       '[{"field":"credit_code","weight":40,"kind":"Exact"},{"field":"name","weight":60,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","name"]'::jsonb,
       '["name","short_name","customer_type","credit_level","credit_code","tax_no","phone","address"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'customer' AND rule_name = '客户默认查重');

-- 物料默认查重：名称编辑距离 + 规格/型号精确（同名不同规格是不同物料）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 3, '物料默认查重', 'material', 'cm_material',
       '[{"field":"name","weight":50,"kind":"EditDistance"},{"field":"spec","weight":25,"kind":"Exact"},{"field":"model","weight":25,"kind":"Exact"}]'::jsonb,
       '["name","spec","model"]'::jsonb,
       '["name","short_name","spec","model","unit","material_type","barcode"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'material' AND rule_name = '物料默认查重');

-- 科目默认查重：科目编码精确 + 名称编辑距离（survive 不含 parent_id/full_path 等树形列——层级结构不参与字段存活裁决）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 4, '科目默认查重', 'gl_account', 'cm_gl_account',
       '[{"field":"acct_no","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["acct_no","name"]'::jsonb,
       '["name","acct_no","acct_type","direction"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'gl_account' AND rule_name = '科目默认查重');




INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 5, '币种默认查重', 'currency', 'cm_currency',
       '[{"field":"currency_code","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["currency_code","name"]'::jsonb,
       '["name","symbol","decimal_places","is_base"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'currency' AND rule_name = '币种默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 6, '计量单位默认查重', 'uom', 'cm_uom',
       '[{"field":"uom_code","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["uom_code","name"]'::jsonb,
       '["name","unit_type"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'uom' AND rule_name = '计量单位默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 7, '物料分类默认查重', 'material_class', 'cm_material_class',
       '[{"field":"class_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["class_code","name"]'::jsonb,
       '["name","class_type"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'material_class' AND rule_name = '物料分类默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 8, '成本中心默认查重', 'cost_center', 'cm_cost_center',
       '[{"field":"cost_center_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["cost_center_code","name"]'::jsonb,
       '["name","dept_id"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'cost_center' AND rule_name = '成本中心默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 9, '利润中心默认查重', 'profit_center', 'cm_profit_center',
       '[{"field":"profit_center_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["profit_center_code","name"]'::jsonb,
       '["name"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'profit_center' AND rule_name = '利润中心默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 10, '公司默认查重', 'company', 'cm_company',
       '[{"field":"credit_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","name"]'::jsonb,
       '["name","short_name","credit_code","legal_person","base_currency_id","registered_address"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'company' AND rule_name = '公司默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 11, '组织默认查重', 'organization', 'cm_organization',
       '[{"field":"org_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["org_code","name"]'::jsonb,
       '["name","company_id","org_type"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'organization' AND rule_name = '组织默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 12, '部门默认查重', 'department', 'cm_department',
       '[{"field":"dept_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["dept_code","name"]'::jsonb,
       '["name","org_id"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'department' AND rule_name = '部门默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 13, '岗位默认查重', 'position', 'cm_position',
       '[{"field":"position_code","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["position_code","name"]'::jsonb,
       '["name","job_family","job_grade"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'position' AND rule_name = '岗位默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 14, '员工默认查重', 'employee', 'cm_employee',
       '[{"field":"emp_no","weight":50,"kind":"Exact"},{"field":"name","weight":30,"kind":"EditDistance"},{"field":"mobile","weight":20,"kind":"Exact"}]'::jsonb,
       '["emp_no","name","mobile"]'::jsonb,
       '["name","mobile","email","company_id","dept_id","position_id","hire_date","emp_status"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'employee' AND rule_name = '员工默认查重');

-- ============================================================
-- 5. MDM 激活映射（mdm_activation）
-- 来源：迁移 20260818_001 段2（10 新域 × create/update 共 26 条）
-- 幂等：ON CONFLICT (activation_code) DO UPDATE
-- ============================================================

-- ─────────────────────────────────────────────
-- 2. 激活映射：10 个新域（create + update 各一条；update 的 key_fields 留空——步骤①查重仅新建场景）
-- ─────────────────────────────────────────────

-- currency · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_bz_create', 'bz__create', 'bz', 'create', 'currency', 'cm_currency',
        '{"currency_code":"currency_code","name":"name","symbol":"symbol","decimal_places":"decimal_places","is_base":"is_base"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"currency_code","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["currency_code","name","symbol"]},{"groupCode":"attr","groupName":"属性","fields":["decimal_places","is_base"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- currency · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_bz_update', 'bz__update', 'bz', 'update', 'currency', 'cm_currency',
        '{"currency_code":"currency_code","name":"name","symbol":"symbol","decimal_places":"decimal_places","is_base":"is_base"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["currency_code","name","symbol"]},{"groupCode":"attr","groupName":"属性","fields":["decimal_places","is_base"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- uom · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_jldw_create', 'jldw__create', 'jldw', 'create', 'uom', 'cm_uom',
        '{"uom_code":"uom_code","name":"name","unit_type":"unit_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"uom_code","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["uom_code","name","unit_type"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- uom · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_jldw_update', 'jldw__update', 'jldw', 'update', 'uom', 'cm_uom',
        '{"uom_code":"uom_code","name":"name","unit_type":"unit_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["uom_code","name","unit_type"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- material_class · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_wldl_create', 'wldl__create', 'wldl', 'create', 'material_class', 'cm_material_class',
        '{"class_code":"class_code","name":"name","parent_id":"parent_id","class_type":"class_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"class_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["class_code","name","parent_id"]},{"groupCode":"attr","groupName":"类别属性","fields":["class_type"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- material_class · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_wldl_update', 'wldl__update', 'wldl', 'update', 'material_class', 'cm_material_class',
        '{"class_code":"class_code","name":"name","parent_id":"parent_id","class_type":"class_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["class_code","name","parent_id"]},{"groupCode":"attr","groupName":"类别属性","fields":["class_type"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- cost_center · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_cbzx_create', 'cbzx__create', 'cbzx', 'create', 'cost_center', 'cm_cost_center',
        '{"cost_center_code":"cost_center_code","name":"name","parent_id":"parent_id","dept_id":"dept_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"cost_center_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["cost_center_code","name","parent_id"]},{"groupCode":"resp","groupName":"责任归属","fields":["dept_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- cost_center · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_cbzx_update', 'cbzx__update', 'cbzx', 'update', 'cost_center', 'cm_cost_center',
        '{"cost_center_code":"cost_center_code","name":"name","parent_id":"parent_id","dept_id":"dept_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["cost_center_code","name","parent_id"]},{"groupCode":"resp","groupName":"责任归属","fields":["dept_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- profit_center · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_lrzx_create', 'lrzx__create', 'lrzx', 'create', 'profit_center', 'cm_profit_center',
        '{"profit_center_code":"profit_center_code","name":"name","parent_id":"parent_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"profit_center_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["profit_center_code","name","parent_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- profit_center · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_lrzx_update', 'lrzx__update', 'lrzx', 'update', 'profit_center', 'cm_profit_center',
        '{"profit_center_code":"profit_center_code","name":"name","parent_id":"parent_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["profit_center_code","name","parent_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- company · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_gs_create', 'gs__create', 'gs', 'create', 'company', 'cm_company',
        '{"company_code":"company_code","name":"name","short_name":"short_name","credit_code":"credit_code","legal_person":"legal_person","base_currency_id":"base_currency_id","registered_address":"registered_address"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"credit_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["company_code","name","short_name"]},{"groupCode":"reg","groupName":"注册信息","fields":["credit_code","legal_person","registered_address"]},{"groupCode":"fin","groupName":"财务","fields":["base_currency_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- company · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_gs_update', 'gs__update', 'gs', 'update', 'company', 'cm_company',
        '{"company_code":"company_code","name":"name","short_name":"short_name","credit_code":"credit_code","legal_person":"legal_person","base_currency_id":"base_currency_id","registered_address":"registered_address"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["company_code","name","short_name"]},{"groupCode":"reg","groupName":"注册信息","fields":["credit_code","legal_person","registered_address"]},{"groupCode":"fin","groupName":"财务","fields":["base_currency_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- organization · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_zz_create', 'zz__create', 'zz', 'create', 'organization', 'cm_organization',
        '{"org_code":"org_code","name":"name","parent_id":"parent_id","company_id":"company_id","org_type":"org_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"org_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["org_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织属性","fields":["company_id","org_type"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- organization · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_zz_update', 'zz__update', 'zz', 'update', 'organization', 'cm_organization',
        '{"org_code":"org_code","name":"name","parent_id":"parent_id","company_id":"company_id","org_type":"org_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["org_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织属性","fields":["company_id","org_type"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- department · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_bm_create', 'bm__create', 'bm', 'create', 'department', 'cm_department',
        '{"dept_code":"dept_code","name":"name","parent_id":"parent_id","org_id":"org_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"dept_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["dept_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织归属","fields":["org_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- department · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_bm_update', 'bm__update', 'bm', 'update', 'department', 'cm_department',
        '{"dept_code":"dept_code","name":"name","parent_id":"parent_id","org_id":"org_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["dept_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织归属","fields":["org_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- position · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_gw_create', 'gw__create', 'gw', 'create', 'position', 'cm_position',
        '{"position_code":"position_code","name":"name","job_family":"job_family","job_grade":"job_grade"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"position_code","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["position_code","name"]},{"groupCode":"attr","groupName":"职级职族","fields":["job_family","job_grade"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- position · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_gw_update', 'gw__update', 'gw', 'update', 'position', 'cm_position',
        '{"position_code":"position_code","name":"name","job_family":"job_family","job_grade":"job_grade"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["position_code","name"]},{"groupCode":"attr","groupName":"职级职族","fields":["job_family","job_grade"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- employee · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_yg_create', 'yg__create', 'yg', 'create', 'employee', 'cm_employee',
        '{"emp_no":"emp_no","name":"name","company_id":"company_id","dept_id":"dept_id","position_id":"position_id","mobile":"mobile","email":"email","hire_date":"hire_date","emp_status":"emp_status"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"emp_no","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":30,"kind":"EditDistance","dedup":true},{"field":"mobile","weight":20,"kind":"Exact","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["emp_no","name","mobile","email"]},{"groupCode":"org","groupName":"组织归属","fields":["company_id","dept_id","position_id"]},{"groupCode":"attr","groupName":"人事属性","fields":["hire_date","emp_status"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- employee · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_yg_update', 'yg__update', 'yg', 'update', 'employee', 'cm_employee',
        '{"emp_no":"emp_no","name":"name","company_id":"company_id","dept_id":"dept_id","position_id":"position_id","mobile":"mobile","email":"email","hire_date":"hire_date","emp_status":"emp_status"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["emp_no","name","mobile","email"]},{"groupCode":"org","groupName":"组织归属","fields":["company_id","dept_id","position_id"]},{"groupCode":"attr","groupName":"人事属性","fields":["hire_date","emp_status"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();


-- ─────────────────────────────────────────────
-- 3. 第一批三域激活映射（kh/wl/kj，深化字段版：客户商务/客户经理/地址明细、物料分类/多单位、科目辅助核算）
-- ─────────────────────────────────────────────

-- customer · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_kh_create', 'kh__create', 'kh', 'create', 'customer', 'cm_customer',
        '{"name":"name","short_name":"short_name","customer_type":"customer_type","credit_level":"credit_level","credit_code":"credit_code","tax_no":"tax_no","phone":"phone","address":"address","credit_limit":"credit_limit","payment_term":"payment_term","invoice_type":"invoice_type","industry":"industry","customer_manager_id":"customer_manager_id","settle_currency_id":"settle_currency_id"}'::jsonb,
        '[{"lineType":"bank","targetDict":"customer_bank","targetTable":"cm_customer_bank","parentIdField":"customer_id","fields":{"account_no":"account_no","account_name":"account_name","bank_name":"bank_name","is_default":"is_default"},"fieldOrder":["account_no","account_name","bank_name","is_default"]},{"lineType":"contact","targetDict":"customer_contact","targetTable":"cm_customer_contact","parentIdField":"customer_id","fields":{"contact_name":"contact_name","position":"position","phone":"phone","email":"email"},"fieldOrder":["contact_name","position","phone","email"]},{"lineType":"address","targetDict":"customer_address","targetTable":"cm_customer_address","parentIdField":"customer_id","fields":{"address_type":"address_type","province":"province","city":"city","district":"district","address_detail":"address_detail","receiver":"receiver","receiver_phone":"receiver_phone","is_default":"is_default"},"fieldOrder":["address_type","province","city","district","address_detail","receiver","receiver_phone","is_default"]}]'::jsonb,
        'name',
        '[{"field":"credit_code","weight":40,"kind":"Exact","dedup":true},{"field":"name","weight":60,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","customer_type","credit_level"]},{"groupCode":"qual","groupName":"资质与联系","fields":["credit_code","tax_no","phone","address"]},{"groupCode":"biz","groupName":"商务信息","fields":["credit_limit","payment_term","invoice_type","industry"]},{"groupCode":"mgr","groupName":"归属","fields":["customer_manager_id","settle_currency_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- customer · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_kh_update', 'kh__update', 'kh', 'update', 'customer', 'cm_customer',
        '{"name":"name","short_name":"short_name","customer_type":"customer_type","credit_level":"credit_level","credit_code":"credit_code","tax_no":"tax_no","phone":"phone","address":"address","credit_limit":"credit_limit","payment_term":"payment_term","invoice_type":"invoice_type","industry":"industry","customer_manager_id":"customer_manager_id","settle_currency_id":"settle_currency_id"}'::jsonb,
        '[{"lineType":"bank","targetDict":"customer_bank","targetTable":"cm_customer_bank","parentIdField":"customer_id","fields":{"account_no":"account_no","account_name":"account_name","bank_name":"bank_name","is_default":"is_default"},"fieldOrder":["account_no","account_name","bank_name","is_default"]},{"lineType":"contact","targetDict":"customer_contact","targetTable":"cm_customer_contact","parentIdField":"customer_id","fields":{"contact_name":"contact_name","position":"position","phone":"phone","email":"email"},"fieldOrder":["contact_name","position","phone","email"]},{"lineType":"address","targetDict":"customer_address","targetTable":"cm_customer_address","parentIdField":"customer_id","fields":{"address_type":"address_type","province":"province","city":"city","district":"district","address_detail":"address_detail","receiver":"receiver","receiver_phone":"receiver_phone","is_default":"is_default"},"fieldOrder":["address_type","province","city","district","address_detail","receiver","receiver_phone","is_default"]}]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","customer_type","credit_level"]},{"groupCode":"qual","groupName":"资质与联系","fields":["credit_code","tax_no","phone","address"]},{"groupCode":"biz","groupName":"商务信息","fields":["credit_limit","payment_term","invoice_type","industry"]},{"groupCode":"mgr","groupName":"归属","fields":["customer_manager_id","settle_currency_id"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- material · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_wl_create', 'wl__create', 'wl', 'create', 'material', 'cm_material',
        '{"name":"name","short_name":"short_name","spec":"spec","model":"model","class_id":"class_id","material_type":"material_type","barcode":"barcode","base_uom_id":"base_uom_id","purchase_uom_id":"purchase_uom_id","stock_uom_id":"stock_uom_id","purchase_rate":"purchase_rate","brand":"brand","origin":"origin","net_weight":"net_weight","shelf_life_days":"shelf_life_days","batch_flag":"batch_flag","serial_flag":"serial_flag","hs_code":"hs_code","long_desc":"long_desc"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"name","weight":50,"kind":"EditDistance","dedup":true},{"field":"spec","weight":25,"kind":"Exact","dedup":true},{"field":"model","weight":25,"kind":"Exact","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","material_type","class_id"]},{"groupCode":"spec","groupName":"规格属性","fields":["spec","model","barcode","brand","origin"]},{"groupCode":"uom","groupName":"单位体系","fields":["base_uom_id","purchase_uom_id","stock_uom_id","purchase_rate"]},{"groupCode":"ext","groupName":"扩展属性","fields":["net_weight","shelf_life_days","batch_flag","serial_flag","hs_code","long_desc"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- material · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_wl_update', 'wl__update', 'wl', 'update', 'material', 'cm_material',
        '{"name":"name","short_name":"short_name","spec":"spec","model":"model","class_id":"class_id","material_type":"material_type","barcode":"barcode","base_uom_id":"base_uom_id","purchase_uom_id":"purchase_uom_id","stock_uom_id":"stock_uom_id","purchase_rate":"purchase_rate","brand":"brand","origin":"origin","net_weight":"net_weight","shelf_life_days":"shelf_life_days","batch_flag":"batch_flag","serial_flag":"serial_flag","hs_code":"hs_code","long_desc":"long_desc"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","material_type","class_id"]},{"groupCode":"spec","groupName":"规格属性","fields":["spec","model","barcode","brand","origin"]},{"groupCode":"uom","groupName":"单位体系","fields":["base_uom_id","purchase_uom_id","stock_uom_id","purchase_rate"]},{"groupCode":"ext","groupName":"扩展属性","fields":["net_weight","shelf_life_days","batch_flag","serial_flag","hs_code","long_desc"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- gl_account · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_kj_create', 'kj__create', 'kj', 'create', 'gl_account', 'cm_gl_account',
        '{"acct_no":"acct_no","name":"name","parent_id":"parent_id","acct_type":"acct_type","direction":"direction","aux_biz_partner":"aux_biz_partner","aux_department":"aux_department","aux_employee":"aux_employee","aux_project":"aux_project","is_cash_flow":"is_cash_flow","foreign_currency_flag":"foreign_currency_flag","quantity_flag":"quantity_flag","ledger_format":"ledger_format"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"acct_no","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["acct_no","name","parent_id"]},{"groupCode":"attr","groupName":"科目属性","fields":["acct_type","direction"]},{"groupCode":"aux","groupName":"辅助核算","fields":["aux_biz_partner","aux_department","aux_employee","aux_project"]},{"groupCode":"gl","groupName":"核算控制","fields":["is_cash_flow","foreign_currency_flag","quantity_flag","ledger_format"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- gl_account · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                            header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                            header_groups, is_active)
VALUES ('mdm_act_kj_update', 'kj__update', 'kj', 'update', 'gl_account', 'cm_gl_account',
        '{"acct_no":"acct_no","name":"name","parent_id":"parent_id","acct_type":"acct_type","direction":"direction","aux_biz_partner":"aux_biz_partner","aux_department":"aux_department","aux_employee":"aux_employee","aux_project":"aux_project","is_cash_flow":"is_cash_flow","foreign_currency_flag":"foreign_currency_flag","quantity_flag":"quantity_flag","ledger_format":"ledger_format"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["acct_no","name","parent_id"]},{"groupCode":"attr","groupName":"科目属性","fields":["acct_type","direction"]},{"groupCode":"aux","groupName":"辅助核算","fields":["aux_biz_partner","aux_department","aux_employee","aux_project"]},{"groupCode":"gl","groupName":"核算控制","fields":["is_cash_flow","foreign_currency_flag","quantity_flag","ledger_format"]}]'::jsonb,
        TRUE)
    ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
                                         target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
                                         header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
                                         subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
                                         doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
                                         is_active = EXCLUDED.is_active, updated_at = now();

-- ============================================================
-- 2. 分发水位（md_dist_watermark）
-- ============================================================

INSERT INTO md_dist_watermark (key, last_seq) VALUES ('fanout', 0) ON CONFLICT (key) DO NOTHING;
