-- 20260721_001_deploy_mode_mono_app_id_unification.down.sql
-- 警告：此迁移不可自动逆向（UPDATE 已丢失原值）。
-- 回滚步骤：
-- 1. 从 cmx_app_id_backup_20260721 备份表逐表恢复 app_id（按 src 列区分）
-- 2. 或根据 cmx_module 表的三元组重新计算 app_id（仅适用于从未改过 module_code 的场景）
-- 此 down.sql 不执行任何 SQL，仅作占位。
SELECT '无法自动回滚 app_id 统一迁移，请手动从 cmx_app_id_backup_20260721 恢复' AS warning;
