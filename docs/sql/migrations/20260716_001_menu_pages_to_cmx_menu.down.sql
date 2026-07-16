-- 回滚：清理迁移导入的菜单（fi/cmxfico 下 gl、report 两模块）
DELETE FROM cmx_menu WHERE domain_code = 'fi' AND application_code = 'cmxfico' AND module_code IN ('gl', 'report');
