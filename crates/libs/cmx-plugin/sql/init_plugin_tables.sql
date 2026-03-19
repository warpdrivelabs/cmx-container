-- =====================================================
-- cmx-plugin 数据库初始化脚本
-- 插件生命周期管理系统 - PostgreSQL 版本
-- 基于：插件生命周期管理与版本控制系统架构文档.md
-- =====================================================

-- =====================================================
-- 第一部分：扩展
-- =====================================================

-- 启用必要的扩展
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- =====================================================
-- 第二部分：核心表结构
-- =====================================================

-- -----------------------------------------------------
-- 2.1 插件注册主表 (cmx_plugin)
-- 存储所有已安装插件的核心信息
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin CASCADE;

CREATE TABLE cmx_plugin (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255) NOT NULL UNIQUE,        -- 插件唯一标识
    name                VARCHAR(500) NOT NULL,               -- 显示名称
    version             VARCHAR(50) NOT NULL,                -- 当前版本(语义版本)
    description         TEXT,                                -- 插件描述
    status              VARCHAR(30) NOT NULL DEFAULT 'installed', -- 插件状态
    wasm_path           TEXT,                                -- WASM文件路径
    install_path        TEXT NOT NULL,                       -- 安装目录
    config_path         TEXT,                                -- 配置文件路径
    backup_path         TEXT,                                -- 备份目录
    db_id               VARCHAR(255) NOT NULL DEFAULT 'default', -- 数据库标识
    db_type             VARCHAR(50),                         -- 数据库类型
    is_system           BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否系统插件
    is_locked           BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否锁定(禁止操作)
    is_active           BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否激活
    domain_code         VARCHAR(100),                        -- 域代码
    application_code    VARCHAR(100),                        -- 应用代码
    module_code         VARCHAR(100),                        -- 模块代码
    vendor_name         VARCHAR(255),                        -- 供应商名称
    vendor_url          VARCHAR(512),                        -- 供应商网址
    vendor_contact      VARCHAR(255),                        -- 供应商联系方式
    metadata            JSONB,                               -- 扩展元数据(JSON)
    signature_algorithm VARCHAR(50),                         -- 签名算法
    signature_value     TEXT,                                -- 签名值
    signer_key_id       VARCHAR(255),                        -- 签名者密钥ID
    permissions         JSONB,                               -- 权限配置(JSON)
    installed_at        TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 安装时间
    activated_at        TIMESTAMP WITH TIME ZONE,            -- 激活时间
    create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 创建时间
    update_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()  -- 更新时间
);

-- 索引
CREATE INDEX idx_plugin_plugin_id ON cmx_plugin(plugin_id);
CREATE INDEX idx_plugin_status ON cmx_plugin(status);
CREATE INDEX idx_plugin_db_id ON cmx_plugin(db_id);
CREATE INDEX idx_plugin_is_system ON cmx_plugin(is_system);
CREATE INDEX idx_plugin_is_active ON cmx_plugin(is_active);

COMMENT ON TABLE cmx_plugin IS '插件注册主表：存储所有已安装插件的核心信息';

-- -----------------------------------------------------
-- 2.2 版本历史表 (cmx_plugin_versions)
-- 记录插件的版本变更历史
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_versions CASCADE;

CREATE TABLE cmx_plugin_versions (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255) NOT NULL,               -- 关联插件ID
    version             VARCHAR(50) NOT NULL,                -- 版本号
    version_type        VARCHAR(30) NOT NULL DEFAULT 'release', -- 版本类型
    from_version        VARCHAR(50),                         -- 升级来源版本
    install_path        TEXT NOT NULL,                       -- 安装路径
    wasm_path           TEXT,                                -- WASM文件路径
    backup_path         TEXT,                                -- 备份路径
    is_current          BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否当前版本
    installed_at        TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 安装时间
    uninstalled_at      TIMESTAMP WITH TIME ZONE,            -- 卸载时间
    installed_by        VARCHAR(255),                        -- 安装者
    install_reason      TEXT,                                -- 安装原因
    change_summary      TEXT,                                -- 变更摘要
    change_details      JSONB                                -- 变更详情
);

-- 索引
CREATE INDEX idx_versions_plugin_id ON cmx_plugin_versions(plugin_id);
CREATE INDEX idx_versions_version ON cmx_plugin_versions(version);
CREATE INDEX idx_versions_is_current ON cmx_plugin_versions(is_current);
CREATE INDEX idx_versions_installed_at ON cmx_plugin_versions(installed_at);

COMMENT ON TABLE cmx_plugin_versions IS '版本历史表：记录插件的版本变更历史';

-- -----------------------------------------------------
-- 2.3 依赖关系表 (cmx_plugin_dependencies)
-- 记录插件之间的依赖关系
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_dependencies CASCADE;

CREATE TABLE cmx_plugin_dependencies (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255) NOT NULL,               -- 依赖方插件ID
    dependency_plugin_id VARCHAR(255) NOT NULL,              -- 被依赖的插件ID
    dependency_name     VARCHAR(500),                        -- 被依赖的插件名称
    version_constraint  VARCHAR(100) NOT NULL,               -- 版本约束表达式
    resolved_version    VARCHAR(50),                         -- 解析后的版本
    is_optional         BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否可选
    is_resolved         BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否已解析
    metadata            JSONB,                               -- 扩展元数据
    create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW() -- 创建时间
);

-- 索引
CREATE INDEX idx_deps_plugin_id ON cmx_plugin_dependencies(plugin_id);
CREATE INDEX idx_deps_dependency_id ON cmx_plugin_dependencies(dependency_plugin_id);
CREATE INDEX idx_deps_optional ON cmx_plugin_dependencies(is_optional);

COMMENT ON TABLE cmx_plugin_dependencies IS '依赖关系表：记录插件之间的依赖关系';

-- -----------------------------------------------------
-- 2.4 节点部署记录表 (cmx_plugin_deployments)
-- 记录在各个节点上的部署状态
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_deployments CASCADE;

CREATE TABLE cmx_plugin_deployments (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255) NOT NULL,               -- 插件ID
    node_id             VARCHAR(100) NOT NULL,               -- 节点标识
    node_name           VARCHAR(255),                        -- 节点名称
    node_type           VARCHAR(50),                         -- 节点类型
    version             VARCHAR(50) NOT NULL,                -- 部署的版本
    deployment_type     VARCHAR(30) NOT NULL,                -- 部署类型
    deployment_strategy VARCHAR(50),                         -- 部署策略
    status              VARCHAR(30) NOT NULL,                -- 部署状态
    progress            INTEGER DEFAULT 0,                   -- 进度(0-100)
    error_message       TEXT,                                -- 错误信息
    error_details       JSONB,                               -- 错误详情
    operation_id        VARCHAR(100),                        -- 操作ID
    sync_token          VARCHAR(255),                        -- 同步令牌
    last_sync_at        TIMESTAMP WITH TIME ZONE,            -- 最后同步时间
    deployed_at         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 部署时间
    validated_at        TIMESTAMP WITH TIME ZONE             -- 验证通过时间
);

-- 索引
CREATE INDEX idx_deploy_plugin ON cmx_plugin_deployments(plugin_id);
CREATE INDEX idx_deploy_node ON cmx_plugin_deployments(node_id);
CREATE INDEX idx_deploy_status ON cmx_plugin_deployments(status);
CREATE INDEX idx_deploy_operation ON cmx_plugin_deployments(operation_id);

COMMENT ON TABLE cmx_plugin_deployments IS '节点部署记录表：记录在各个节点上的部署状态';

-- -----------------------------------------------------
-- 2.5 审计日志表 (cmx_plugin_audit_log)
-- 记录所有插件生命周期操作
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_audit_log CASCADE;

CREATE TABLE cmx_plugin_audit_log (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255),                        -- 关联插件ID
    version_id          VARCHAR(64),                         -- 关联版本ID
    deployment_id       VARCHAR(64),                         -- 关联部署ID
    operation_id        VARCHAR(100),                        -- 操作ID
    operation_type      VARCHAR(50) NOT NULL,                -- 操作类型
    operation_status    VARCHAR(30) NOT NULL,                -- 操作状态
    operator            VARCHAR(255),                        -- 操作者
    operator_ip         VARCHAR(45),                         -- 操作者IP
    operator_session    VARCHAR(255),                        -- 会话ID
    request_id          VARCHAR(100),                        -- 请求ID(链路追踪)
    correlation_id      VARCHAR(100),                        -- 关联ID
    details             JSONB,                               -- 操作详情
    old_value           JSONB,                               -- 旧值
    new_value           JSONB,                               -- 新值
    error_code          VARCHAR(50),                         -- 错误代码
    error_message       TEXT,                                -- 错误消息
    stack_trace         TEXT,                                -- 堆栈跟踪
    started_at          TIMESTAMP WITH TIME ZONE NOT NULL,   -- 操作开始时间
    completed_at        TIMESTAMP WITH TIME ZONE,            -- 操作完成时间
    duration_ms         BIGINT                               -- 操作耗时(毫秒)
);

-- 索引
CREATE INDEX idx_audit_plugin ON cmx_plugin_audit_log(plugin_id);
CREATE INDEX idx_audit_operation ON cmx_plugin_audit_log(operation_type);
CREATE INDEX idx_audit_status ON cmx_plugin_audit_log(operation_status);
CREATE INDEX idx_audit_operator ON cmx_plugin_audit_log(operator);
CREATE INDEX idx_audit_timestamp ON cmx_plugin_audit_log(started_at);
CREATE INDEX idx_audit_request ON cmx_plugin_audit_log(request_id);
CREATE INDEX idx_audit_correlation ON cmx_plugin_audit_log(correlation_id);
CREATE INDEX idx_audit_plugin_timestamp ON cmx_plugin_audit_log(plugin_id, started_at DESC);

COMMENT ON TABLE cmx_plugin_audit_log IS '审计日志表：记录所有插件生命周期操作';

-- -----------------------------------------------------
-- 2.6 回滚记录表 (cmx_plugin_rollback)
-- 记录回滚点信息
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_rollback CASCADE;

CREATE TABLE cmx_plugin_rollback (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255) NOT NULL,               -- 插件ID
    operation_id        VARCHAR(100) NOT NULL UNIQUE,        -- 原始操作ID
    from_version        VARCHAR(50) NOT NULL,                -- 回滚前版本
    to_version          VARCHAR(50) NOT NULL,                -- 回滚后版本
    backup_path         TEXT NOT NULL,                       -- 备份路径
    backup_size         BIGINT,                              -- 备份大小(字节)
    backup_checksum     VARCHAR(128),                        -- 备份校验和
    backup_create_time   TIMESTAMP WITH TIME ZONE NOT NULL,   -- 备份创建时间
    status              VARCHAR(30) NOT NULL DEFAULT 'pending', -- 状态
    completed_at        TIMESTAMP WITH TIME ZONE,            -- 完成时间
    reason              TEXT,                                -- 回滚原因
    triggered_by        VARCHAR(255),                        -- 触发者
    create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW() -- 创建时间
);

-- 索引
CREATE INDEX idx_rollback_plugin ON cmx_plugin_rollback(plugin_id);
CREATE INDEX idx_rollback_operation ON cmx_plugin_rollback(operation_id);
CREATE INDEX idx_rollback_status ON cmx_plugin_rollback(status);
CREATE INDEX idx_rollback_created ON cmx_plugin_rollback(create_time);

COMMENT ON TABLE cmx_plugin_rollback IS '回滚记录表：记录回滚点信息';

-- -----------------------------------------------------
-- 2.7 系统默认插件配置表 (cmx_system_plugins)
-- 配置系统启动时需要自动安装的插件
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_system_plugins CASCADE;

CREATE TABLE cmx_system_plugins (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255) NOT NULL UNIQUE,        -- 插件ID
    name                VARCHAR(500) NOT NULL,               -- 插件名称
    description         TEXT,                                -- 插件描述
    version             VARCHAR(50) NOT NULL,                -- 默认版本
    fallback_version    VARCHAR(50),                         -- 备用版本
    install_order       INTEGER NOT NULL DEFAULT 0,          -- 安装顺序
    is_optional         BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否可选
    is_critical         BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否关键
    auto_activate       BOOLEAN NOT NULL DEFAULT TRUE,       -- 是否自动激活
    retry_count         INTEGER NOT NULL DEFAULT 3,          -- 重试次数
    retry_delay_seconds INTEGER NOT NULL DEFAULT 10,         -- 重试间隔(秒)
    wait_for_plugins    TEXT[],                              -- 需要等待完成的插件列表
    source_type         VARCHAR(30) NOT NULL,                -- 来源类型
    source_path         TEXT,                                -- 来源路径
    source_url          TEXT,                                -- 来源URL
    source_registry     VARCHAR(255),                        -- 注册表名称
    required_signature  BOOLEAN NOT NULL DEFAULT TRUE,       -- 是否必须签名
    public_key_id       VARCHAR(255),                        -- 公钥ID
    install_config      JSONB,                               -- 安装配置
    env_vars            JSONB,                               -- 环境变量
    permissions         JSONB,                               -- 权限配置
    status              VARCHAR(30) NOT NULL DEFAULT 'pending', -- 状态
    last_installed_at   TIMESTAMP WITH TIME ZONE,            -- 最后安装时间
    last_installed_version VARCHAR(50),                      -- 最后安装版本
    install_attempts    INTEGER NOT NULL DEFAULT 0,          -- 安装尝试次数
    last_error          TEXT,                                -- 最后错误信息
    create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 创建时间
    update_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()  -- 更新时间
);

-- 索引
CREATE INDEX idx_system_plugin_order ON cmx_system_plugins(install_order);
CREATE INDEX idx_system_plugin_status ON cmx_system_plugins(status);
CREATE INDEX idx_system_plugin_optional ON cmx_system_plugins(is_optional);
CREATE INDEX idx_system_plugin_critical ON cmx_system_plugins(is_critical);

COMMENT ON TABLE cmx_system_plugins IS '系统默认插件配置表：配置系统启动时需要自动安装的插件';

-- -----------------------------------------------------
-- 2.8 节点信息表 (cmx_plugin_nodes)
-- 记录集群中的节点信息
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_nodes CASCADE;

CREATE TABLE cmx_plugin_nodes (
    node_id             VARCHAR(100) NOT NULL PRIMARY KEY,   -- 节点ID
    node_name           VARCHAR(255) NOT NULL,               -- 节点名称
    node_type           VARCHAR(30) NOT NULL DEFAULT 'worker', -- 节点类型
    status              VARCHAR(30) NOT NULL DEFAULT 'offline', -- 节点状态
    is_active           BOOLEAN NOT NULL DEFAULT TRUE,       -- 是否活跃
    host                VARCHAR(255) NOT NULL,               -- 主机地址
    port                INTEGER NOT NULL,                    -- 端口号
    protocol            VARCHAR(10) NOT NULL DEFAULT 'http', -- 协议类型
    capabilities        JSONB,                               -- 节点能力
    metadata            JSONB,                               -- 节点元数据
    last_heartbeat      TIMESTAMP WITH TIME ZONE,            -- 最后心跳时间
    last_health_check   TIMESTAMP WITH TIME ZONE,            -- 最后健康检查时间
    health_check_interval INTEGER NOT NULL DEFAULT 30,       -- 健康检查间隔(秒)
    health_status       VARCHAR(30),                         -- 健康状态
    plugin_manager_version VARCHAR(50),                      -- 插件管理器版本
    runtime_version     VARCHAR(50),                         -- 运行时版本
    total_plugins       INTEGER NOT NULL DEFAULT 0,          -- 插件总数
    active_plugins      INTEGER NOT NULL DEFAULT 0,          -- 激活插件数
    registered_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 注册时间
    last_seen_at        TIMESTAMP WITH TIME ZONE,            -- 最后在线时间
    create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 创建时间
    update_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()  -- 更新时间
);

-- 索引
CREATE INDEX idx_node_status ON cmx_plugin_nodes(status);
CREATE INDEX idx_node_type ON cmx_plugin_nodes(node_type);
CREATE INDEX idx_node_active ON cmx_plugin_nodes(is_active);
CREATE INDEX idx_node_last_heartbeat ON cmx_plugin_nodes(last_heartbeat);

COMMENT ON TABLE cmx_plugin_nodes IS '节点信息表：记录集群中的节点信息';

-- -----------------------------------------------------
-- 2.9 服务注册表 (cmx_plugin_services)
-- 记录插件提供的服务
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_services CASCADE;

CREATE TABLE cmx_plugin_services (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    service_id          VARCHAR(255) NOT NULL,               -- 服务ID
    service_name        VARCHAR(500) NOT NULL,               -- 服务名称
    service_version     VARCHAR(50) NOT NULL,                -- 服务版本
    plugin_id           VARCHAR(255) NOT NULL,               -- 提供服务的插件ID
    description         TEXT,                                -- 服务描述
    endpoints           JSONB,                               -- 服务端点列表
    methods             JSONB,                               -- 支持的方法列表
    is_singleton        BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否单例
    is_lazy             BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否懒加载
    metadata            JSONB,                               -- 服务元数据
    status              VARCHAR(30) NOT NULL DEFAULT 'active', -- 状态
    registered_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 注册时间
    update_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()  -- 更新时间
);

-- 索引
CREATE INDEX idx_services_service_id ON cmx_plugin_services(service_id);
CREATE INDEX idx_services_plugin_id ON cmx_plugin_services(plugin_id);
CREATE INDEX idx_services_status ON cmx_plugin_services(status);

COMMENT ON TABLE cmx_plugin_services IS '服务注册表：记录插件提供的服务';

-- -----------------------------------------------------
-- 2.10 服务实例表 (cmx_plugin_service_instances)
-- 记录服务的具体实例
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_service_instances CASCADE;

CREATE TABLE cmx_plugin_service_instances (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    service_id          VARCHAR(255) NOT NULL,               -- 服务ID
    plugin_id           VARCHAR(255) NOT NULL,               -- 插件ID
    node_id             VARCHAR(100),                        -- 节点ID
    instance_id         VARCHAR(255) NOT NULL UNIQUE,        -- 实例ID
    endpoint            TEXT NOT NULL,                       -- 实例端点
    status              VARCHAR(30) NOT NULL DEFAULT 'active', -- 状态
    metadata            JSONB,                               -- 实例元数据
    last_health_check   TIMESTAMP WITH TIME ZONE,            -- 最后健康检查时间
    health_status       VARCHAR(30),                         -- 健康状态
    registered_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 注册时间
    last_seen_at        TIMESTAMP WITH TIME ZONE             -- 最后在线时间
);

-- 索引
CREATE INDEX idx_instances_service_id ON cmx_plugin_service_instances(service_id);
CREATE INDEX idx_instances_plugin_id ON cmx_plugin_service_instances(plugin_id);
CREATE INDEX idx_instances_node_id ON cmx_plugin_service_instances(node_id);
CREATE INDEX idx_instances_status ON cmx_plugin_service_instances(status);

COMMENT ON TABLE cmx_plugin_service_instances IS '服务实例表：记录服务的具体实例';

-- -----------------------------------------------------
-- 2.11 插件权限配置表 (cmx_plugin_permissions)
-- 记录插件的权限配置
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_permissions CASCADE;

CREATE TABLE cmx_plugin_permissions (
    id                  VARCHAR(64) NOT NULL PRIMARY KEY,    -- 主键ID
    plugin_id           VARCHAR(255) NOT NULL UNIQUE,        -- 插件ID
    policy              VARCHAR(30) NOT NULL DEFAULT 'strict', -- 权限策略
    fs_paths            JSONB,                               -- 允许的文件路径
    fs_mode             VARCHAR(10),                         -- 文件系统模式
    network_hosts       JSONB,                               -- 允许的主机
    network_ports       JSONB,                               -- 允许的端口
    database_ids        JSONB,                               -- 允许的数据库ID
    database_operations JSONB,                               -- 允许的数据库操作
    env_vars            JSONB,                               -- 允许的环境变量
    syscalls            JSONB,                               -- 允许的系统调用
    plugin_calls        JSONB,                               -- 允许调用的插件
    whitelist           JSONB,                               -- 白名单
    blacklist           JSONB,                               -- 黑名单
    metadata            JSONB,                               -- 扩展元数据
    create_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(), -- 创建时间
    update_time          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()  -- 更新时间
);

-- 索引
CREATE INDEX idx_permissions_plugin_id ON cmx_plugin_permissions(plugin_id);
CREATE INDEX idx_permissions_policy ON cmx_plugin_permissions(policy);

COMMENT ON TABLE cmx_plugin_permissions IS '插件权限配置表：记录插件的权限配置';

-- =====================================================
-- 第三部分：视图
-- =====================================================

-- 插件完整信息视图
CREATE OR REPLACE VIEW v_plugin_full_info AS
SELECT
    p.id,
    p.plugin_id,
    p.name,
    p.version,
    p.status,
    p.is_system,
    p.is_active,
    p.db_id,
    p.installed_at,
    p.activated_at,
    p.vendor_name,
    p.description,
    (SELECT COUNT(*) FROM cmx_plugin_dependencies d WHERE d.plugin_id = p.plugin_id) AS dependency_count,
    (SELECT COUNT(*) FROM cmx_plugin_services s WHERE s.plugin_id = p.plugin_id) AS service_count,
    n.node_id AS deployed_on_node,
    n.status AS node_status
FROM cmx_plugin p
LEFT JOIN cmx_plugin_deployments d ON d.plugin_id = p.plugin_id AND d.status = 'completed'
LEFT JOIN cmx_plugin_nodes n ON n.node_id = d.node_id;

COMMENT ON VIEW v_plugin_full_info IS '插件完整信息视图：包含插件基本信息、依赖数量、服务数量等';

-- 节点状态视图
CREATE OR REPLACE VIEW v_node_status AS
SELECT
    node_id,
    node_name,
    node_type,
    status,
    host,
    port,
    last_heartbeat,
    EXTRACT(EPOCH FROM (NOW() - last_heartbeat)) AS seconds_since_heartbeat,
    total_plugins,
    active_plugins,
    registered_at
FROM cmx_plugin_nodes;

COMMENT ON VIEW v_node_status IS '节点状态视图：包含节点基本信息和心跳状态';

-- 审计日志统计视图
CREATE OR REPLACE VIEW v_audit_stats AS
SELECT
    operation_type,
    operation_status,
    COUNT(*) AS operation_count,
    AVG(duration_ms) AS avg_duration_ms,
    MAX(duration_ms) AS max_duration_ms,
    MIN(duration_ms) AS min_duration_ms,
    DATE(started_at) AS operation_date
FROM cmx_plugin_audit_log
GROUP BY operation_type, operation_status, DATE(started_at);

COMMENT ON VIEW v_audit_stats IS '审计日志统计视图：按操作类型和状态统计';

-- =====================================================
-- 第四部分：完成提示
-- =====================================================

DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'cmx-plugin 数据库初始化完成';
    RAISE NOTICE '创建的表:';
    RAISE NOTICE '  - cmx_plugin (插件注册主表)';
    RAISE NOTICE '  - cmx_plugin_versions (版本历史表)';
    RAISE NOTICE '  - cmx_plugin_dependencies (依赖关系表)';
    RAISE NOTICE '  - cmx_plugin_deployments (节点部署记录表)';
    RAISE NOTICE '  - cmx_plugin_audit_log (审计日志表)';
    RAISE NOTICE '  - cmx_plugin_rollback (回滚记录表)';
    RAISE NOTICE '  - cmx_system_plugins (系统默认插件配置表)';
    RAISE NOTICE '  - cmx_plugin_nodes (节点信息表)';
    RAISE NOTICE '  - cmx_plugin_services (服务注册表)';
    RAISE NOTICE '  - cmx_plugin_service_instances (服务实例表)';
    RAISE NOTICE '  - cmx_plugin_permissions (插件权限配置表)';
    RAISE NOTICE '========================================';
END $$;
