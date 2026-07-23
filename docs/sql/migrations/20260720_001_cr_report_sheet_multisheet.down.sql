-- 回滚 20260720_001：重建 cr_report_sheet 的 2 列唯一索引。
--
-- ⚠ 警告：重建后「同一报表同一版本」将重新被限制为只能有 1 个 sheet，
-- 多 sheet 报表会保存失败。仅为迁移对称性提供，正常不应回滚。
-- 若表中已存在多 sheet 数据，本索引会创建失败（预期行为，说明不该回滚）。
CREATE UNIQUE INDEX IF NOT EXISTS uk_cr_report_sheet_1
    ON cr_report_sheet (report_code, version_code);
