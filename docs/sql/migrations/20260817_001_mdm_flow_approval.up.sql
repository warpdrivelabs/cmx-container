-- M7 · 主数据审批对接流程平台：审批角色种子。
--
-- cmx_role 新增 mdm_approver（主数据审批员，BPMN candidateGroups 引用，start 时物化为
-- 任务候选池）+ admin 预分配（开发/演示环境便利；生产请走 IAM 管理界面分配，并按
-- 「人员分离」原则确保申请人与审批人非同一人——角色分离是控制层，人员分离是管理层）。
--
-- ★ 角色无成员 = 任务对所有人不可见（候选 start 时物化，事后补角色对运行中实例无效），
--   本迁移同步预分配 admin 即为消除该部署陷阱；deploy-mdm-flow.sh 部署时亦有自检提示。
-- 幂等：ON CONFLICT DO NOTHING / NOT EXISTS 防重。

INSERT INTO cmx_role (id, code, name, data_scope, sort_order, status, description)
VALUES ('1898765432100001101', 'mdm_approver', '主数据审批员', 1, 10, 1, '主数据变更申请审批（候选池认领）')
ON CONFLICT (code) WHERE archived = 0 DO NOTHING;

-- admin 预分配（id 用固定字面量保证迁移幂等可重放；uk_cmx_user_role 已含防重）
INSERT INTO cmx_user_role (id, user_id, role_id, archived, create_time)
SELECT '1898765432100001201', u.id, '1898765432100001101', 0, CURRENT_TIMESTAMP
FROM cmx_user u
WHERE u.username = 'admin'
  AND NOT EXISTS (
    SELECT 1 FROM cmx_user_role ur
    WHERE ur.user_id = u.id AND ur.role_id = '1898765432100001101'
  );
