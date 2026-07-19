-- 回滚：仅删索引，保留表（表可能已有 demo/生产绑定数据，删表有数据丢失风险，故不回滚表）。
DROP INDEX IF EXISTS idx_cmx_flow_subflow_binding_key;
