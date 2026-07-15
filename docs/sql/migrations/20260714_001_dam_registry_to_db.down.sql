-- ============================================================
-- 20260714_001 DAM 注册表迁移回滚
--
-- 回滚动作：
--   1. 删除由 registry.json 转换灌入的小写编码数据
--   2. 恢复旧 FIN/LOG/SAL 大写编码种子
--   3. 恢复 cmx_permission 24 条 GL 权限原 code
--   4. DROP 新增列
--
-- 注意：回滚后 DAM 注册表恢复为文件存储模式（registry.json）
--       需同时 git 回滚后端 store.rs / handlers 代码。
-- ============================================================


-- ============================================================
-- 第 1 部分：删除 registry.json 转换灌入的小写编码数据
-- ============================================================

DELETE FROM cmx_module
WHERE code IN (
    'portal_portal_overview', 'fi_cmxfico_gl', 'fi_sap_gl', 'fi_ebs_gl',
    'fi_yonyou_gl', 'fi_kingdee_gl', 'hr_recruit_candidate',
    'cr_explorer_explorer-menu', 'fi_cmxfico_report'
);

DELETE FROM cmx_application
WHERE code IN (
    'portal_portal', 'fi_cmxfico', 'fi_sap', 'fi_ebs', 'fi_yonyou',
    'fi_kingdee', 'hr_recruit', 'cr_explorer', 'dr_zhili', 'sc_datalake'
);

DELETE FROM cmx_domain
WHERE code IN ('fi', 'hr', 'cr', 'dr', 'sc', 'portal');


-- ============================================================
-- 第 2 部分：恢复旧 FIN/LOG/SAL 大写编码种子
-- ============================================================

-- 2.1 域
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000001', 'FIN', '资金与价值流领域', '企业的记账本，所有业务最终化为货币数字流入此领域', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000002', 'LOG', '物流与供应链领域', '管理实物资产的买入、存放和流转', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000003', 'SAL', '营收与客户领域', '企业的赚钱引擎', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000004', 'MFG', '制造与工程领域', '把原材料变成可以卖出去的成品', 'business', 4, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000005', 'HCM', '组织与人力领域', '企业的社会网络', 'business', 5, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_domain (id, code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000006', 'XAP', '跨应用基座领域', '公共底层领域，为全系统提供主数据和公共服务', 'technical', 6, 1, 0)
ON CONFLICT (id) DO NOTHING;

-- 2.2 应用
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000011', 'FI', 'FIN', '财务会计', '对外报告，满足税务审计合规要求', 'product', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000012', 'CO', 'FIN', '管理会计', '对内分析，算清部门和产品的盈亏', 'product', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000013', 'MM', 'LOG', '物料管理', '采购、库存、发票校验', 'product', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000014', 'EWM', 'LOG', '仓储管理', '管理物理货架与仓库作业', 'product', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000015', 'SD', 'SAL', '销售与分销', '销售订单、交货、开票与定价', 'product', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000016', 'PP', 'MFG', '生产计划', '物料需求运算、车间排程', 'product', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000017', 'QPM', 'MFG', '质量管理与设备维护', '质量检验与设备维护', 'product', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000018', 'HRM', 'HCM', '人力资源管理', '组织、人事、薪资、考勤', 'product', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000019', 'BP', 'XAP', '商业伙伴主数据', '统一管理客户、供应商、联系人', 'platform', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000020', 'MDM', 'XAP', '物料主数据', '物料基本信息与分类，全系统共享', 'platform', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_application (id, code, domain_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000021', 'CA', 'XAP', '跨应用组件', '分类系统、文档管理、权限管理', 'platform', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;

-- 2.3 模块
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000101', 'GL', 'FIN', 'FI', '总账', '总分类账管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000102', 'AR', 'FIN', 'FI', '应收账款', '应收账款管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000103', 'AP', 'FIN', 'FI', '应付账款', '应付账款管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000104', 'AA', 'FIN', 'FI', '固定资产', '固定资产核算', 'business', 4, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000105', 'CCA', 'FIN', 'CO', '成本中心会计', '成本中心核算', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000106', 'PCA', 'FIN', 'CO', '利润中心会计', '利润中心核算', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000107', 'IO', 'FIN', 'CO', '内部订单', '内部订单管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000108', 'PUR', 'LOG', 'MM', '采购管理', '采购流程管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000109', 'INV', 'LOG', 'MM', '库存管理', '库存数量与价值管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000110', 'IV', 'LOG', 'MM', '发票校验', '采购发票校验', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000111', 'INB', 'LOG', 'EWM', '入库管理', '入库流程管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000112', 'OUT', 'LOG', 'EWM', '出库管理', '出库流程管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000113', 'BIN', 'LOG', 'EWM', '货位管理', '仓库货位管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000114', 'SOM', 'SAL', 'SD', '销售订单管理', '销售订单全流程管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000115', 'DLM', 'SAL', 'SD', '交货管理', '交货流程管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000116', 'BLM', 'SAL', 'SD', '开票管理', '开票流程管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000117', 'PRM', 'SAL', 'SD', '定价管理', '定价策略管理', 'business', 4, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000118', 'BOM', 'MFG', 'PP', '物料清单', '物料清单管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000119', 'RTG', 'MFG', 'PP', '工艺路线', '工艺路线管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000120', 'PROD', 'MFG', 'PP', '生产订单', '生产订单管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000121', 'QI', 'MFG', 'QPM', '质量检验', '质量检验管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000122', 'PM', 'MFG', 'QPM', '设备维护', '设备维护管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000123', 'FL', 'MFG', 'QPM', '功能位置', '功能位置管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000124', 'OM', 'HCM', 'HRM', '组织管理', '组织架构管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000125', 'PAM', 'HCM', 'HRM', '人事管理', '人事信息管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000126', 'PAY', 'HCM', 'HRM', '薪资管理', '薪资核算管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000127', 'TA', 'HCM', 'HRM', '考勤管理', '考勤记录管理', 'business', 4, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000128', 'CUS', 'XAP', 'BP', '客户管理', '客户信息管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000129', 'VEN', 'XAP', 'BP', '供应商管理', '供应商信息管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000130', 'CON', 'XAP', 'BP', '联系人管理', '联系人信息管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000131', 'MBI', 'XAP', 'MDM', '物料基本信息', '物料基本信息管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000132', 'MCL', 'XAP', 'MDM', '物料分类', '物料分类管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000133', 'CLS', 'XAP', 'CA', '分类系统', '分类体系管理', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000134', 'DMS', 'XAP', 'CA', '文档管理', '文档存储与管理', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_module (id, code, domain_code, application_code, name, description, type, sort_order, status, archived)
VALUES ('1898765432100000135', 'AUTH', 'XAP', 'CA', '权限管理', '权限配置与管理', 'business', 3, 1, 0)
ON CONFLICT (id) DO NOTHING;


-- ============================================================
-- 第 3 部分：恢复 cmx_permission 24 条 GL 权限原 code
-- ============================================================

UPDATE cmx_permission
SET domain_code = 'FIN',
    app_code    = 'FI',
    module_code = 'GL'
WHERE domain_code = 'fi'
  AND app_code    = 'fi_cmxfico'
  AND module_code = 'fi_cmxfico_gl';


-- ============================================================
-- 第 4 部分：DROP 新增列
-- ============================================================

ALTER TABLE cmx_domain DROP COLUMN IF EXISTS icon;
ALTER TABLE cmx_domain DROP COLUMN IF EXISTS title;

ALTER TABLE cmx_application DROP COLUMN IF EXISTS icon;
ALTER TABLE cmx_application DROP COLUMN IF EXISTS title;

ALTER TABLE cmx_module DROP COLUMN IF EXISTS icon;
ALTER TABLE cmx_module DROP COLUMN IF EXISTS title;
ALTER TABLE cmx_module DROP COLUMN IF EXISTS resource_root;
ALTER TABLE cmx_module DROP COLUMN IF EXISTS manifest_path;
ALTER TABLE cmx_module DROP COLUMN IF EXISTS theme;
ALTER TABLE cmx_module DROP COLUMN IF EXISTS theme_color;
