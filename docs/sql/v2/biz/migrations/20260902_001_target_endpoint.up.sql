-- 20260902_001_target_endpoint.up.sql
-- MDM 分发订阅方案C（v2.2）：目标端点 + 订阅两级模型。
-- 新建 md_target_endpoint（目标系统级凭证/投递策略载体），md_subscription 挂 endpoint_id 瘦身。
-- 多库说明：md_subscription 在业务库——本迁移须在每个启用 MDM 治理的业务库执行。
-- 回滚保险：阶段一保留订阅旧列并双写；阶段二（另起迁移）drop 旧列。

-- 0) 前置分歧检测：同 (target_sys, channel) 组内 url/secret 不一致则中止，人工裁决拆端点后重跑。
--    （检测在本文件中以 DO 块实现：有分歧直接 RAISE，迁移整体失败，不产生半成品。）
DO $$
DECLARE n INT;
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'md_subscription') THEN
        SELECT count(*) INTO n
        FROM (
            SELECT target_sys, channel
            FROM md_subscription
            WHERE channel = 'webhook'
            GROUP BY target_sys, channel
            HAVING count(DISTINCT channel_config->>'url') > 1
                OR count(DISTINCT channel_config->>'secret') > 1
        ) g;
        IF n > 0 THEN
            RAISE EXCEPTION 'md_subscription 存在同 (target_sys, channel) 组内 url/secret 分歧的订阅（% 组），请先人工拆分后再执行迁移', n;
        END IF;
    END IF;
END $$;

-- 1) 目标端点表（v2.2：不建唯一索引——同系统同通道允许多端点，重复创建由应用层提示）
CREATE TABLE IF NOT EXISTS md_target_endpoint (
    id             BIGINT       NOT NULL,
    target_sys     VARCHAR(64)  NOT NULL,
    channel        VARCHAR(16)  NOT NULL,
    name           VARCHAR(128),
    description    VARCHAR(512),
    channel_config JSONB        NOT NULL DEFAULT '{}',
    retry_max      INT          NOT NULL DEFAULT 8,
    timeout_ms     INT          NOT NULL DEFAULT 10000,
    batch_size     INT          NOT NULL DEFAULT 50,
    active         BOOLEAN      NOT NULL DEFAULT TRUE,
    created_by     VARCHAR(64),
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_target_endpoint IS '分发目标端点（目标系统×通道，凭证与投递策略的唯一载体）';
COMMENT ON COLUMN md_target_endpoint.id             IS '主键（应用层 snowflake；回填段 9000000000+ 为非 snowflake 保留段）';
COMMENT ON COLUMN md_target_endpoint.target_sys     IS '目标系统标识（同系统同通道可多端点，重复创建由应用层提示）';
COMMENT ON COLUMN md_target_endpoint.channel        IS '通道 webhook/kafka/rocketmq/rest_pull';
COMMENT ON COLUMN md_target_endpoint.name           IS '端点名称（展示，中文为主）';
COMMENT ON COLUMN md_target_endpoint.description    IS '端点描述';
COMMENT ON COLUMN md_target_endpoint.channel_config IS '通道配置：webhook {url,secret,timeout_ms}；rest_pull {consumerId}；kafka {brokers,topic,partition_key}（预留）';
COMMENT ON COLUMN md_target_endpoint.retry_max      IS '最大尝试次数（含首发）';
COMMENT ON COLUMN md_target_endpoint.timeout_ms     IS '单次投递超时（毫秒）';
COMMENT ON COLUMN md_target_endpoint.batch_size     IS '单轮该端点下订阅最大投递数';
COMMENT ON COLUMN md_target_endpoint.active         IS '端点启停（停用 = 级联停其下全部订阅：扇出与 claim 双门）';
CREATE INDEX IF NOT EXISTS idx_md_target_endpoint ON md_target_endpoint (target_sys, channel);

-- 2) 订阅加端点列
ALTER TABLE md_subscription ADD COLUMN IF NOT EXISTS endpoint_id BIGINT;

-- 3) 回填端点（组级幂等闸：已回填组跳过，断点续跑）
--    id 用 9000000000+序号 非 snowflake 保留段（10 位 < snowflake 14 位无碰撞）；
--    分歧已被步骤 0 拦截，组内 url/secret 全一致，min() 取值确定性成立；
--    bool_or：组内任一订阅启用 → 端点启用（bool_and 会静默掐死组内其它启用订阅的扇出）。
INSERT INTO md_target_endpoint (id, target_sys, channel, channel_config, retry_max, timeout_ms, batch_size, active, created_at, updated_at)
SELECT 9000000000 + row_number() OVER (ORDER BY g.target_sys, g.channel),
       g.target_sys, g.channel,
       jsonb_build_object('url',   min(s.channel_config->>'url'),
                          'secret', min(s.channel_config->>'secret')),
       min(s.retry_max), min(s.timeout_ms), min(s.batch_size),
       bool_or(s.active), now(), now()
FROM md_subscription s
JOIN (SELECT DISTINCT target_sys, channel FROM md_subscription) g
  ON g.target_sys = s.target_sys AND g.channel = s.channel
WHERE NOT EXISTS (SELECT 1 FROM md_target_endpoint t
                  WHERE t.target_sys = g.target_sys AND t.channel = g.channel)
GROUP BY g.target_sys, g.channel;

-- 4) 订阅挂端点（幂等：仅补 NULL）
UPDATE md_subscription s SET endpoint_id = t.id
FROM md_target_endpoint t
WHERE t.target_sys = s.target_sys AND t.channel = s.channel AND s.endpoint_id IS NULL;

ALTER TABLE md_subscription ALTER COLUMN endpoint_id SET NOT NULL;

COMMENT ON COLUMN md_subscription.endpoint_id IS '所属目标端点 → md_target_endpoint.id（凭证/通道/投递策略载体）';

-- 5) 唯一键换维：同端点同字典唯一（原 (target_sys, dict_code, channel) 语义由端点归并承接）
DROP INDEX IF EXISTS uk_md_subscription;
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_subscription ON md_subscription (endpoint_id, dict_code);
