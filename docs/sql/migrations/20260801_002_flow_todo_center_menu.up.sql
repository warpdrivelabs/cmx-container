-- cmx-flow 待办中心菜单注册（F3；对标流程设计工作台 fi-gl-flow-design-workbench）。
-- 同源已加入 data/menu-pages/fi/cmxfico/gl/explorer-menu.json，menu-generator 重跑不丢。
-- workspace 无 model 块（F0 教训：model 块会致 portal 双 content host 抢 state）。
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time)
VALUES ('7485938639050227713', 'fi-gl-flow-todo-center', '待办中心', 'task', NULL, 32,
  '{"caption": "待办中心", "workspace": {"id": "flow_todo_center", "explorer": {"caption": "待办分类", "icon": "tree", "views": [{"id": "flow-todo-center-explorer", "tabLabel": "分类", "icon": "tree", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "explorer"}]}, "content": {"caption": "待办中心", "icon": "task", "views": [{"id": "flow-todo-center-content", "tabLabel": "待办", "icon": "task", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "content"}]}, "property": {"caption": "流程轨迹", "icon": "detail-view", "views": [{"id": "flow-todo-center-prop", "tabLabel": "轨迹", "icon": "detail-view", "type": "native_pages", "native_page": "portal.flow.todo-center", "view": "property"}]}}, "type": "workspace-node", "name": "flow-todo-center"}'::jsonb,
  'fi', 'cmxfico', 'gl', '7485938633702490112', 'gl', 2, 1,
  '/gl/fi-gl-flow-todo-center', '/7485938633702490112/7485938639050227713', 1, 1, 0, 0, now(), now())
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
