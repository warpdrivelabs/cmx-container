
delete from cmx_application ;
delete from cmx_module ;
delete from cmx_domain;


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
VALUES ('portal', 'portal', '门户',           'Portal',                  'home',           '门户平台域。',                            0, 0, 8);
-- MDM 独立新域（basic/dataplatform/mdm）：主数据管理平台，只存 published，变更走 CR 单据
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES ('basic', 'basic', '基础主数据',     'Master Data',             'database',       '企业主数据管理平台（MDM）：主数据(cm_*)只存 published，变更请求(cv_*)走审批激活。', 1, 0, 9);


INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('portal_portal',  'portal',  'portal', '门户',         'Portal',                'home',                          '门户平台应用。',           0, 0, 8);
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
-- MDM 主数据管理平台应用（basic 域）
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES ('basic_dataplatform', 'dataplatform', 'basic', '数据平台', 'Data Platform', 'database', '主数据管理平台应用：含主数据建模、激活器、变更请求、匹配合并、分发。', 1, 0, 10);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('portal_portal_overview', 'overview', 'portal', 'portal',
 '平台总览', '门户平台总览', 'home', '门户平台使用入门与总览帮助。',
 '[]', 'portal/portal/overview', 'modules/portal/portal/overview/module.json', 0, 0, 8);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_cmxfico_gl', 'gl', 'fi', 'cmxfico',
 '总账', '会计核算管理 / 总账', 'activity-items', '会计核算管理、ERP 凭证、总账科目、辅助核算等资源。',
 '["fi.cmxfico.gl","cmxfico.gl"]', 'fi/cmxfico/gl', 'modules/fi/cmxfico/gl/module.json', 1, 0, 1);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_sap_gl', 'sap_gl', 'fi', 'sap',
 'SAP 总账', 'SAP 总账样例', 'business-objects-experience', 'SAP FI 总账样例。',
 '[]', 'fi/sap/sap_gl', 'modules/fi/sap/sap_gl/module.json', 1, 0, 2);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_ebs_gl', 'ebs_gl', 'fi', 'ebs',
 'Oracle EBS 总账', 'Oracle EBS 总账样例', 'database', 'Oracle EBS 总账样例。',
 '[]', 'fi/ebs/ebs_gl', 'modules/fi/ebs/ebs_gl/module.json', 1, 0, 3);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_yonyou_gl', 'yonyou_gl', 'fi', 'yonyou',
 '用友总账', '用友总账样例', 'database', '用友总账样例。',
 '[]', 'fi/yonyou/yonyou_gl', 'modules/fi/yonyou/yonyou_gl/module.json', 1, 0, 4);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_kingdee_gl', 'kingdee_gl', 'fi', 'kingdee',
 '金蝶总账', '金蝶总账样例', 'database', '金蝶总账样例。',
 '[]', 'fi/kingdee/kingdee_gl', 'modules/fi/kingdee/kingdee_gl/module.json', 1, 0, 5);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('hr_recruit_candidate', 'candidate', 'hr', 'recruit',
 '候选人', '招聘候选人服务目录', 'employee', '候选人服务目录。',
 '[]', 'hr/recruit/candidate', 'modules/hr/recruit/candidate/module.json', 1, 0, 6);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('cr_explorer_explorer-menu', 'explorer-menu', 'cr', 'explorer',
 'Explorer 菜单', 'CR Explorer 菜单页面示例', 'documents', 'CR Explorer 菜单页面示例。',
 '[]', 'cr/explorer/explorer-menu', 'modules/cr/explorer/explorer-menu/module.json', 1, 0, 7);

INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('fi_cmxfico_report', 'report', 'fi', 'cmxfico',
 '报表', '报表', 'excel-attachment', '',
 '[]', 'fi/cmxfico/report', 'modules/fi/cmxfico/report/module.json', 1, 0, 8);

-- MDM 主数据管理模块（basic/dataplatform/mdm）：对应 data/meta/definitions/basic/dataplatform/mdm/
INSERT INTO cmx_module
(id, code, domain_code, application_code, name, title, icon, description,
 tags, resource_root, manifest_path, status, archived, sort_order)
VALUES ('basic_dataplatform_mdm', 'mdm', 'basic', 'dataplatform',
 '主数据', '企业主数据管理', 'database', '主数据(cm_*)只存 published；变更请求(cv_*)走审批激活；含激活映射配置、匹配合并、分发订阅。',
 '["basic.dataplatform.mdm","mdm"]', 'basic/dataplatform/mdm', 'modules/basic/dataplatform/mdm/module.json', 1, 0, 9);
