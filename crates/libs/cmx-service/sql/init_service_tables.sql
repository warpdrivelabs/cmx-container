-- =====================================================
-- cmx-service 数据库初始化脚本
-- 服务编排系统 - PostgreSQL 版本
-- =====================================================

-- =====================================================
-- 第一部分：扩展
-- =====================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- =====================================================
-- 第二部分：服务定义表
-- =====================================================

-- 服务定义主表
DROP TABLE IF EXISTS cmx_service_define CASCADE;

CREATE TABLE cmx_service_define (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,
    service_key         VARCHAR(100) NOT NULL,
    service_name        VARCHAR(100),
    description         VARCHAR(255),
    plugin_id           VARCHAR(64),
    status              INT4 DEFAULT 1 COMMENT '状态：0-禁用，1-启用',
    version             VARCHAR(50) DEFAULT '1.0.0',
    create_time         TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time         TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100)
);

COMMENT ON TABLE cmx_service_define IS '服务定义表';
CREATE UNIQUE INDEX uk_cmx_service_define_key ON cmx_service_define(service_key);
CREATE INDEX idx_cmx_service_define_plugin ON cmx_service_define(plugin_id);
CREATE INDEX idx_cmx_service_define_status ON cmx_service_define(status);

-- 服务定义版本表
DROP TABLE IF EXISTS cmx_service_define_version CASCADE;

CREATE TABLE cmx_service_define_version (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,
    service_key         VARCHAR(100),
    version             VARCHAR(50),
    plugin_id           VARCHAR(64),
    plugin_version      VARCHAR(50),
    config              TEXT,
    create_time         TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time         TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by           VARCHAR(100),
    create_name         VARCHAR(100),
    update_by           VARCHAR(100),
    update_name         VARCHAR(100)
);

COMMENT ON TABLE cmx_service_define_version IS '服务定义版本表';
CREATE INDEX idx_cmx_service_version_service ON cmx_service_define_version(service_key);
CREATE INDEX idx_cmx_service_version_plugin ON cmx_service_define_version(plugin_id);

-- =====================================================
-- 第二部分：完成提示
-- =====================================================

DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'cmx-service 数据库初始化完成';
    RAISE NOTICE '创建的表:';
    RAISE NOTICE '  - cmx_service_define (服务定义表)';
    RAISE NOTICE '  - cmx_service_define_version (服务定义版本表)';
    RAISE NOTICE '========================================';
END $$;
