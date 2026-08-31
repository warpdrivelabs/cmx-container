-- =============================================
-- 迁移说明：扩展供应商主数据激活映射（商务/资质/风险/联系人）
-- 影响表：mdm_activation
-- 操作类型：UPDATE JSONB 配置
-- 回滚方式：20260831_001_供应商主数据字段扩展.down.sql
-- =============================================

UPDATE mdm_activation
SET header_mapping = '{"name":"name","short_name":"short_name","supplier_type":"supplier_type","supplier_category":"supplier_category","tax_no":"tax_no","credit_code":"credit_code","legal_person":"legal_person","taxpayer_type":"taxpayer_type","address":"address","phone":"phone","industry":"industry","payment_term":"payment_term","invoice_type":"invoice_type","settle_currency_id":"settle_currency_id","buyer_id":"buyer_id","risk_level":"risk_level","status":"status"}'::jsonb,
    line_mappings = '[{"lineType":"bank","targetDict":"supplier_bank","targetTable":"cm_bank_account","parentIdField":"supplier_id","fields":{"account_no":"account_no","account_name":"account_name","bank_name":"bank_name","account_type":"account_type","currency_id":"currency_id","bank_code":"bank_code","is_default":"is_default"},"fieldOrder":["account_no","account_name","bank_name","account_type","currency_id","bank_code","is_default"]},{"lineType":"contact","targetDict":"supplier_contact","targetTable":"cm_supplier_contact","parentIdField":"supplier_id","fields":{"contact_name":"contact_name","department":"department","position":"position","phone":"phone","email":"email","is_default":"is_default"},"fieldOrder":["contact_name","department","position","phone","email","is_default"]}]'::jsonb,
    header_groups = '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","supplier_type","supplier_category"]},{"groupCode":"qual","groupName":"资质与联系","fields":["tax_no","credit_code","legal_person","taxpayer_type","address","phone"]},{"groupCode":"biz","groupName":"商务信息","fields":["industry","payment_term","invoice_type","settle_currency_id"]},{"groupCode":"gov","groupName":"治理与归属","fields":["buyer_id","risk_level","status"]}]'::jsonb,
    updated_at = now()
WHERE activation_code IN ('mdm_act_gys_create', 'mdm_act_gys_update');
