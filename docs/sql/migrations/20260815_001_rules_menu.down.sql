-- 回滚：删除决策规则引擎的两个列表工作台菜单（F2b）。
DELETE FROM cmx_menu WHERE code IN ('fi-gl-rules-design-workbench', 'fi-gl-rules-sim-workbench', 'fi-gl-rules-logs');
