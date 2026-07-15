-- ============================================================
-- 20260714_001 DAM 注册表迁移：registry.json → 数据库
--
-- 将 data/dam-registry/registry.json 的域/应用/模块主数据
-- 迁入 cmx_domain / cmx_application / cmx_module 三表，
-- 废弃文件存储方式。
--
-- 背景说明：
--   - 旧种子使用大写编码 (FIN/LOG/SAL/FI/CO/GL...)，与 registry.json 的小写
--     编码 (fi/hr/cr/cmxfico...) 无交集，本迁移以 registry 数据替换旧种子。
--   - module 的 resource_root/manifest_path/icon/title/theme/theme_color 为
--     新增列（DAM 资产定位用），aliases 复用既有 tags 列。
--   - code 生成规则：domain 原样；app = {domain}_{id}；module = {domain}_{app}_{id}。
--     id = code（主键与业务键同值）。
--
-- 注意：此迁移为幂等设计，可重复执行。
-- ============================================================


-- ============================================================
-- 第 1 部分：DDL 加列
-- ============================================================

-- 1.1 cmx_domain 加 icon / title
ALTER TABLE cmx_domain ADD COLUMN IF NOT EXISTS icon  VARCHAR(100);
ALTER TABLE cmx_domain ADD COLUMN IF NOT EXISTS title VARCHAR(200);
COMMENT ON COLUMN cmx_domain.icon  IS '域图标名（UI5 图标标识）';
COMMENT ON COLUMN cmx_domain.title IS '域英文标题/副标题';

-- 1.2 cmx_application 加 icon / title
ALTER TABLE cmx_application ADD COLUMN IF NOT EXISTS icon  VARCHAR(100);
ALTER TABLE cmx_application ADD COLUMN IF NOT EXISTS title VARCHAR(200);
COMMENT ON COLUMN cmx_application.icon  IS '应用图标名（UI5 图标标识）';
COMMENT ON COLUMN cmx_application.title IS '应用英文标题/副标题';

-- 1.3 cmx_module 加 icon / title / resource_root / manifest_path / theme / theme_color
ALTER TABLE cmx_module ADD COLUMN IF NOT EXISTS icon          VARCHAR(100);
ALTER TABLE cmx_module ADD COLUMN IF NOT EXISTS title         VARCHAR(200);
ALTER TABLE cmx_module ADD COLUMN IF NOT EXISTS resource_root VARCHAR(255);
ALTER TABLE cmx_module ADD COLUMN IF NOT EXISTS manifest_path VARCHAR(500);
ALTER TABLE cmx_module ADD COLUMN IF NOT EXISTS theme         VARCHAR(100);
ALTER TABLE cmx_module ADD COLUMN IF NOT EXISTS theme_color   VARCHAR(50);
COMMENT ON COLUMN cmx_module.icon          IS '模块图标名（UI5 图标标识）';
COMMENT ON COLUMN cmx_module.title         IS '模块英文标题/副标题';
COMMENT ON COLUMN cmx_module.resource_root IS '模块资源目录相对路径（相对 data/ 根），格式 domain/application/module';
COMMENT ON COLUMN cmx_module.manifest_path IS '模块清单文件相对路径，格式 modules/<d>/<a>/<m>/module.json';
COMMENT ON COLUMN cmx_module.theme         IS '模块主题名';
COMMENT ON COLUMN cmx_module.theme_color   IS '模块主题色（十六进制或色名）';


-- ============================================================
-- 第 2 部分：清理旧 FIN/LOG/SAL 大写编码种子
--
-- 这些种子与 registry.json 的小写编码无交集，予以清除。
-- cmx_*_code 关联均为逻辑外键（无 DB FK 约束），DELETE 不会报错。
-- 唯一子表依赖：cmx_permission 的 24 条 GL 权限（见第 4 部分改写）。
--
-- ⚠️ 线上库风险：若 cmx_form / cmx_menu / cmx_module_current_version
--    等含 NOT NULL domain/application/module code 列的表已有用大写 code
--    写入的业务数据，删除前请先核查，避免逻辑悬空。
-- ============================================================

-- 2.1 删除旧模块种子
DELETE FROM cmx_module WHERE code IN (
    'GL','AR','AP','AA','CCA','PCA','IO','PUR','INV','IV','INB','OUT','BIN',
    'SOM','DLM','BLM','PRM','BOM','RTG','PROD','QI','PM','FL','OM','PAM',
    'PAY','TA','CUS','VEN','CON','MBI','MCL','CLS','DMS','AUTH'
);

-- 2.2 删除旧应用种子
DELETE FROM cmx_application WHERE code IN (
    'FI','CO','MM','EWM','SD','PP','QPM','HRM','BP','MDM','CA'
);

-- 2.3 删除旧域种子
DELETE FROM cmx_domain WHERE code IN ('FIN','LOG','SAL','MFG','HCM','XAP');


-- ============================================================
-- 第 3 部分：从 registry.json 转换的 INSERT
--
-- 数据来源：data/dam-registry/registry.json（5 域 / 10 应用 / 9 模块）
-- 数据修正：补录 portal 域（applications/modules 中引用但 domains 缺失）
-- code 规则：domain=原 id；app={domain}_{id}；module={domain}_{app}_{id}
-- id = code；status: active→1, disabled→0
-- ============================================================

-- 3.1 域数据（6 条，含补录的 portal）
INSERT INTO cmx_domain (id, code, name, title, icon, description, status, archived, sort_order)
VALUES
    ('fi',     'fi',     '财务资源管理',   'Finance',                'expense-report', '财务、会计核算、总账及 ERP 凭证相关资源。', 1, 0, 1),
    ('hr',     'hr',     '人力资源管理',   'Human Resources',         'employee',       '招聘与候选人等人力资源资源。',           1, 0, 2),
    ('cr',     'cr',     '客户资源管理',   'Collaboration Resources', 'collaborate',    '协作资源、示例页面与门户扩展资源。',       1, 0, 3),
    ('dr',     'dr',     '数据资源管理',   'data Resources',          'database',       '',                                       1, 0, 4),
    ('sc',     'sc',     '生产资源管理',   '生产资源管理',             'machine',        '',                                       1, 0, 5),
    -- 补录：applications/modules 引用 domain=portal 但 registry domains 缺失
    ('portal', 'portal', '门户',           'Portal',                  'home',           '门户平台域。',                            0, 0, 0)
ON CONFLICT (id) DO UPDATE SET
    code = EXCLUDED.code, name = EXCLUDED.name, title = EXCLUDED.title,
    icon = EXCLUDED.icon, description = EXCLUDED.description,
    status = EXCLUDED.status, archived = EXCLUDED.archived, sort_order = EXCLUDED.sort_order;

-- 3.2 应用数据（10 条，code = {domain}_{id}）
INSERT INTO cmx_application (id, code, domain_code, name, title, icon, description, status, archived, sort_order)
VALUES
    ('portal_portal',  'portal_portal',  'portal', '门户',         'Portal',                'home',                          '门户平台应用。',           1, 0, 0),
    ('fi_cmxfico',     'fi_cmxfico',     'fi',     '会计核算',     'CMX FICO',              'expense-report',                '自研会计核算应用。',       1, 0, 1),
    ('fi_sap',         'fi_sap',         'fi',     'SAP',          'SAP FI',                'business-objects-experience',   'SAP 总账样例资源。',       1, 0, 2),
    ('fi_ebs',         'fi_ebs',         'fi',     'Oracle EBS',   'Oracle EBS',            'decrease-line-height',          'Oracle EBS 总账样例资源。', 1, 0, 3),
    ('fi_yonyou',      'fi_yonyou',      'fi',     '用友',         'Yonyou',                'developer-settings',            '用友总账样例资源。',       1, 0, 4),
    ('fi_kingdee',     'fi_kingdee',     'fi',     '金蝶',         'Kingdee',               'electronic-medical-record',     '金蝶总账样例资源。',       1, 0, 5),
    ('hr_recruit',     'hr_recruit',     'hr',     '招聘',         'Recruitment',           'add-employee',                  '招聘服务目录。',           1, 0, 6),
    ('cr_explorer',    'cr_explorer',    'cr',     '资源浏览',     'Explorer',              'documents',                     '资源浏览与菜单页面示例。', 1, 0, 7),
    ('dr_zhili',       'dr_zhili',       'dr',     '数据中台',     '',                      'display-more',                  '',                        1, 0, 8),
    ('sc_datalake',    'sc_datalake',    'sc',     '数据湖',       '',                      'background',                    '',                        1, 0, 9)
ON CONFLICT (id) DO UPDATE SET
    code = EXCLUDED.code, domain_code = EXCLUDED.domain_code, name = EXCLUDED.name,
    title = EXCLUDED.title, icon = EXCLUDED.icon, description = EXCLUDED.description,
    status = EXCLUDED.status, archived = EXCLUDED.archived, sort_order = EXCLUDED.sort_order;

-- 3.3 模块数据（9 条，code = {domain}_{app}_{id}）
-- aliases → tags（JSON 数组字符串）；resource_root/manifest_path 原样；title/icon 原样
INSERT INTO cmx_module
    (id, code, domain_code, application_code, name, title, icon, description,
     tags, resource_root, manifest_path, status, archived, sort_order)
VALUES
    ('portal_portal_overview', 'portal_portal_overview', 'portal', 'portal_portal',
     '平台总览', '门户平台总览', 'home', '门户平台使用入门与总览帮助。',
     '[]', 'portal/portal/overview', 'modules/portal/portal/overview/module.json', 1, 0, 0),

    ('fi_cmxfico_gl', 'fi_cmxfico_gl', 'fi', 'fi_cmxfico',
     '总账', '会计核算管理 / 总账', 'activity-items', '会计核算管理、ERP 凭证、总账科目、辅助核算等资源。',
     '["fi.cmxfico.gl","cmxfico.gl"]', 'fi/cmxfico/gl', 'modules/fi/cmxfico/gl/module.json', 1, 0, 1),

    ('fi_sap_gl', 'fi_sap_gl', 'fi', 'fi_sap',
     'SAP 总账', 'SAP 总账样例', 'business-objects-experience', 'SAP FI 总账样例。',
     '[]', 'fi/sap/gl', 'modules/fi/sap/gl/module.json', 1, 0, 2),

    ('fi_ebs_gl', 'fi_ebs_gl', 'fi', 'fi_ebs',
     'Oracle EBS 总账', 'Oracle EBS 总账样例', 'database', 'Oracle EBS 总账样例。',
     '[]', 'fi/ebs/gl', 'modules/fi/ebs/gl/module.json', 1, 0, 3),

    ('fi_yonyou_gl', 'fi_yonyou_gl', 'fi', 'fi_yonyou',
     '用友总账', '用友总账样例', 'database', '用友总账样例。',
     '[]', 'fi/yonyou/gl', 'modules/fi/yonyou/gl/module.json', 1, 0, 4),

    ('fi_kingdee_gl', 'fi_kingdee_gl', 'fi', 'fi_kingdee',
     '金蝶总账', '金蝶总账样例', 'database', '金蝶总账样例。',
     '[]', 'fi/kingdee/gl', 'modules/fi/kingdee/gl/module.json', 1, 0, 5),

    ('hr_recruit_candidate', 'hr_recruit_candidate', 'hr', 'hr_recruit',
     '候选人', '招聘候选人服务目录', 'employee', '候选人服务目录。',
     '[]', 'hr/recruit/candidate', 'modules/hr/recruit/candidate/module.json', 1, 0, 6),

    ('cr_explorer_explorer-menu', 'cr_explorer_explorer-menu', 'cr', 'cr_explorer',
     'Explorer 菜单', 'CR Explorer 菜单页面示例', 'documents', 'CR Explorer 菜单页面示例。',
     '[]', 'cr/explorer/explorer-menu', 'modules/cr/explorer/explorer-menu/module.json', 1, 0, 7),

    ('fi_cmxfico_report', 'fi_cmxfico_report', 'fi', 'fi_cmxfico',
     '报表', '报表', 'excel-attachment', '',
     '[]', 'fi/cmxfico/report', 'modules/fi/cmxfico/report/module.json', 1, 0, 8)
ON CONFLICT (id) DO UPDATE SET
    code = EXCLUDED.code, domain_code = EXCLUDED.domain_code, application_code = EXCLUDED.application_code,
    name = EXCLUDED.name, title = EXCLUDED.title, icon = EXCLUDED.icon, description = EXCLUDED.description,
    tags = EXCLUDED.tags, resource_root = EXCLUDED.resource_root, manifest_path = EXCLUDED.manifest_path,
    status = EXCLUDED.status, archived = EXCLUDED.archived, sort_order = EXCLUDED.sort_order;


-- ============================================================
-- 第 4 部分：cmx_permission 24 条 GL 权限 code 改写
--
-- 原 domain_code='FIN', app_code='FI', module_code='GL'
-- 改为小写：domain_code='fi', app_code='fi_cmxfico', module_code='fi_cmxfico_gl'
-- （旧 module GL 对应新 module fi_cmxfico_gl，即会计核算总账）
-- ============================================================

-- UPDATE cmx_permission
-- SET domain_code = 'fi',
--     app_code    = 'fi_cmxfico',
--     module_code = 'fi_cmxfico_gl'
-- WHERE domain_code = 'FIN'
--   AND app_code    = 'FI'
--   AND module_code = 'GL';
