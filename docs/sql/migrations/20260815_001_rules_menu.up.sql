-- =============================================================================
-- 决策规则引擎菜单注册（F2b）：两个"列表工作台"进菜单（入口），挂 fi/cmxfico/gl 组下
-- （与 flow 设计工作台 fi-gl-flow-design-workbench / report 设计工作台 fi-gl-rpt-design-workbench 并列）。
--
-- 一芯双壳：definition JSONB 内嵌 workspace-node，explorer/content/property 三区各引用同一 native_page
-- （portal.rules.*）、不同 view。页面源码在独立微服务 cmx-rulesengine/web/ui-native/，门户按
-- [center_client.urls].rules 经 RulesProxyModule 的页面反代（is_rules_owned_page="portal.rules.*"）取页。
--
-- 两个"干活的页"（portal.rules.designer / portal.rules.simulator，多实例）**不进菜单**，由列表页
-- openWorkNode 动态开成 Tab（F3/F4 落地）。
--
-- parent_id / id_path 用 code='gl' 子查询解析（跨环境 id 不同，避免硬编码父 id 失配）。
-- 节点 id 固定（新号段，未占用）。幂等：ON CONFLICT (code) WHERE archived=0 DO NOTHING。
-- =============================================================================

-- 1) 决策集设计工作台
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7490000000000000101', 'fi-gl-rules-design-workbench', '决策集设计工作台', 'table-view', NULL, 33,
  '{"caption":"决策集设计工作台","type":"workspace-node","name":"rules-design-workbench","workspace":{"id":"rules_design_workbench","explorer":{"caption":"决策集","icon":"tree","views":[{"id":"rules-design-explorer","tabLabel":"决策集","icon":"tree","type":"native_pages","native_page":"portal.rules.design-workbench","view":"explorer"}]},"content":{"caption":"决策表","icon":"table-view","views":[{"id":"rules-design-content","tabLabel":"决策表","icon":"table-view","type":"native_pages","native_page":"portal.rules.design-workbench","view":"content"}]},"property":{"caption":"完整性","icon":"detail-view","views":[{"id":"rules-design-prop","tabLabel":"完整性","icon":"detail-view","type":"native_pages","native_page":"portal.rules.design-workbench","view":"property"}]}}}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-rules-design-workbench', p.id_path || '/7490000000000000101',
  1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

-- 2) 决策应用工作台
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7490000000000000102', 'fi-gl-rules-sim-workbench', '决策应用工作台', 'play', NULL, 34,
  '{"caption":"决策应用工作台","type":"workspace-node","name":"rules-sim-workbench","workspace":{"id":"rules_sim_workbench","explorer":{"caption":"决策集","icon":"tree","views":[{"id":"rules-sim-explorer","tabLabel":"决策集","icon":"tree","type":"native_pages","native_page":"portal.rules.sim-workbench","view":"explorer"}]},"content":{"caption":"求值","icon":"play","views":[{"id":"rules-sim-content","tabLabel":"求值","icon":"play","type":"native_pages","native_page":"portal.rules.sim-workbench","view":"content"}]},"property":{"caption":"决策轨迹","icon":"detail-view","views":[{"id":"rules-sim-prop","tabLabel":"trace","icon":"detail-view","type":"native_pages","native_page":"portal.rules.sim-workbench","view":"property"}]}}}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-rules-sim-workbench', p.id_path || '/7490000000000000102',
  1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

-- 3) 决策审计中心（F5）
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7490000000000000103', 'fi-gl-rules-logs', '决策审计中心', 'history', NULL, 35,
  '{"caption":"决策审计中心","type":"workspace-node","name":"rules-logs","workspace":{"id":"rules_logs","explorer":{"caption":"决策集","icon":"tree","views":[{"id":"rules-logs-explorer","tabLabel":"决策集","icon":"tree","type":"native_pages","native_page":"portal.rules.logs","view":"explorer"}]},"content":{"caption":"决策日志","icon":"history","views":[{"id":"rules-logs-content","tabLabel":"日志","icon":"history","type":"native_pages","native_page":"portal.rules.logs","view":"content"}]},"property":{"caption":"归因","icon":"detail-view","views":[{"id":"rules-logs-prop","tabLabel":"trace","icon":"detail-view","type":"native_pages","native_page":"portal.rules.logs","view":"property"}]}}}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-rules-logs', p.id_path || '/7490000000000000103',
  1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
