-- =============================================
-- cmx-container 初始数据 (DML)
-- 包含：域、应用、模块的初始数据
-- =============================================

-- =============================================
-- 1. 域数据
-- =============================================
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000001', 'FIN', '资金与价值流领域', '企业的记账本，所有业务最终化为货币数字流入此领域', 'business',
        1, 1, 0);
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000002', 'LOG', '物流与供应链领域', '管理实物资产的买入、存放和流转', 'business', 2, 1, 0);
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000003', 'SAL', '营收与客户领域', '企业的赚钱引擎', 'business', 3, 1, 0);
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000004', 'MFG', '制造与工程领域', '把原材料变成可以卖出去的成品', 'business', 4, 1, 0);
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000005', 'HCM', '组织与人力领域', '企业的社会网络', 'business', 5, 1, 0);
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000006', 'XAP', '跨应用基座领域', '公共底层领域，为全系统提供主数据和公共服务', 'technical', 6, 1,
        0);

-- =============================================
-- 2. 应用数据
-- =============================================
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000011', 'FI', 'FIN', '财务会计', '对外报告，满足税务审计合规要求', 'product', 1, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000012', 'CO', 'FIN', '管理会计', '对内分析，算清部门和产品的盈亏', 'product', 2, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000013', 'MM', 'LOG', '物料管理', '采购、库存、发票校验', 'product', 1, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000014', 'EWM', 'LOG', '仓储管理', '管理物理货架与仓库作业', 'product', 2, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000015', 'SD', 'SAL', '销售与分销', '销售订单、交货、开票与定价', 'product', 1, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000016', 'PP', 'MFG', '生产计划', '物料需求运算、车间排程', 'product', 1, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000017', 'QPM', 'MFG', '质量管理与设备维护', '质量检验与设备维护', 'product', 2, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000018', 'HRM', 'HCM', '人力资源管理', '组织、人事、薪资、考勤', 'product', 1, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000019', 'BP', 'XAP', '商业伙伴主数据', '统一管理客户、供应商、联系人', 'platform', 1, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000020', 'MDM', 'XAP', '物料主数据', '物料基本信息与分类，全系统共享', 'platform', 2, 1, 0);
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000021', 'CA', 'XAP', '跨应用组件', '分类系统、文档管理、权限管理', 'platform', 3, 1, 0);

-- =============================================
-- 3. 模块数据
-- =============================================
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000101', 'GL', 'FIN', 'FI', '总账', '总分类账管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000102', 'AR', 'FIN', 'FI', '应收账款', '应收账款管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000103', 'AP', 'FIN', 'FI', '应付账款', '应付账款管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000104', 'AA', 'FIN', 'FI', '固定资产', '固定资产核算', 'business', 4, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000105', 'CCA', 'FIN', 'CO', '成本中心会计', '成本中心核算', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000106', 'PCA', 'FIN', 'CO', '利润中心会计', '利润中心核算', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000107', 'IO', 'FIN', 'CO', '内部订单', '内部订单管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000108', 'PUR', 'LOG', 'MM', '采购管理', '采购流程管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000109', 'INV', 'LOG', 'MM', '库存管理', '库存数量与价值管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000110', 'IV', 'LOG', 'MM', '发票校验', '采购发票校验', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000111', 'INB', 'LOG', 'EWM', '入库管理', '入库流程管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000112', 'OUT', 'LOG', 'EWM', '出库管理', '出库流程管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000113', 'BIN', 'LOG', 'EWM', '货位管理', '仓库货位管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000114', 'SOM', 'SAL', 'SD', '销售订单管理', '销售订单全流程管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000115', 'DLM', 'SAL', 'SD', '交货管理', '交货流程管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000116', 'BLM', 'SAL', 'SD', '开票管理', '开票流程管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000117', 'PRM', 'SAL', 'SD', '定价管理', '定价策略管理', 'business', 4, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000118', 'BOM', 'MFG', 'PP', '物料清单', '物料清单管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000119', 'RTG', 'MFG', 'PP', '工艺路线', '工艺路线管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000120', 'PROD', 'MFG', 'PP', '生产订单', '生产订单管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000121', 'QI', 'MFG', 'QPM', '质量检验', '质量检验管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000122', 'PM', 'MFG', 'QPM', '设备维护', '设备维护管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000123', 'FL', 'MFG', 'QPM', '功能位置', '功能位置管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000124', 'OM', 'HCM', 'HRM', '组织管理', '组织架构管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000125', 'PAM', 'HCM', 'HRM', '人事管理', '人事信息管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000126', 'PAY', 'HCM', 'HRM', '薪资管理', '薪资核算管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000127', 'TA', 'HCM', 'HRM', '考勤管理', '考勤记录管理', 'business', 4, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000128', 'CUS', 'XAP', 'BP', '客户管理', '客户信息管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000129', 'VEN', 'XAP', 'BP', '供应商管理', '供应商信息管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000130', 'CON', 'XAP', 'BP', '联系人管理', '联系人信息管理', 'business', 3, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000131', 'MBI', 'XAP', 'MDM', '物料基本信息', '物料基本信息管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000132', 'MCL', 'XAP', 'MDM', '物料分类', '物料分类管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000133', 'CLS', 'XAP', 'CA', '分类系统', '分类体系管理', 'business', 1, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000134', 'DMS', 'XAP', 'CA', '文档管理', '文档存储与管理', 'business', 2, 1, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000135', 'AUTH', 'XAP', 'CA', '权限管理', '权限配置与管理', 'business', 3, 1, 0);

-- =============================================
-- 4. 内置角色数据 (cmx_role)
-- =============================================
INSERT INTO cmx_role (id, code, name, data_scope, sort_order, status, archived, description)
VALUES ('1898765432100001001', 'admin', '系统管理员', 1, 1, 1, 0, '拥有全部权限');
INSERT INTO cmx_role (id, code, name, data_scope, sort_order, status, archived, description)
VALUES ('1898765432100001002', 'user', '普通用户', 5, 2, 1, 0, '仅查看本人数据');

-- =============================================
-- 5. 内置权限数据 (cmx_permission)
-- =============================================
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667162255360', 'gl:account', '科目管理', 'menu', null, 3, '会计科目体系维护', 1, 0, '2026-06-25 09:53:36.798272', '2026-06-25 09:53:36.798272', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:account', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676112900096', 'gl:account:add', '科目新增', 'button', '7475848667162255360', 1, '新增会计科目', 1, 0, '2026-06-25 09:53:38.915084', '2026-06-25 09:53:38.915084', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:add', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848677895479296', 'gl:account:delete', '科目删除', 'button', '7475848667162255360', 3, '删除末级科目', 1, 0, '2026-06-25 09:53:39.343854', '2026-06-25 09:53:39.343854', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:delete', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676989509632', 'gl:account:edit', '科目编辑', 'button', '7475848667162255360', 2, '修改科目信息', 1, 0, '2026-06-25 09:53:39.129204', '2026-06-25 09:53:39.129204', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:edit', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848679703224320', 'gl:account:export', '科目导出', 'button', '7475848667162255360', 5, '导出科目体系', 1, 0, '2026-06-25 09:53:39.776420', '2026-06-25 09:53:39.776420', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:export', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848678809837568', 'gl:account:query', '科目查询', 'api', '7475848667162255360', 4, '查询科目树形列表', 1, 0, '2026-06-25 09:53:39.562303', '2026-06-25 09:53:39.562303', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:query', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848665702637568', 'gl:dashboard', '总账仪表盘', 'menu', null, 1, '总账模块概览看板', 1, 0, '2026-06-25 09:53:36.449545', '2026-06-25 09:53:36.449545', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:dashboard', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670211514368', 'gl:dashboard:refresh', '数据刷新', 'button', '7475848665702637568', 2, '手动刷新仪表盘统计', 1, 0, '2026-06-25 09:53:37.499114', '2026-06-25 09:53:37.499114', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:refresh', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848669469122560', 'gl:dashboard:view', '仪表盘查看', 'api', '7475848665702637568', 1, '查看总账仪表盘数据', 1, 0, '2026-06-25 09:53:37.325539', '2026-06-25 09:53:37.325539', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:view', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667845926912', 'gl:period', '期末处理', 'menu', null, 4, '期末结账与损益结转', 1, 0, '2026-06-25 09:53:36.957909', '2026-06-25 09:53:36.957909', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:period', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848680584028160', 'gl:period:close', '期末结账', 'button', '7475848667845926912', 1, '对当前会计期间结账', 1, 0, '2026-06-25 09:53:39.985243', '2026-06-25 09:53:39.985243', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:period', '/gl:period/gl:period:close', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848682249166848', 'gl:period:settle', '损益结转', 'button', '7475848667845926912', 3, '结转本期损益至本年利润', 1, 0, '2026-06-25 09:53:40.383092', '2026-06-25 09:53:40.383092', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:period', '/gl:period/gl:period:settle', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848681427083264', 'gl:period:unclose', '反结账', 'button', '7475848667845926912', 2, '撤销已结账期间', 1, 0, '2026-06-25 09:53:40.186302', '2026-06-25 09:53:40.186302', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:period', '/gl:period/gl:period:unclose', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848668558958592', 'gl:report', '报表中心', 'menu', null, 5, '总账报表查询与导出', 1, 0, '2026-06-25 09:53:37.122063', '2026-06-25 09:53:37.122063', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:report', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848683444543488', 'gl:report:balance', '资产负债表', 'api', '7475848668558958592', 1, '生成资产负债表', 1, 0, '2026-06-25 09:53:40.665266', '2026-06-25 09:53:40.665266', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:report', '/gl:report/gl:report:balance', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848685168402432', 'gl:report:cashflow', '现金流量表', 'api', '7475848668558958592', 3, '生成现金流量表', 1, 0, '2026-06-25 09:53:41.078737', '2026-06-25 09:53:41.078737', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:report', '/gl:report/gl:report:cashflow', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848684400844800', 'gl:report:income', '利润表', 'api', '7475848668558958592', 2, '生成利润表', 1, 0, '2026-06-25 09:53:40.893176', '2026-06-25 09:53:40.893176', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:report', '/gl:report/gl:report:income', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848666486972416', 'gl:voucher', '凭证管理', 'menu', null, 2, '会计凭证全生命周期管理', 1, 0, '2026-06-25 09:53:36.636949', '2026-06-25 09:53:36.636949', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:voucher', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670991654912', 'gl:voucher:add', '凭证录入', 'button', '7475848666486972416', 1, '新增会计凭证', 1, 0, '2026-06-25 09:53:37.676977', '2026-06-25 09:53:37.676977', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:add', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848673445322752', 'gl:voucher:audit', '凭证审核', 'button', '7475848666486972416', 4, '审核/反审核凭证', 1, 0, '2026-06-25 09:53:38.281608', '2026-06-25 09:53:38.281608', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:audit', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848675001409536', 'gl:voucher:delete', '凭证删除', 'button', '7475848666486972416', 6, '删除作废凭证', 1, 0, '2026-06-25 09:53:38.655424', '2026-06-25 09:53:38.655424', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:delete', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848672686153728', 'gl:voucher:edit', '凭证修改', 'button', '7475848666486972416', 3, '修改未审核凭证', 1, 0, '2026-06-25 09:53:38.088995', '2026-06-25 09:53:38.088995', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:edit', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848674095439872', 'gl:voucher:post', '凭证过账', 'button', '7475848666486972416', 5, '将审核凭证过账到账簿', 1, 0, '2026-06-25 09:53:38.439356', '2026-06-25 09:53:38.439356', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:post', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848671801155584', 'gl:voucher:query', '凭证查询', 'api', '7475848666486972416', 2, '按条件查询凭证列表', 1, 0, '2026-06-25 09:53:37.878384', '2026-06-25 09:53:37.878384', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:query', 1, 2);





-- INSERT INTO cmx_permission (id, code, name, resource_type, sort_order, status, archived, description) VALUES
--     ('1898765432100002001', 'user:list',        '用户列表',   'api',  1, 1, 0, '查看用户列表'),
--     ('1898765432100002002', 'user:create',      '创建用户',   'api',  2, 1, 0, '创建新用户'),
--     ('1898765432100002003', 'user:read',        '查看用户',   'api',  3, 1, 0, '查看用户详情'),
--     ('1898765432100002004', 'user:update',      '更新用户',   'api',  4, 1, 0, '更新用户信息'),
--     ('1898765432100002005', 'user:delete',      '删除用户',   'api',  5, 1, 0, '删除用户'),
--     ('1898765432100002006', 'user:assign_role', '分配角色',   'api',  6, 1, 0, '为用户分配角色'),
--     ('1898765432100002011', 'role:list',        '角色列表',   'api', 11, 1, 0, '查看角色列表'),
--     ('1898765432100002012', 'role:create',      '创建角色',   'api', 12, 1, 0, '创建新角色'),
--     ('1898765432100002013', 'role:read',        '查看角色',   'api', 13, 1, 0, '查看角色详情'),
--     ('1898765432100002014', 'role:update',      '更新角色',   'api', 14, 1, 0, '更新角色信息'),
--     ('1898765432100002015', 'role:delete',      '删除角色',   'api', 15, 1, 0, '删除角色'),
--     ('1898765432100002016', 'role:assign_perm', '分配权限',   'api', 16, 1, 0, '为角色分配权限'),
--     ('1898765432100002021', 'permission:list',  '权限列表',   'api', 21, 1, 0, '查看权限列表'),
--     ('1898765432100002022', 'permission:read',  '查看权限',   'api', 22, 1, 0, '查看权限详情'),
--     ('1898765432100002023', 'system:all',       '系统管理',   'api', 99, 1, 0, '系统全部权限');

-- =============================================
-- 6. admin 角色拥有全部权限 (cmx_role_permission)
-- =============================================
-- INSERT INTO cmx_role_permission (id, role_id, permission_id) VALUES
--     ('1898765432100003001', '1898765432100001001', '1898765432100002001'),
--     ('1898765432100003002', '1898765432100001001', '1898765432100002002'),
--     ('1898765432100003003', '1898765432100001001', '1898765432100002003'),
--     ('1898765432100003004', '1898765432100001001', '1898765432100002004'),
--     ('1898765432100003005', '1898765432100001001', '1898765432100002005'),
--     ('1898765432100003006', '1898765432100001001', '1898765432100002006'),
--     ('1898765432100003011', '1898765432100001001', '1898765432100002011'),
--     ('1898765432100003012', '1898765432100001001', '1898765432100002012'),
--     ('1898765432100003013', '1898765432100001001', '1898765432100002013'),
--     ('1898765432100003014', '1898765432100001001', '1898765432100002014'),
--     ('1898765432100003015', '1898765432100001001', '1898765432100002015'),
--     ('1898765432100003016', '1898765432100001001', '1898765432100002016'),
--     ('1898765432100003021', '1898765432100001001', '1898765432100002021'),
--     ('1898765432100003022', '1898765432100001001', '1898765432100002022'),
--     ('1898765432100003023', '1898765432100001001', '1898765432100002023');


-- -- 新增权限码（规则管理）
-- INSERT INTO cmx_permission (id, code, name, resource_type, sort_order, status, description) VALUES
--                                                                                                 ('1898765432100002031', 'rule:read',   '查看权限规则', 'api', 31, 1, '查询互斥规则及规则项'),
--                                                                                                 ('1898765432100002032', 'rule:manage', '管理权限规则', 'api', 32, 1, '创建/更新/删除/启用禁用规则及规则项')
--     ON CONFLICT (code) DO NOTHING;
--
-- -- 新增权限码对 admin 角色的批量授权（复用 CTE 逻辑）
-- WITH new_perms AS (
--     SELECT id FROM cmx_permission WHERE code IN ('rule:read', 'rule:manage')
-- )
-- INSERT INTO cmx_role_permission (id, role_id, permission_id)
-- SELECT CONCAT('1898765432100003', LPAD(ROW_NUMBER() OVER ()::TEXT, 4, '0')),
--        '1898765432100001001',
--        id
-- FROM new_perms
--     ON CONFLICT DO NOTHING;