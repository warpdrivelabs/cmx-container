-- 回滚：移除 MDM_BILL 保底规则（仅删本 seed 写入的固定 id 行，不动用户在 UI 改过的同名规则）。
DELETE FROM cmx_code_rule WHERE rule_code = 'MDM_BILL' AND id = 9000000000000001;
