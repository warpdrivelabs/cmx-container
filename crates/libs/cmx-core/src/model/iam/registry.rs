//! 权限注册表 - 编译期 inventory 收集
//!
//! 通过 inventory crate 在编译期将所有权限定义和路由处理器登记到全局集合,
//! 启动期用于:① 漏写告警 ② DB 一致性校验 ③ 权限元数据查询。

use serde::Serialize;

/// 编译期注册的权限定义
pub struct RegisteredPermission {
    pub key: &'static str,
    pub group: &'static str,
    pub display: &'static str,
    pub description: &'static str,
    pub source: &'static str,
}
inventory::collect!(RegisteredPermission);

/// 编译期注册的路由处理器登记(漏写告警用)
///
/// 注:不做路由→权限自动映射(职责分离),
/// 此结构仅用于统计已注解 handler 数量,与路由总数粗略比对。
pub struct RegisteredRouteHandler {
    pub handler_name: &'static str,
    pub is_public: bool,
    pub source: &'static str,
}
inventory::collect!(RegisteredRouteHandler);

/// 获取所有已注册权限
pub fn all_registered_permissions() -> Vec<&'static RegisteredPermission> {
    inventory::iter::<RegisteredPermission>().collect()
}

/// 获取所有已注册路由处理器
pub fn all_registered_handlers() -> Vec<&'static RegisteredRouteHandler> {
    inventory::iter::<RegisteredRouteHandler>().collect()
}

/// 权限元数据信息(查询用)
#[derive(Debug, Clone, Serialize)]
pub struct PermissionInfo {
    pub code: String,
    pub group: String,
    pub display: String,
    pub description: String,
    pub source: String,
}

/// 内存权限注册表 - 供前端管理界面/运维查询 inventory 中的权限元数据
pub struct PermissionRegistry;

impl PermissionRegistry {
    /// 返回所有编译期注册的权限定义(含 group/display/description 元数据)
    pub fn list_all() -> Vec<PermissionInfo> {
        all_registered_permissions()
            .iter()
            .map(|p| PermissionInfo {
                code: p.key.to_string(),
                group: p.group.to_string(),
                display: p.display.to_string(),
                description: p.description.to_string(),
                source: p.source.to_string(),
            })
            .collect()
    }
}
