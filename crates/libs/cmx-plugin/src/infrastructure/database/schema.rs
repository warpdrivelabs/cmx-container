//! 数据库表结构定义模块
//!
//! 定义插件系统所需的数据库表结构

/// 数据库表结构管理器
pub struct SchemaManager;

impl SchemaManager {
    /// 获取创建插件系统表的SQL
    pub fn get_create_system_tables_sql() -> Vec<&'static str> {
        vec![
            include_str!("../../../sql/plugin_lifecycle_schema.sql"),
        ]
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
