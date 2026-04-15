//! 数据库表结构定义模块
//!
//! 定义插件系统所需的数据库表结构

/// 数据库表结构管理器
pub struct SchemaManager;

impl SchemaManager {
    /// 获取创建插件系统表的SQL
    pub fn get_create_system_tables_sql() -> Vec<&'static str> {
        vec![
            // include_str!("../../../sql/plugin_lifecycle_schema.sql"),
        ]
    }

}
