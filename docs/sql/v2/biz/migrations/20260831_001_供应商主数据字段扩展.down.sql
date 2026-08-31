-- =============================================
-- 迁移说明：回滚供应商主数据激活映射扩展
-- 影响表：mdm_activation
-- 操作类型：UPDATE JSONB 配置
-- 回滚方式：无
-- =============================================

UPDATE mdm_activation
SET header_mapping = '{"name":"name","short_name":"short_name","tax_no":"tax_no","credit_code":"credit_code","phone":"phone"}'::jsonb,
    line_mappings = '[{"lineType":"bank","targetDict":"supplier_bank","targetTable":"cm_bank_account","parentIdField":"supplier_id","fields":{"account_no":"account_no","bank_name":"bank_name"},"fieldOrder":["account_no","bank_name"]}]'::jsonb,
    header_groups = '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name"]},{"groupCode":"qual","groupName":"资质与联系","fields":["tax_no","credit_code","phone"]}]'::jsonb,
    updated_at = now()
WHERE activation_code IN ('mdm_act_gys_create', 'mdm_act_gys_update');
