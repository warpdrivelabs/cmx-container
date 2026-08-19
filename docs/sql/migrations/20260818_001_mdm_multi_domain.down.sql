-- =============================================
-- 回滚：MDM M6 多域扩展全量种子
-- 说明：仅删除本迁移 seed 的配置行；cm_customer / cm_material / cm_gl_account 等物理表
--       由模型中心 deploy DCT 创建，不在本迁移回滚范围。
-- =============================================

-- 激活映射（按 activation_code 定位，幂等）
DELETE FROM cmx_mdm_activation
WHERE activation_code IN ('kh__create', 'kh__update', 'wl__create', 'wl__update', 'kj__create', 'kj__update',
                          'bz__create', 'bz__update', 'jldw__create', 'jldw__update', 'wldl__create', 'wldl__update',
                          'cbzx__create', 'cbzx__update', 'lrzx__create', 'lrzx__update', 'gs__create', 'gs__update',
                          'zz__create', 'zz__update', 'bm__create', 'bm__update', 'gw__create', 'gw__update',
                          'yg__create', 'yg__update');

-- 编码规则（固定 id + rule_code 双条件，避免误删用户自行新建的同名规则）
DELETE FROM cmx_code_rule
WHERE id BETWEEN 9000000000000002 AND 9000000000000015
  AND rule_code IN ('MDM_KH', 'MDM_WL', 'MDM_KJ', 'MDM_BZ', 'MDM_JLDW', 'MDM_WLDL', 'MDM_CBZX', 'MDM_LRZX',
                    'MDM_GS', 'MDM_ZZ', 'MDM_BM', 'MDM_GW', 'MDM_YG', 'MDM_GYS');

-- 查重规则（固定 id 双条件）
DELETE FROM md_match_config
WHERE id BETWEEN 2 AND 14
  AND dict_code IN ('customer', 'material', 'gl_account', 'currency', 'uom', 'material_class',
                    'cost_center', 'profit_center', 'company', 'organization', 'department', 'position', 'employee');
