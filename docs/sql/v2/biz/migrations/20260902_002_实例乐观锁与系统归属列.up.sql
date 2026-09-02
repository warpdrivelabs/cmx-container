-- 20260902_002: 实例乐观锁与系统归属列（技术债 007 + 005，治理方案批次 2/4）
-- 007：cmx_flow_instance 加 version 乐观锁（save 以 WHERE id AND version CAS 提交并 +1；
--       配合代码侧 cc/转签台账旁路剥离——两表不再随快照 DELETE 重插，此迁移不涉及）。
-- 005：instance/hi_instance 加 system_id（结构化 API Key 声明的调用方系统归属）。
-- 说明：cc 已读保护与台账幂等化均为 SQL 语句形态变更，无 DDL。
BEGIN;

SET LOCAL lock_timeout = '5s';

ALTER TABLE cmx_flow_instance    ADD COLUMN IF NOT EXISTS version   BIGINT NOT NULL DEFAULT 0;
ALTER TABLE cmx_flow_instance    ADD COLUMN IF NOT EXISTS system_id VARCHAR(64);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_system ON cmx_flow_instance (system_id);

ALTER TABLE cmx_flow_hi_instance ADD COLUMN IF NOT EXISTS version   BIGINT NOT NULL DEFAULT 0;
ALTER TABLE cmx_flow_hi_instance ADD COLUMN IF NOT EXISTS system_id VARCHAR(64);

COMMENT ON COLUMN cmx_flow_instance.version   IS '乐观锁版本（技术债 007）：save 以 WHERE id AND version CAS 提交并 +1，0 行即并发冲突 409';
COMMENT ON COLUMN cmx_flow_instance.system_id IS '发起方业务系统标识（技术债 005：来自结构化 API Key 声明；NULL = legacy 调用未声明系统）；子实例继承';
COMMENT ON COLUMN cmx_flow_hi_instance.version   IS '归档时的乐观锁版本（技术债 007 审计留档）';
COMMENT ON COLUMN cmx_flow_hi_instance.system_id IS '发起方业务系统标识（技术债 005 归档登记）';

COMMIT;
