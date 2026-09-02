-- =============================================
-- 迁移说明：流程 webhook v2.4 三级路由——实例发起绑定列 + 投递行路由成因列
-- 影响表：cmx_flow_instance, cmx_flow_hi_instance, cmx_flow_webhook_delivery
-- 操作类型：ADD COLUMN / CREATE INDEX
-- 回滚方式：20260902_001_流程webhook三级路由列.down.sql
-- =============================================

-- L2 发起绑定：实例落订阅 id（NULL = 未绑定，事件走规则 2 全量匹配；子实例继承父实例值）。
-- 部分索引服务删除守卫（非终态绑定实例存在性检查）与绑定维度查询；写入开销近零。
ALTER TABLE cmx_flow_instance ADD COLUMN IF NOT EXISTS subscriber_id BIGINT;
COMMENT ON COLUMN cmx_flow_instance.subscriber_id IS '发起绑定的 webhook 订阅 id（v2.4 三级路由 L2；NULL = 未绑定走规则 2 全量匹配）；子实例继承父实例值';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_subscriber ON cmx_flow_instance (subscriber_id) WHERE subscriber_id IS NOT NULL;

-- 历史实例归档同步登记（终态清理后「曾绑给谁」审计不断链）。
ALTER TABLE cmx_flow_hi_instance ADD COLUMN IF NOT EXISTS subscriber_id BIGINT;
COMMENT ON COLUMN cmx_flow_hi_instance.subscriber_id IS '发起绑定的 webhook 订阅 id（v2.4 归档登记；终态清理后「曾绑给谁」审计不断链）';

-- 投递行路由成因：回答「这条投递为什么发生」。值域 bound | matched；
-- 测试行不经此列区分（复用 source='test'），落默认 matched。
-- 生产执行注意（方案 §3.3）：长查询可能被 ALTER 锁队列阻塞，先 SET lock_timeout 再执行。
SET lock_timeout = '5s';
ALTER TABLE cmx_flow_webhook_delivery ADD COLUMN IF NOT EXISTS route_source VARCHAR(8) NOT NULL DEFAULT 'matched';
COMMENT ON COLUMN cmx_flow_webhook_delivery.route_source IS '路由成因（v2.4 三级路由）：bound 发起绑定定向投递 / matched 规则匹配（含 L3 显式旁听）；测试行复用 source 列区分';
