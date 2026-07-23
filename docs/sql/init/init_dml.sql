-- =============================================
-- cmx-container 初始数据 (DML)
-- 包含：域、应用、模块的初始数据
-- =============================================

-- =============================================
-- 1. 域数据
-- 来源：data/dam-registry/registry.json（含补录 portal 域）
-- code 规则：domain = 原始 id；id = code
-- =============================================
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES ('fi',     'fi',     '财务资源管理',   'Finance',                'expense-report', '财务、会计核算、总账及 ERP 凭证相关资源。', 1, 0, 1);
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES ('hr',     'hr',     '人力资源管理',   'Human Resources',         'employee',       '招聘与候选人等人力资源资源。',           1, 0, 2);
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES ('cr',     'cr',     '客户资源管理',   'Collaboration Resources', 'collaborate',    '协作资源、示例页面与门户扩展资源。',       1, 0, 3);
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES ('dr',     'dr',     '数据资源管理',   'data Resources',          'database',       '',                                       1, 0, 4);
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES ('sc',     'sc',     '生产资源管理',   '生产资源管理',             'machine',        '',                                       1, 0, 5);
-- 补录：applications/modules 引用 domain=portal 但 registry domains 缺失
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES ('portal', 'portal', '门户',           'Portal',                  'home',           '门户平台域。',                            1, 0, 0);

-- =============================================
-- 2. 应用数据
-- 来源：data/dam-registry/registry.json
-- code 规则：code = 原始短 id（纯净短码，如 cmxfico）；
--            id = {domain}_{短id}（物理主键，保证全局唯一，ON CONFLICT 幂等）
-- =============================================
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('portal_portal',  'portal',  'portal', '门户',         'Portal',                'home',                          '门户平台应用。',           1, 0, 0);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('fi_cmxfico',     'cmxfico', 'fi',     '会计核算',     'CMX FICO',              'expense-report',                '自研会计核算应用。',       1, 0, 1);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('fi_sap',         'sap',     'fi',     'SAP',          'SAP FI',                'business-objects-experience',   'SAP 总账样例资源。',       1, 0, 2);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('fi_ebs',         'ebs',     'fi',     'Oracle EBS',   'Oracle EBS',            'decrease-line-height',          'Oracle EBS 总账样例资源。', 1, 0, 3);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('fi_yonyou',      'yonyou',  'fi',     '用友',         'Yonyou',                'developer-settings',            '用友总账样例资源。',       1, 0, 4);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('fi_kingdee',     'kingdee', 'fi',     '金蝶',         'Kingdee',               'electronic-medical-record',     '金蝶总账样例资源。',       1, 0, 5);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('hr_recruit',     'recruit', 'hr',     '招聘',         'Recruitment',           'add-employee',                  '招聘服务目录。',           1, 0, 6);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('cr_explorer',    'explorer','cr',     '资源浏览',     'Explorer',              'documents',                     '资源浏览与菜单页面示例。', 1, 0, 7);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('dr_zhili',       'zhili',   'dr',     '数据中台',     '',                      'display-more',                  '',                        1, 0, 8);
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('sc_datalake',    'datalake','sc',     '数据湖',       '',                      'background',                    '',                        1, 0, 9);

-- =============================================
-- 3. 模块数据
-- 来源：data/dam-registry/registry.json
-- code 规则：code = 原始短 id（纯净短码，如 gl）；
--            id = {domain}_{app}_{短id}（物理主键，保证全局唯一，ON CONFLICT 幂等）
-- application_code 存纯净短码（与 cmx_application.code 对齐）
-- aliases → tags（JSON 数组字符串）
-- =============================================
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('portal_portal_overview', 'overview', 'portal', 'portal', '平台总览', '门户平台总览', 'home', '门户平台使用入门与总览帮助。', '[]', 'portal/portal/overview', 'modules/portal/portal/overview/module.json', 1, 0, 0);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_cmxfico_gl', 'gl', 'fi', 'cmxfico', '总账', '会计核算管理 / 总账', 'activity-items', '会计核算管理、ERP 凭证、总账科目、辅助核算等资源。', '["fi.cmxfico.gl","cmxfico.gl"]', 'fi/cmxfico/gl', 'modules/fi/cmxfico/gl/module.json', 1, 0, 1);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_sap_gl', 'sap_gl', 'fi', 'sap', 'SAP 总账', 'SAP 总账样例', 'business-objects-experience', 'SAP FI 总账样例。', '[]', 'fi/sap/sap_gl', 'modules/fi/sap/sap_gl/module.json', 1, 0, 2);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_ebs_gl', 'ebs_gl', 'fi', 'ebs', 'Oracle EBS 总账', 'Oracle EBS 总账样例', 'database', 'Oracle EBS 总账样例。', '[]', 'fi/ebs/ebs_gl', 'modules/fi/ebs/ebs_gl/module.json', 1, 0, 3);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_yonyou_gl', 'yonyou_gl', 'fi', 'yonyou', '用友总账', '用友总账样例', 'database', '用友总账样例。', '[]', 'fi/yonyou/yonyou_gl', 'modules/fi/yonyou/yonyou_gl/module.json', 1, 0, 4);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_kingdee_gl', 'kingdee_gl', 'fi', 'kingdee', '金蝶总账', '金蝶总账样例', 'database', '金蝶总账样例。', '[]', 'fi/kingdee/kingdee_gl', 'modules/fi/kingdee/kingdee_gl/module.json', 1, 0, 5);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('hr_recruit_candidate', 'candidate', 'hr', 'recruit', '候选人', '招聘候选人服务目录', 'employee', '候选人服务目录。', '[]', 'hr/recruit/candidate', 'modules/hr/recruit/candidate/module.json', 1, 0, 6);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('cr_explorer_explorer-menu', 'explorer-menu', 'cr', 'explorer', 'Explorer 菜单', 'CR Explorer 菜单页面示例', 'documents', 'CR Explorer 菜单页面示例。', '[]', 'cr/explorer/explorer-menu', 'modules/cr/explorer/explorer-menu/module.json', 1, 0, 7);
INSERT INTO cmx_module (id, code, domain_code, application_code, name, title, icon, description, tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_cmxfico_report', 'report', 'fi', 'cmxfico', '报表', '报表', 'excel-attachment', '', '[]', 'fi/cmxfico/report', 'modules/fi/cmxfico/report/module.json', 1, 0, 8);

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
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667162255360', 'gl:account', '科目管理', 'menu', null, 3, '会计科目体系维护', 1, 0, '2026-06-25 09:53:36.798272', '2026-06-25 09:53:36.798272', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:account', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676112900096', 'gl:account:add', '科目新增', 'button', '7475848667162255360', 1, '新增会计科目', 1, 0, '2026-06-25 09:53:38.915084', '2026-06-25 09:53:38.915084', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:add', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848677895479296', 'gl:account:delete', '科目删除', 'button', '7475848667162255360', 3, '删除末级科目', 1, 0, '2026-06-25 09:53:39.343854', '2026-06-25 09:53:39.343854', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:delete', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676989509632', 'gl:account:edit', '科目编辑', 'button', '7475848667162255360', 2, '修改科目信息', 1, 0, '2026-06-25 09:53:39.129204', '2026-06-25 09:53:39.129204', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:edit', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848679703224320', 'gl:account:export', '科目导出', 'button', '7475848667162255360', 5, '导出科目体系', 1, 0, '2026-06-25 09:53:39.776420', '2026-06-25 09:53:39.776420', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:export', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848678809837568', 'gl:account:query', '科目查询', 'api', '7475848667162255360', 4, '查询科目树形列表', 1, 0, '2026-06-25 09:53:39.562303', '2026-06-25 09:53:39.562303', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:account', '/gl:account/gl:account:query', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848665702637568', 'gl:dashboard', '总账仪表盘', 'menu', null, 1, '总账模块概览看板', 1, 0, '2026-06-25 09:53:36.449545', '2026-06-25 09:53:36.449545', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:dashboard', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670211514368', 'gl:dashboard:refresh', '数据刷新', 'button', '7475848665702637568', 2, '手动刷新仪表盘统计', 1, 0, '2026-06-25 09:53:37.499114', '2026-06-25 09:53:37.499114', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:refresh', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848669469122560', 'gl:dashboard:view', '仪表盘查看', 'api', '7475848665702637568', 1, '查看总账仪表盘数据', 1, 0, '2026-06-25 09:53:37.325539', '2026-06-25 09:53:37.325539', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:view', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667845926912', 'gl:period', '期末处理', 'menu', null, 4, '期末结账与损益结转', 1, 0, '2026-06-25 09:53:36.957909', '2026-06-25 09:53:36.957909', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:period', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848680584028160', 'gl:period:close', '期末结账', 'button', '7475848667845926912', 1, '对当前会计期间结账', 1, 0, '2026-06-25 09:53:39.985243', '2026-06-25 09:53:39.985243', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:period', '/gl:period/gl:period:close', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848682249166848', 'gl:period:settle', '损益结转', 'button', '7475848667845926912', 3, '结转本期损益至本年利润', 1, 0, '2026-06-25 09:53:40.383092', '2026-06-25 09:53:40.383092', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:period', '/gl:period/gl:period:settle', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848681427083264', 'gl:period:unclose', '反结账', 'button', '7475848667845926912', 2, '撤销已结账期间', 1, 0, '2026-06-25 09:53:40.186302', '2026-06-25 09:53:40.186302', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:period', '/gl:period/gl:period:unclose', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848668558958592', 'gl:report', '报表中心', 'menu', null, 5, '总账报表查询与导出', 1, 0, '2026-06-25 09:53:37.122063', '2026-06-25 09:53:37.122063', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:report', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848683444543488', 'gl:report:balance', '资产负债表', 'api', '7475848668558958592', 1, '生成资产负债表', 1, 0, '2026-06-25 09:53:40.665266', '2026-06-25 09:53:40.665266', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:report', '/gl:report/gl:report:balance', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848685168402432', 'gl:report:cashflow', '现金流量表', 'api', '7475848668558958592', 3, '生成现金流量表', 1, 0, '2026-06-25 09:53:41.078737', '2026-06-25 09:53:41.078737', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:report', '/gl:report/gl:report:cashflow', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848684400844800', 'gl:report:income', '利润表', 'api', '7475848668558958592', 2, '生成利润表', 1, 0, '2026-06-25 09:53:40.893176', '2026-06-25 09:53:40.893176', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:report', '/gl:report/gl:report:income', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848666486972416', 'gl:voucher', '凭证管理', 'menu', null, 2, '会计凭证全生命周期管理', 1, 0, '2026-06-25 09:53:36.636949', '2026-06-25 09:53:36.636949', null, null, null, null, 'fi', 'cmxfico', 'gl', null, null, '/gl:voucher', 0, 1);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670991654912', 'gl:voucher:add', '凭证录入', 'button', '7475848666486972416', 1, '新增会计凭证', 1, 0, '2026-06-25 09:53:37.676977', '2026-06-25 09:53:37.676977', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:add', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848673445322752', 'gl:voucher:audit', '凭证审核', 'button', '7475848666486972416', 4, '审核/反审核凭证', 1, 0, '2026-06-25 09:53:38.281608', '2026-06-25 09:53:38.281608', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:audit', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848675001409536', 'gl:voucher:delete', '凭证删除', 'button', '7475848666486972416', 6, '删除作废凭证', 1, 0, '2026-06-25 09:53:38.655424', '2026-06-25 09:53:38.655424', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:delete', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848672686153728', 'gl:voucher:edit', '凭证修改', 'button', '7475848666486972416', 3, '修改未审核凭证', 1, 0, '2026-06-25 09:53:38.088995', '2026-06-25 09:53:38.088995', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:edit', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848674095439872', 'gl:voucher:post', '凭证过账', 'button', '7475848666486972416', 5, '将审核凭证过账到账簿', 1, 0, '2026-06-25 09:53:38.439356', '2026-06-25 09:53:38.439356', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:post', 1, 2);
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848671801155584', 'gl:voucher:query', '凭证查询', 'api', '7475848666486972416', 2, '按条件查询凭证列表', 1, 0, '2026-06-25 09:53:37.878384', '2026-06-25 09:53:37.878384', null, null, null, null, 'fi', 'cmxfico', 'gl', null, 'gl:voucher', '/gl:voucher/gl:voucher:query', 1, 2);





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