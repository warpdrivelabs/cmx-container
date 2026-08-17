-- cmx-flow 待办中心菜单注册（F3；对标流程设计工作台 fi-gl-flow-design-workbench）。
-- workspace 无 model 块（F0 教训：model 块会致 portal 双 content host 抢 state）。
--
-- parent_id / id_path 用 code='gl' 子查询解析（跨环境 gl 雪花 id 每次 seed 不同，硬编码父 id
-- 会在 gl 重新 seed 后失配 → 待办中心成孤儿从菜单树消失）。对齐 20260815_001_rules_menu 写法。
-- 节点自身 id 固定（原值，未占用）。幂等：ON CONFLICT (code) WHERE archived=0 DO NOTHING。
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
SELECT '7485938639050227713', 'fi-gl-flow-todo-center', '待办中心', 'task', NULL, 32,
  '{"caption": "待办中心", "workspace": {"id": "flow_todo_center", "explorer": {"caption": "待办分类", "icon": "tree", "views": [{"id": "flow-todo-center-explorer", "tabLabel": "分类", "icon": "tree", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "explorer"}]}, "content": {"caption": "待办中心", "icon": "task", "views": [{"id": "flow-todo-center-content", "tabLabel": "待办", "icon": "task", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "content"}]}, "property": {"caption": "流程轨迹", "icon": "detail-view", "views": [{"id": "flow-todo-center-prop", "tabLabel": "轨迹", "icon": "detail-view", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "property"}]}}, "type": "workspace-node", "name": "flow-todo-center"}'::jsonb,
  'fi', 'cmxfico', 'gl',
  p.id, 'gl', 2, 1,
  '/gl/fi-gl-flow-todo-center', p.id_path || '/7485938639050227713', 1, 1, 0, 0, now(), now()
FROM cmx_menu p WHERE p.code = 'gl' AND p.archived = 0 LIMIT 1
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
