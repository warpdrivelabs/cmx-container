-- 回滚：cmx-flow 流程设计工作台菜单。

DELETE FROM cmx_menu WHERE code = 'fi-gl-flow-design-workbench' AND archived = 0;
