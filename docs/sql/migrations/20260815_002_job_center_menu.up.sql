-- 异步任务中心（后端任务中心）菜单注册。页面 portal.job.monitor（native_pages 三区工作台，
-- 异步任务中心 M1+M2+M3 全能力）已在 data/native-pages/index.json 注册；本迁移把入口挂进 gl 组，
-- 与流程/报表/规则设计工作台并列。
--
-- 背景：该节点在 data/menu-pages/fi/cmxfico/gl/explorer-menu.json 里有 fi-gl-job-center 定义，
-- 但它是在 gl 那批一次性 seed **之后**才加入源文件的，seed 幂等不再重跑 → 从未进 cmx_menu →
-- 菜单里看不到任务中心。故补此独立迁移（对齐 20260801_002 待办 / 20260815_001 规则的做法）。
--
-- workspace 去掉 model 块（F0 教训：model 块会致 portal 双 content host 抢 state；源文件里带 model，
-- 此处刻意剥离）。parent_id / id_path 用 code='gl' 子查询解析，避免硬编码父 id 跨环境失配。
-- 节点自身 id 固定（gl 号段内未占用值）。幂等：ON CONFLICT (code) WHERE archived=0 DO NOTHING。
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7485938639050227714', 'fi-gl-job-center', '任务中心', 'process', NULL, 36,
  $def${"name":"job-center","caption":"任务中心","type":"workspace-node","icon":"process","workspace":{"id":"job_center","explorer":{"caption":"概览 / 新建 / 过滤","icon":"detail-view","views":[{"id":"job-center-explorer","tabLabel":"任务台","icon":"detail-view","type":"native_pages","native_page":"portal.job.monitor","view":"explorer"}]},"content":{"caption":"任务列表与监控","icon":"process","views":[{"id":"job-center-active","tabLabel":"活跃作业","icon":"process","type":"native_pages","native_page":"portal.job.monitor","view":"active"},{"id":"job-center-history","tabLabel":"历史作业","icon":"history","type":"native_pages","native_page":"portal.job.monitor","view":"history"}]},"property":{"caption":"作业属性","icon":"detail-view","views":[{"id":"job-center-prop","tabLabel":"属性","icon":"detail-view","type":"native_pages","native_page":"portal.job.monitor","view":"property"}]}}}$def$::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-job-center', p.id_path || '/7485938639050227714', 1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
