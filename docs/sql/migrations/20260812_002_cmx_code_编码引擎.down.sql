-- =============================================
-- 迁移说明：回滚——删除 cmx-code 编码引擎三张表（反序）
-- 影响表：cmx_code_seq, cmx_code_gap, cmx_code_rule
-- 操作类型：DROP TABLE
-- 回滚方式：无
-- =============================================

DROP TABLE IF EXISTS cmx_code_seq;
DROP TABLE IF EXISTS cmx_code_gap;
DROP TABLE IF EXISTS cmx_code_rule;
