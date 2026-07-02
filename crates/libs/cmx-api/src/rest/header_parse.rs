use axum::http::HeaderMap;
use cmx_database::get_default_db_manager;

/// 从请求头中提取 `db_id`，如果不存在则返回默认数据库 ID。
///
/// # 参数
/// - `header`: HTTP 请求头的引用（`HeaderMap`）。
///
/// # 返回值
/// 返回一个 `String`，表示要使用的数据库 ID：
/// - 如果请求头中包含有效的 `db_id`，则使用该值；
/// - 否则，异步获取并返回默认数据库 ID。
///
/// # 错误处理
/// 如果 `db_id` 头存在但包含非法字符（非 ASCII 或无效 UTF-8），
/// 则会回退到默认数据库 ID（不会 panic）。
pub async fn get_db_id_from_header(header: &HeaderMap) -> String {
    if let Some(db_id_header) = header.get("db_id") {
        // 将 HeaderValue 转换为 &str（可能失败，例如包含非 UTF-8 字节）
        match db_id_header.to_str() {
            Ok(db_id_str) => {
                // 成功解析，返回副本（转换为 String）
                return db_id_str.to_owned();
            }
            Err(_) => {
                // 记录警告日志（如使用 tracing 或 log crate）
                tracing::error!("Invalid UTF-8 in 'db_id' header, falling back to default.");
                return get_default_db_manager().get_default_db_id().await;
            }
        }
    }

    // 如果头不存在或解析失败，则获取默认数据库 ID
    get_default_db_manager().get_default_db_id().await
}
