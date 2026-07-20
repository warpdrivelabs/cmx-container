-- =============================================
-- 迁移说明：回滚 cmx-flow 流程引擎完整建表（M1~M5.3 + 设计器阶段0 + 菜单注册）
--          合并自 20260717_001 ~ 20260718_011 共 14 个碎片化迁移的反向操作
-- 影响表：同 up.sql
-- 操作类型：DELETE / DROP TABLE
-- 回滚方式：无
-- 注意：DROP TABLE 连带删除其上索引；DROP COLUMN 会丢失该列数据。
--       cmx_org/cmx_position 若已被 cmx_user.org_id 等引用，回滚前需确认无依赖数据。
-- 顺序：反序回滚（先回滚最后建的，最后回滚基础表）
-- =============================================

-- 17. 流程设计工作台菜单
DELETE FROM cmx_menu WHERE code = 'fi-gl-flow-design-workbench' AND archived = 0;

-- 16. 流程定义版本历史
DROP TABLE IF EXISTS cmx_flow_definition_version;

-- 15. 流程定义主记录
DROP TABLE IF EXISTS cmx_flow_definition;

-- 14. 子流程组织绑定
DROP TABLE IF EXISTS cmx_flow_subflow_binding;

-- 13. 转签台账
DROP TABLE IF EXISTS cmx_flow_task_delegation;

-- 12. 抄送记录
DROP TABLE IF EXISTS cmx_flow_cc;

-- 11. 任务候选人池
DROP TABLE IF EXISTS cmx_flow_task_candidate;

-- 10. 用户-岗位关联
DROP TABLE IF EXISTS cmx_user_position;

-- 9. 岗位表
DROP TABLE IF EXISTS cmx_position;

-- 8. 组织/部门树
DROP TABLE IF EXISTS cmx_org;

-- 7. 定时器作业
DROP TABLE IF EXISTS cmx_flow_job;

-- 6. 多实例执行域
DROP TABLE IF EXISTS cmx_flow_mi_scope;

-- 5. 历史任务
DROP TABLE IF EXISTS cmx_flow_hi_task;

-- 4. 历史实例
DROP TABLE IF EXISTS cmx_flow_hi_instance;

-- 3. 用户任务
DROP TABLE IF EXISTS cmx_flow_task;

-- 2. 令牌表
DROP TABLE IF EXISTS cmx_flow_token;

-- 1. 流程实例
DROP TABLE IF EXISTS cmx_flow_instance;
