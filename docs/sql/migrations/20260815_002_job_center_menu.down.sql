-- 回滚：删除异步任务中心菜单入口。
DELETE FROM cmx_menu WHERE code = 'fi-gl-job-center';
