-- =====================================================
-- cmx-plugin 数据库初始化脚本
-- 插件生命周期管理系统 - PostgreSQL 版本
-- 基于：插件生命周期管理与版本控制系统架构文档.md
-- =====================================================

-- =====================================================
-- 第一部分：扩展和工具函数
-- =====================================================

-- 启用必要的扩展
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- 自动更新 updated_at 时间戳的函数
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- 生成 UUID 的函数
CREATE OR REPLACE FUNCTION generate_operation_id()
RETURNS VARCHAR(100) AS $$
BEGIN
    RETURN 'op_' || to_char(NOW(), 'YYYYMMDDHH24MISS') || '_' || substr(md5(random()::text), 1, 8);
END;
$$ language 'plpgsql';

-- =====================================================
-- 第二部分：核心表结构
-- =====================================================

-- -----------------------------------------------------
-- 2.1 插件注册主表 (cmx_plugin)
-- 存储所有已安装插件的核心信息
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin CASCADE;

CREATE TABLE cmx_plugin (
    -- 主键
    id                  BIGSERIAL PRIMARY KEY,
    
    -- 基础信息
    plugin_id           VARCHAR(255) NOT NULL UNIQUE,  -- 插件唯一标识
    name                VARCHAR(500) NOT NULL,         -- 显示名称
    version             VARCHAR(50) NOT NULL,          -- 当前版本 (语义版本)
    description         TEXT,                          -- 插件描述
    
    -- 状态信息
    status              VARCHAR(30) NOT NULL DEFAULT 'installed',  -- 插件状态
    
    -- 文件路径
    wasm_path           TEXT,                          -- WASM 文件路径
    install_path        TEXT NOT NULL,                 -- 安装目录
    config_path         TEXT,                          -- 配置文件路径
    backup_path         TEXT,                          -- 备份目录
    
    -- 数据库配置
    db_id               VARCHAR(255) NOT NULL DEFAULT 'default',  -- 数据库标识
    db_type             VARCHAR(50),                   -- 数据库类型
    
    -- 系统标记
    is_system           BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否系统插件
    is_locked           BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否锁定（禁止操作）
    is_active           BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否激活
    
    -- 域信息
    domain_code         VARCHAR(100),                  -- 域代码
    application_code    VARCHAR(100),                  -- 应用代码
    module_code         VARCHAR(100),                  -- 模块代码
    
    -- 供应商信息
    vendor_name         VARCHAR(255),                  -- 供应商名称
    vendor_url          VARCHAR(512),                  -- 供应商网址
    vendor_contact      VARCHAR(255),                  -- 供应商联系方式
    
    -- 元数据
    metadata            JSONB,                         -- 扩展元数据 (JSON)
    
    -- 签名信息
    signature_algorithm VARCHAR(50),                   -- 签名算法
    signature_value     TEXT,                          -- 签名值
    signer_key_id       VARCHAR(255),                  -- 签名者密钥 ID
    
    -- 权限配置
    permissions         JSONB,                         -- 权限配置 (JSON)
    
    -- 时间戳
    installed_at        TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    activated_at        TIMESTAMP WITH TIME ZONE,
    created_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- 约束
    CONSTRAINT chk_plugin_status CHECK (
        status IN ('installed', 'active', 'inactive', 'error', 'pending', 'uninstalling')
    )
);

-- 索引
CREATE INDEX idx_plugin_plugin_id ON cmx_plugin(plugin_id);
CREATE INDEX idx_plugin_status ON cmx_plugin(status);
CREATE INDEX idx_plugin_db_id ON cmx_plugin(db_id);
CREATE INDEX idx_plugin_is_system ON cmx_plugin(is_system);
CREATE INDEX idx_plugin_is_active ON cmx_plugin(is_active);
CREATE INDEX idx_plugin_domain ON cmx_plugin(domain_code, application_code, module_code);

-- 触发器
CREATE TRIGGER update_cmx_plugin_updated_at 
    BEFORE UPDATE ON cmx_plugin 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE cmx_plugin IS '插件注册主表：存储所有已安装插件的核心信息';
COMMENT ON COLUMN cmx_plugin.plugin_id IS '插件唯一标识符，如 "example_plugin"';
COMMENT ON COLUMN cmx_plugin.status IS '插件状态：installed-已安装, active-已激活, inactive-已停用, error-错误, pending-待处理';
COMMENT ON COLUMN cmx_plugin.db_id IS '数据库标识符，用于关联插件数据库';

-- -----------------------------------------------------
-- 2.2 版本历史表 (cmx_plugin_versions)
-- 记录插件的版本变更历史
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_versions CASCADE;

CREATE TABLE cmx_plugin_versions (
    id                  BIGSERIAL PRIMARY KEY,
    plugin_id           VARCHAR(255) NOT NULL,         -- 关联插件 ID
    
    -- 版本信息
    version             VARCHAR(50) NOT NULL,          -- 版本号
    version_type        VARCHAR(30) NOT NULL DEFAULT 'release',  -- 版本类型
    from_version        VARCHAR(50),                   -- 升级来源版本
    
    -- 文件路径
    install_path        TEXT NOT NULL,                 -- 安装路径
    wasm_path           TEXT,                          -- WASM 文件路径
    backup_path         TEXT,                          -- 备份路径
    
    -- 状态
    is_current          BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否当前版本
    
    -- 安装信息
    installed_at        TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    uninstalled_at      TIMESTAMP WITH TIME ZONE,
    installed_by        VARCHAR(255),                  -- 安装者
    install_reason      TEXT,                          -- 安装原因
    
    -- 变更信息
    change_summary      TEXT,                          -- 变更摘要
    change_details      JSONB,                         -- 变更详情
    
    -- 约束
    CONSTRAINT fk_versions_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE,
    CONSTRAINT chk_version_type CHECK (
        version_type IN ('release', 'beta', 'alpha', 'dev', 'rc')
    )
);

-- 索引
CREATE INDEX idx_versions_plugin_id ON cmx_plugin_versions(plugin_id);
CREATE INDEX idx_versions_version ON cmx_plugin_versions(version);
CREATE INDEX idx_versions_is_current ON cmx_plugin_versions(is_current);
CREATE INDEX idx_versions_installed_at ON cmx_plugin_versions(installed_at);

-- 唯一约束：同一插件的同一版本只能有一条记录
CREATE UNIQUE INDEX idx_versions_unique ON cmx_plugin_versions(plugin_id, version);

COMMENT ON TABLE cmx_plugin_versions IS '版本历史表：记录插件的版本变更历史';
COMMENT ON COLUMN cmx_plugin_versions.version_type IS '版本类型：release-正式版, beta-测试版, alpha-内测版, dev-开发版, rc-候选版';

-- -----------------------------------------------------
-- 2.3 依赖关系表 (cmx_plugin_dependencies)
-- 记录插件之间的依赖关系
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_dependencies CASCADE;

CREATE TABLE cmx_plugin_dependencies (
    id                  BIGSERIAL PRIMARY KEY,
    plugin_id           VARCHAR(255) NOT NULL,         -- 依赖方插件 ID
    
    -- 被依赖的插件信息
    dependency_plugin_id VARCHAR(255) NOT NULL,        -- 被依赖的插件 ID
    dependency_name     VARCHAR(500),                  -- 被依赖的插件名称
    
    -- 版本约束
    version_constraint  VARCHAR(100) NOT NULL,         -- 版本约束表达式
    resolved_version    VARCHAR(50),                   -- 解析后的版本
    
    -- 依赖属性
    is_optional         BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否可选
    is_resolved         BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否已解析
    
    -- 元数据
    metadata            JSONB,                         -- 扩展元数据
    
    created_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- 约束
    CONSTRAINT fk_deps_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE
);

-- 索引
CREATE INDEX idx_deps_plugin_id ON cmx_plugin_dependencies(plugin_id);
CREATE INDEX idx_deps_dependency_id ON cmx_plugin_dependencies(dependency_plugin_id);
CREATE INDEX idx_deps_optional ON cmx_plugin_dependencies(is_optional);

-- 唯一约束：同一插件对同一依赖只能有一条记录
CREATE UNIQUE INDEX idx_deps_unique ON cmx_plugin_dependencies(plugin_id, dependency_plugin_id);

COMMENT ON TABLE cmx_plugin_dependencies IS '依赖关系表：记录插件之间的依赖关系';
COMMENT ON COLUMN cmx_plugin_dependencies.version_constraint IS '版本约束表达式，如 "^1.2.3", ">=1.0.0,<2.0.0", "~1.2.0"';

-- -----------------------------------------------------
-- 2.4 节点部署记录表 (cmx_plugin_deployments)
-- 记录在各个节点上的部署状态
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_deployments CASCADE;

CREATE TABLE cmx_plugin_deployments (
    id                  BIGSERIAL PRIMARY KEY,
    plugin_id           VARCHAR(255) NOT NULL,
    
    -- 节点信息
    node_id             VARCHAR(100) NOT NULL,         -- 节点标识
    node_name           VARCHAR(255),                  -- 节点名称
    node_type           VARCHAR(50),                   -- 节点类型
    
    -- 部署信息
    version             VARCHAR(50) NOT NULL,          -- 部署的版本
    deployment_type     VARCHAR(30) NOT NULL,          -- 部署类型
    deployment_strategy VARCHAR(50),                   -- 部署策略
    
    -- 同步状态
    status              VARCHAR(30) NOT NULL,          -- 部署状态
    progress            INTEGER DEFAULT 0,             -- 进度 (0-100)
    error_message       TEXT,                          -- 错误信息
    error_details       JSONB,                         -- 错误详情
    
    -- 同步信息
    operation_id        VARCHAR(100),                  -- 操作 ID
    sync_token          VARCHAR(255),                  -- 同步令牌
    last_sync_at        TIMESTAMP WITH TIME ZONE,      -- 最后同步时间
    
    -- 部署结果
    deployed_at         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    validated_at        TIMESTAMP WITH TIME ZONE,      -- 验证通过时间
    
    -- 约束
    CONSTRAINT fk_deploy_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE,
    CONSTRAINT uk_node_plugin UNIQUE (node_id, plugin_id),
    CONSTRAINT chk_deployment_status CHECK (
        status IN ('pending', 'in_progress', 'completed', 'failed', 'rolling_back', 'rolled_back', 'cancelled')
    ),
    CONSTRAINT chk_deployment_type CHECK (
        deployment_type IN ('initial', 'sync', 'recovery', 'upgrade', 'rollback')
    )
);

-- 索引
CREATE INDEX idx_deploy_plugin ON cmx_plugin_deployments(plugin_id);
CREATE INDEX idx_deploy_node ON cmx_plugin_deployments(node_id);
CREATE INDEX idx_deploy_status ON cmx_plugin_deployments(status);
CREATE INDEX idx_deploy_operation ON cmx_plugin_deployments(operation_id);

COMMENT ON TABLE cmx_plugin_deployments IS '节点部署记录表：记录在各个节点上的部署状态';
COMMENT ON COLUMN cmx_plugin_deployments.deployment_strategy IS '部署策略：serial-串行, parallel-并行, rolling-滚动, blue_green-蓝绿';

-- -----------------------------------------------------
-- 2.5 审计日志表 (cmx_plugin_audit_log)
-- 记录所有插件生命周期操作
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_audit_log CASCADE;

CREATE TABLE cmx_plugin_audit_log (
    id                  BIGSERIAL PRIMARY KEY,
    
    -- 关联信息
    plugin_id           VARCHAR(255),                  -- 关联插件 ID
    version_id          BIGINT,                        -- 关联版本 ID
    deployment_id       BIGINT,                        -- 关联部署 ID
    
    -- 操作信息
    operation_id        VARCHAR(100),                  -- 操作 ID
    operation_type      VARCHAR(50) NOT NULL,          -- 操作类型
    operation_status    VARCHAR(30) NOT NULL,          -- 操作状态
    
    -- 操作者信息
    operator            VARCHAR(255),                  -- 操作者
    operator_ip         VARCHAR(45),                   -- 操作者 IP
    operator_session    VARCHAR(255),                  -- 会话 ID
    
    -- 请求信息
    request_id          VARCHAR(100),                  -- 请求 ID (链路追踪)
    correlation_id      VARCHAR(100),                  -- 关联 ID
    
    -- 详情
    details             JSONB,                         -- 操作详情
    old_value           JSONB,                         -- 旧值
    new_value           JSONB,                         -- 新值
    
    -- 错误信息
    error_code          VARCHAR(50),                   -- 错误代码
    error_message       TEXT,                          -- 错误消息
    stack_trace         TEXT,                          -- 堆栈跟踪
    
    -- 时间戳
    started_at          TIMESTAMP WITH TIME ZONE NOT NULL, -- 操作开始时间
    completed_at        TIMESTAMP WITH TIME ZONE,      -- 操作完成时间
    duration_ms         BIGINT,                        -- 操作耗时 (毫秒)
    
    -- 约束
    CONSTRAINT chk_operation_type CHECK (
        operation_type IN (
            'install', 'uninstall', 'activate', 'deactivate', 
            'upgrade', 'downgrade', 'rollback', 'validate',
            'deploy', 'sync', 'recovery', 'config_update',
            'signature_verify', 'dependency_resolve', 'permission_check'
        )
    ),
    CONSTRAINT chk_operation_status CHECK (
        operation_status IN ('pending', 'in_progress', 'success', 'failed', 'partial_failed', 'cancelled')
    )
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
COMMENT ON COLUMN cmx_plugin_audit_log.operation_type IS '操作类型：install-安装, uninstall-卸载, activate-激活, deactivate-停用, upgrade-升级, downgrade-降级, rollback-回滚';

-- -----------------------------------------------------
-- 2.6 回滚记录表 (cmx_plugin_rollback)
-- 记录回滚点信息
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_rollback CASCADE;

CREATE TABLE cmx_plugin_rollback (
    id                  BIGSERIAL PRIMARY KEY,
    
    -- 关联信息
    plugin_id           VARCHAR(255) NOT NULL,
    operation_id        VARCHAR(100) NOT NULL UNIQUE,  -- 原始操作 ID
    
    -- 版本信息
    from_version        VARCHAR(50) NOT NULL,          -- 回滚前版本
    to_version          VARCHAR(50) NOT NULL,          -- 回滚后版本
    
    -- 备份信息
    backup_path         TEXT NOT NULL,                 -- 备份路径
    backup_size         BIGINT,                        -- 备份大小 (字节)
    backup_checksum     VARCHAR(128),                  -- 备份校验和
    backup_created_at   TIMESTAMP WITH TIME ZONE NOT NULL,
    
    -- 状态
    status              VARCHAR(30) NOT NULL DEFAULT 'pending',
    completed_at        TIMESTAMP WITH TIME ZONE,      -- 完成时间
    
    -- 原因
    reason              TEXT,                          -- 回滚原因
    triggered_by        VARCHAR(255),                  -- 触发者
    
    -- 时间戳
    created_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- 约束
    CONSTRAINT fk_rollback_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE,
    CONSTRAINT chk_rollback_status CHECK (
        status IN ('pending', 'in_progress', 'completed', 'failed', 'skipped', 'expired')
    )
);

-- 索引
CREATE INDEX idx_rollback_plugin ON cmx_plugin_rollback(plugin_id);
CREATE INDEX idx_rollback_operation ON cmx_plugin_rollback(operation_id);
CREATE INDEX idx_rollback_status ON cmx_plugin_rollback(status);
CREATE INDEX idx_rollback_created ON cmx_plugin_rollback(created_at);

COMMENT ON TABLE cmx_plugin_rollback IS '回滚记录表：记录回滚点信息';

-- -----------------------------------------------------
-- 2.7 系统默认插件配置表 (cmx_system_plugins)
-- 配置系统启动时需要自动安装的插件
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_system_plugins CASCADE;

CREATE TABLE cmx_system_plugins (
    id                  BIGSERIAL PRIMARY KEY,
    
    -- 插件信息
    plugin_id           VARCHAR(255) NOT NULL UNIQUE,
    name                VARCHAR(500) NOT NULL,
    description         TEXT,
    
    -- 安装配置
    version             VARCHAR(50) NOT NULL,          -- 默认版本
    fallback_version    VARCHAR(50),                   -- 备用版本
    
    -- 安装顺序
    install_order       INTEGER NOT NULL DEFAULT 0,    -- 安装顺序
    
    -- 行为配置
    is_optional         BOOLEAN NOT NULL DEFAULT FALSE, -- 是否可选
    is_critical         BOOLEAN NOT NULL DEFAULT FALSE, -- 是否关键
    auto_activate       BOOLEAN NOT NULL DEFAULT TRUE,  -- 是否自动激活
    retry_count         INTEGER NOT NULL DEFAULT 3,    -- 重试次数
    retry_delay_seconds INTEGER NOT NULL DEFAULT 10,   -- 重试间隔
    
    -- 依赖配置
    wait_for_plugins    TEXT[],                        -- 需要等待完成的插件列表
    
    -- 安装源
    source_type         VARCHAR(30) NOT NULL,          -- 来源类型
    source_path         TEXT,                          -- 来源路径
    source_url          TEXT,                          -- 来源 URL
    source_registry     VARCHAR(255),                  -- 注册表名称
    
    -- 签名配置
    required_signature  BOOLEAN NOT NULL DEFAULT TRUE, -- 是否必须签名
    public_key_id       VARCHAR(255),                  -- 公钥 ID
    
    -- 扩展配置
    install_config      JSONB,                         -- 安装配置
    env_vars            JSONB,                         -- 环境变量
    permissions         JSONB,                         -- 权限配置
    
    -- 状态
    status              VARCHAR(30) NOT NULL DEFAULT 'pending',
    last_installed_at   TIMESTAMP WITH TIME ZONE,      -- 最后安装时间
    last_installed_version VARCHAR(50),                -- 最后安装版本
    install_attempts    INTEGER NOT NULL DEFAULT 0,    -- 安装尝试次数
    last_error          TEXT,                          -- 最后错误信息
    
    -- 时间戳
    created_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- 约束
    CONSTRAINT chk_source_type CHECK (
        source_type IN ('bundled', 'registry', 'url', 'path', 'directory')
    ),
    CONSTRAINT chk_system_plugin_status CHECK (
        status IN ('pending', 'installing', 'installed', 'active', 'failed', 'skipped', 'disabled')
    )
);

-- 索引
CREATE INDEX idx_system_plugin_order ON cmx_system_plugins(install_order);
CREATE INDEX idx_system_plugin_status ON cmx_system_plugins(status);
CREATE INDEX idx_system_plugin_optional ON cmx_system_plugins(is_optional);
CREATE INDEX idx_system_plugin_critical ON cmx_system_plugins(is_critical);

-- 触发器
CREATE TRIGGER update_cmx_system_plugins_updated_at 
    BEFORE UPDATE ON cmx_system_plugins 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE cmx_system_plugins IS '系统默认插件配置表：配置系统启动时需要自动安装的插件';
COMMENT ON COLUMN cmx_system_plugins.install_order IS '安装顺序：数字越小越先安装';
COMMENT ON COLUMN cmx_system_plugins.is_critical IS '是否关键：关键插件失败会导致系统无法启动';

-- -----------------------------------------------------
-- 2.8 节点信息表 (cmx_plugin_nodes)
-- 记录集群中的节点信息
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_nodes CASCADE;

CREATE TABLE cmx_plugin_nodes (
    node_id             VARCHAR(100) PRIMARY KEY,
    node_name           VARCHAR(255) NOT NULL,
    
    -- 节点类型
    node_type           VARCHAR(30) NOT NULL DEFAULT 'worker',  -- 节点类型
    
    -- 节点状态
    status              VARCHAR(30) NOT NULL DEFAULT 'offline', -- 节点状态
    is_active           BOOLEAN NOT NULL DEFAULT TRUE,
    
    -- 地址信息
    host                VARCHAR(255) NOT NULL,
    port                INTEGER NOT NULL,
    protocol            VARCHAR(10) NOT NULL DEFAULT 'http',
    
    -- 能力
    capabilities        JSONB,                         -- 节点能力
    metadata            JSONB,                         -- 节点元数据
    
    -- 健康检查
    last_heartbeat      TIMESTAMP WITH TIME ZONE,      -- 最后心跳时间
    last_health_check   TIMESTAMP WITH TIME ZONE,      -- 最后健康检查
    health_check_interval INTEGER NOT NULL DEFAULT 30, -- 健康检查间隔 (秒)
    health_status       VARCHAR(30),                   -- 健康状态
    
    -- 版本信息
    plugin_manager_version VARCHAR(50),                -- 插件管理器版本
    runtime_version     VARCHAR(50),                   -- 运行时版本
    
    -- 统计信息
    total_plugins       INTEGER NOT NULL DEFAULT 0,    -- 插件总数
    active_plugins      INTEGER NOT NULL DEFAULT 0,    -- 激活插件数
    
    -- 时间戳
    registered_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_seen_at        TIMESTAMP WITH TIME ZONE,
    created_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- 约束
    CONSTRAINT chk_node_type CHECK (
        node_type IN ('master', 'worker', 'edge', 'gateway')
    ),
    CONSTRAINT chk_node_status CHECK (
        status IN ('online', 'offline', 'maintenance', 'degraded', 'unreachable')
    )
);

-- 索引
CREATE INDEX idx_node_status ON cmx_plugin_nodes(status);
CREATE INDEX idx_node_type ON cmx_plugin_nodes(node_type);
CREATE INDEX idx_node_active ON cmx_plugin_nodes(is_active);
CREATE INDEX idx_node_last_heartbeat ON cmx_plugin_nodes(last_heartbeat);

-- 触发器
CREATE TRIGGER update_cmx_plugin_nodes_updated_at 
    BEFORE UPDATE ON cmx_plugin_nodes 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE cmx_plugin_nodes IS '节点信息表：记录集群中的节点信息';
COMMENT ON COLUMN cmx_plugin_nodes.node_type IS '节点类型：master-主节点, worker-工作节点, edge-边缘节点, gateway-网关节点';
COMMENT ON COLUMN cmx_plugin_nodes.capabilities IS '节点能力：包含 CPU 核心数、内存大小、支持的运行时等信息';

-- -----------------------------------------------------
-- 2.9 服务注册表 (cmx_plugin_services)
-- 记录插件提供的服务
-- -----------------------------------------------------
DROP TABLE IF EXISTS cmx_plugin_services CASCADE;

CREATE TABLE cmx_plugin_services (
    id                  BIGSERIAL PRIMARY KEY,
    
    -- 服务信息
    service_id          VARCHAR(255) NOT NULL,         -- 服务 ID
    service_name        VARCHAR(500) NOT NULL,         -- 服务名称
    service_version     VARCHAR(50) NOT NULL,          -- 服务版本
    
    -- 关联插件
    plugin_id           VARCHAR(255) NOT NULL,         -- 提供服务的插件 ID
    
    -- 服务描述
    description         TEXT,                          -- 服务描述
    endpoints           JSONB,                         -- 服务端点列表
    methods             JSONB,                         -- 支持的方法列表
    
    -- 服务配置
    is_singleton        BOOLEAN NOT NULL DEFAULT FALSE, -- 是否单例
    is_lazy            BOOLEAN NOT NULL DEFAULT FALSE, -- 是否懒加载
    
    -- 元数据
    metadata            JSONB,                         -- 服务元数据
    
    -- 状态
    status              VARCHAR(30) NOT NULL DEFAULT 'active',
    
    -- 时间戳
    registered_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- 约束
    CONSTRAINT fk_services_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE,
    CONSTRAINT uk_service_plugin UNIQUE (service_id, plugin_id),
    CONSTRAINT chk_service_status CHECK (
        status IN ('active', 'inactive', 'deprecated')
    )
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
    id                  BIGSERIAL PRIMARY KEY,
    
    -- 关联信息
    service_id          VARCHAR(255) NOT NULL,         -- 服务 ID
    plugin_id           VARCHAR(255) NOT NULL,         -- 插件 ID
    node_id             VARCHAR(100),                  -- 节点 ID
    
    -- 实例信息
    instance_id         VARCHAR(255) NOT NULL UNIQUE,  -- 实例 ID
    endpoint            TEXT NOT NULL,                 -- 实例端点
    
    -- 状态
    status              VARCHAR(30) NOT NULL DEFAULT 'active',
    
    -- 元数据
    metadata            JSONB,                         -- 实例元数据
    
    -- 健康检查
    last_health_check   TIMESTAMP WITH TIME ZONE,
    health_status       VARCHAR(30),
    
    -- 时间戳
    registered_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_seen_at        TIMESTAMP WITH TIME ZONE,
    
    -- 约束
    CONSTRAINT chk_instance_status CHECK (
        status IN ('active', 'inactive', 'starting', 'stopping', 'error')
    )
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
    id                  BIGSERIAL PRIMARY KEY,
    
    -- 关联插件
    plugin_id           VARCHAR(255) NOT NULL UNIQUE,  -- 插件 ID
    
    -- 权限策略
    policy              VARCHAR(30) NOT NULL DEFAULT 'strict', -- 权限策略
    
    -- 文件系统权限
    fs_paths            JSONB,                         -- 允许的文件路径
    fs_mode             VARCHAR(10),                   -- 文件系统模式
    
    -- 网络权限
    network_hosts       JSONB,                         -- 允许的主机
    network_ports       JSONB,                         -- 允许的端口
    
    -- 数据库权限
    database_ids        JSONB,                         -- 允许的数据库 ID
    database_operations JSONB,                         -- 允许的数据库操作
    
    -- 环境变量权限
    env_vars            JSONB,                         -- 允许的环境变量
    
    -- 系统调用权限
    syscalls            JSONB,                         -- 允许的系统调用
    
    -- 插件调用权限
    plugin_calls        JSONB,                         -- 允许调用的插件
    
    -- 白名单/黑名单
    whitelist           JSONB,                         -- 白名单
    blacklist           JSONB,                         -- 黑名单
    
    -- 元数据
    metadata            JSONB,                         -- 扩展元数据
    
    -- 时间戳
    created_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- 约束
    CONSTRAINT fk_permissions_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE,
    CONSTRAINT chk_permission_policy CHECK (
        policy IN ('strict', 'permissive', 'custom')
    )
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
-- 第四部分：存储过程
-- =====================================================

-- 清理过期的回滚记录
CREATE OR REPLACE FUNCTION cleanup_expired_rollbacks(
    p_days_to_keep INTEGER DEFAULT 30
)
RETURNS INTEGER AS $$
DECLARE
    v_deleted_count INTEGER;
BEGIN
    DELETE FROM cmx_plugin_rollback
    WHERE created_at < NOW() - (p_days_to_keep || ' days')::INTERVAL
    AND status IN ('completed', 'skipped', 'expired');
    
    GET DIAGNOSTICS v_deleted_count = ROW_COUNT;
    
    RETURN v_deleted_count;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION cleanup_expired_rollbacks IS '清理过期的回滚记录';

-- 清理旧的审计日志
CREATE OR REPLACE FUNCTION cleanup_old_audit_logs(
    p_days_to_keep INTEGER DEFAULT 90
)
RETURNS INTEGER AS $$
DECLARE
    v_deleted_count INTEGER;
BEGIN
    DELETE FROM cmx_plugin_audit_log
    WHERE started_at < NOW() - (p_days_to_keep || ' days')::INTERVAL;
    
    GET DIAGNOSTICS v_deleted_count = ROW_COUNT;
    
    RETURN v_deleted_count;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION cleanup_old_audit_logs IS '清理旧的审计日志';

-- 检查节点健康状态
CREATE OR REPLACE FUNCTION check_node_health(
    p_timeout_seconds INTEGER DEFAULT 60
)
RETURNS TABLE(
    node_id VARCHAR(100),
    node_name VARCHAR(255),
    status VARCHAR(30),
    seconds_since_heartbeat BIGINT,
    is_healthy BOOLEAN
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        n.node_id,
        n.node_name,
        n.status,
        EXTRACT(EPOCH FROM (NOW() - n.last_heartbeat))::BIGINT AS seconds_since_heartbeat,
        CASE 
            WHEN n.status = 'online' AND n.last_heartbeat > NOW() - (p_timeout_seconds || ' seconds')::INTERVAL 
            THEN TRUE 
            ELSE FALSE 
        END AS is_healthy
    FROM cmx_plugin_nodes n
    WHERE n.is_active = TRUE;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION check_node_health IS '检查节点健康状态';

-- 获取插件依赖树
CREATE OR REPLACE FUNCTION get_plugin_dependency_tree(
    p_plugin_id VARCHAR(255)
)
RETURNS TABLE(
    plugin_id VARCHAR(255),
    dependency_id VARCHAR(255),
    version_constraint VARCHAR(100),
    level INTEGER
) AS $$
WITH RECURSIVE dep_tree AS (
    -- 基础查询：获取直接依赖
    SELECT 
        d.plugin_id,
        d.dependency_plugin_id AS dependency_id,
        d.version_constraint,
        1 AS level
    FROM cmx_plugin_dependencies d
    WHERE d.plugin_id = p_plugin_id
    
    UNION ALL
    
    -- 递归查询：获取传递依赖
    SELECT 
        dt.dependency_id AS plugin_id,
        d.dependency_plugin_id AS dependency_id,
        d.version_constraint,
        dt.level + 1 AS level
    FROM dep_tree dt
    JOIN cmx_plugin_dependencies d ON d.plugin_id = dt.dependency_id
    WHERE dt.level < 10  -- 防止无限递归
)
SELECT * FROM dep_tree
ORDER BY level, plugin_id;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_plugin_dependency_tree IS '获取插件依赖树';

-- =====================================================
-- 第五部分：初始数据
-- =====================================================

-- 插入默认的系统插件配置示例（可根据实际需求修改）
-- INSERT INTO cmx_system_plugins (plugin_id, name, version, install_order, source_type, source_path)
-- VALUES 
--     ('core_auth', '核心认证模块', '1.0.0', 1, 'bundled', '/plugins/core_auth.zip'),
--     ('core_storage', '核心存储模块', '1.0.0', 2, 'bundled', '/plugins/core_storage.zip');

-- =====================================================
-- 第六部分：权限设置
-- =====================================================

-- 创建只读用户（可选）
-- CREATE ROLE cmx_plugin_readonly;
-- GRANT CONNECT ON DATABASE cmx TO cmx_plugin_readonly;
-- GRANT USAGE ON SCHEMA public TO cmx_plugin_readonly;
-- GRANT SELECT ON ALL TABLES IN SCHEMA public TO cmx_plugin_readonly;

-- 创建读写用户（可选）
-- CREATE ROLE cmx_plugin_readwrite;
-- GRANT CONNECT ON DATABASE cmx TO cmx_plugin_readwrite;
-- GRANT USAGE ON SCHEMA public TO cmx_plugin_readwrite;
-- GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cmx_plugin_readwrite;
-- GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO cmx_plugin_readwrite;

-- =====================================================
-- 完成提示
-- =====================================================

-- 输出完成信息
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
