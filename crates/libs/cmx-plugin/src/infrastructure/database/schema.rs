//! 数据库表结构定义模块
//! 
//! 定义插件系统所需的数据库表结构

/// 数据库表结构管理器
pub struct SchemaManager;

impl SchemaManager {
    /// 获取创建插件系统表的SQL
    pub fn get_create_system_tables_sql() -> Vec<&'static str> {
        vec![
            include_str!("../../../../../../.trae/documents/plugin_lifecycle_schema.sql"),
        ]
    }
    
    /// 获取创建插件功能表的SQL
    pub fn get_create_features_table_sql() -> &'static str {
        r#"
CREATE TABLE IF NOT EXISTS cmx_plugin_features (
    id                  VARCHAR(64) NOT NULL,
    plugin_id           VARCHAR(64) NOT NULL,
    feature_id          VARCHAR(255) NOT NULL,
    feature_name        VARCHAR(500) NOT NULL,
    feature_type        VARCHAR(50) NOT NULL,
    description         TEXT,
    config              JSONB,
    status              VARCHAR(30) NOT NULL DEFAULT 'active',
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_plugin_features_plugin FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(id) ON DELETE CASCADE
);

COMMENT ON COLUMN cmx_plugin_features.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_features.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_features.feature_id IS '功能唯一标识';
COMMENT ON COLUMN cmx_plugin_features.feature_name IS '功能名称';
COMMENT ON COLUMN cmx_plugin_features.feature_type IS '功能类型: service, event_handler, scheduler, api';
COMMENT ON COLUMN cmx_plugin_features.description IS '功能描述';
COMMENT ON COLUMN cmx_plugin_features.config IS '功能配置';
COMMENT ON COLUMN cmx_plugin_features.status IS '状态: active, inactive, error';
"#
    }
    
    /// 获取创建插件事件表的SQL
    pub fn get_create_events_table_sql() -> &'static str {
        r#"
CREATE TABLE IF NOT EXISTS cmx_plugin_events (
    id                  VARCHAR(64) NOT NULL,
    plugin_id           VARCHAR(64) NOT NULL,
    event_type          VARCHAR(100) NOT NULL,
    event_data          JSONB,
    processed           BOOLEAN NOT NULL DEFAULT FALSE,
    processed_at        TIMESTAMP WITH TIME ZONE,
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id),
    CONSTRAINT fk_plugin_events_plugin FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(id) ON DELETE CASCADE
);

COMMENT ON COLUMN cmx_plugin_events.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_events.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_events.event_type IS '事件类型';
COMMENT ON COLUMN cmx_plugin_events.event_data IS '事件数据';
COMMENT ON COLUMN cmx_plugin_events.processed IS '是否已处理';
COMMENT ON COLUMN cmx_plugin_events.processed_at IS '处理时间';
"#
    }
}
