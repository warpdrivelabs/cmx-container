-- =============================================
-- 迁移说明：给 MDM 多域种子编码规则（20260818_001 引入的 13 条）补 domain/application/module 归属，
--           使规则管理页按入口模块过滤时能正确归组（basic/dataplatform/mdm）。
--           仅补归属为空的行，不覆盖用户已设置的归属。
-- 影响表：cmx_code_rule
-- 操作类型：UPDATE
-- 回滚方式：无（数据修复类，回滚无意义）
-- =============================================

UPDATE cmx_code_rule
SET domain_code = 'basic', application_code = 'dataplatform', module_code = 'mdm'
WHERE rule_code IN ('MDM_KH', 'MDM_WL', 'MDM_KJ', 'MDM_BZ', 'MDM_JLDW', 'MDM_WLDL',
                    'MDM_CBZX', 'MDM_LRZX', 'MDM_GS', 'MDM_ZZ', 'MDM_BM', 'MDM_GW', 'MDM_YG')
  AND COALESCE(domain_code, '') = '';
