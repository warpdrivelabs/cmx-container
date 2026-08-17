-- MDM 变更申请单据号保底规则 MDM_BILL。
--
-- cv_mdm_apply.doc_no 在单据元数据里挂了 codeRule{ruleCode:MDM_BILL}，但该规则此前从未 seed
-- → 未被 activation 的 doc_code_rules 覆盖时，铸号查不到规则 → doc_no 空 → NOT NULL 校验拦截。
--
-- 本规则格式：const「CR」+ dateSerial(YYYYMMDD,width:6) → CR20260813000001（按日重置流水）。
-- 定位：单据号「保底」规则——activation 配 doc_code_rules 覆盖 doc_no 时优先用覆盖的规则；
--       未覆盖时回退本规则。如需调整单据号格式，可在「编码规则配置器」UI 修改 ruleCode=MDM_BILL。
-- 幂等：ON CONFLICT 已存在则跳过（不覆盖用户在 UI 改过的配置）。
INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000001, 'MDM_BILL', 'MDM 变更申请单据号（CR+日期+流水）', 'auto',
        '[{"type":"const","value":"CR"},{"type":"dateSerial","format":"YYYYMMDD","width":6,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;
