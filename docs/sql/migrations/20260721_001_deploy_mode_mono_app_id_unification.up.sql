-- 20260721_001_deploy_mode_mono_app_id_unification.up.sql
-- 目的：把历史 app_id 为具体模块值的记录统一为 'default'，
-- 配合 [deploy] mode = "mono" 下 get_app_id() 固定返回 "default" 的行为。
--
-- 背景：引入 [deploy] mode = "mono" | "micro" 部署模式开关后，
-- mono 模式下 get_app_id() 固定返回 "default"（不读 [app].module_code），
-- 为让历史数据（app_id='gl'/'ap'/... 等）在 mono 模式下仍可见，需统一迁移。
--
-- 注意：此迁移仅在切到 mono 模式时执行；
-- 若未来切回 micro 模式并希望恢复按模块隔离，需用 down.sql 提示手动恢复。
-- 迁移前**必须**执行下方备份脚本。

-- ============ 备份（运维必须执行，可选但强烈建议） ============
-- CREATE TABLE cmx_app_id_backup_20260721 AS
-- SELECT id, app_id, 'cmx_plugin' AS src FROM cmx_plugin WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_plugin_versions' FROM cmx_plugin_versions WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_plugin_audit_log' FROM cmx_plugin_audit_log WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_meta_table_define' FROM cmx_meta_table_define WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_meta_table_define_version' FROM cmx_meta_table_define_version WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_service_define' FROM cmx_service_define WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_service_define_version' FROM cmx_service_define_version WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_meta' FROM cmx_model_meta WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_module' FROM cmx_model_module WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_module_kind' FROM cmx_model_module_kind WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_deploy_history' FROM cmx_model_deploy_history WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_source' FROM cmx_model_source WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_registry' FROM cmx_model_registry WHERE app_id != 'default';

-- ============ 统一 app_id 为 'default'（13 张表） ============
UPDATE cmx_plugin SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_plugin_versions SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_plugin_audit_log SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_meta_table_define SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_meta_table_define_version SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_service_define SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_service_define_version SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_meta SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_module SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_module_kind SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_deploy_history SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_source SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_registry SET app_id = 'default' WHERE app_id != 'default';

-- cmx_audit_log 不迁移（审计数据不可变；查询侧 mono 模式不做 app_id 过滤）

-- ============ 验证（应全部返回 0） ============
-- SELECT 'cmx_plugin' AS t, COUNT(*) FROM cmx_plugin WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_plugin_versions', COUNT(*) FROM cmx_plugin_versions WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_plugin_audit_log', COUNT(*) FROM cmx_plugin_audit_log WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_meta_table_define', COUNT(*) FROM cmx_meta_table_define WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_meta_table_define_version', COUNT(*) FROM cmx_meta_table_define_version WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_service_define', COUNT(*) FROM cmx_service_define WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_service_define_version', COUNT(*) FROM cmx_service_define_version WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_meta', COUNT(*) FROM cmx_model_meta WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_module', COUNT(*) FROM cmx_model_module WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_module_kind', COUNT(*) FROM cmx_model_module_kind WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_deploy_history', COUNT(*) FROM cmx_model_deploy_history WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_source', COUNT(*) FROM cmx_model_source WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_registry', COUNT(*) FROM cmx_model_registry WHERE app_id != 'default';
