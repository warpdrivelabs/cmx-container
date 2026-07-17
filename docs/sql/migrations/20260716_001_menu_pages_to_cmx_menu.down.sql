-- 回滚：清理迁移导入的菜单（按扫描到的模块删除）
DELETE FROM cmx_menu WHERE domain_code = 'fi' AND application_code = 'cmxfico' AND module_code IN ('gl', 'report');
