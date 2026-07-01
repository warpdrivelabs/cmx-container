-- 回滚：模块为中心的插件解耦批次(删除顺序与建表相反)
DROP TABLE IF EXISTS cmx_module_version_history;
DROP TABLE IF EXISTS cmx_module_current_version;
DROP TABLE IF EXISTS cmx_menu;
DROP TABLE IF EXISTS cmx_form;
