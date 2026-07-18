-- cmx-flow 流程引擎 M5.3：多挂载去重列
-- 幂等：ALTER TABLE ADD COLUMN IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY。
-- 依赖：20260718_004（M5.1 子流程父子列）。
-- 涉及变更：cmx_flow_instance 补 parent_node_bpmn_id 一列，支撑「一个主流程多处挂载子流程」。
-- 背景（为什么必须补这一列）：
--   一个令牌可串行经过多个 callActivity 节点（挂载点 A→B）。子流程启动去重原先只按
--   parent_token_id 归位，会把「挂载点 A 已完成的子实例」误判成「本令牌已启动子流程」，
--   导致挂载点 B 的子流程漏起（串行多挂载失效）。补记发起子实例的 callActivity 节点后，
--   去重键 = (parent_token_id, parent_node_bpmn_id)，同一令牌的不同挂载点各自独立启动。
--   并行多挂载（不同令牌）本就正确，此列对其无副作用。
-- 兼容：M5.1/M5.2 单挂载场景本列恒空，旧数据无需回填。

ALTER TABLE cmx_flow_instance ADD COLUMN IF NOT EXISTS parent_node_bpmn_id VARCHAR(128);
COMMENT ON COLUMN cmx_flow_instance.parent_node_bpmn_id IS '父实例中发起本子实例的 callActivity 节点 bpmn id（M5.3 多挂载去重键；单挂载恒空）';
