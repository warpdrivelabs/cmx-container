//! UUID 生成器模块
//!
//! 提供多种 UUID 生成功能，包括 UUID v4（随机）等。

use uuid::Uuid;

/// UUID 生成器
///
/// 提供生成不同版本和格式 UUID 的功能。
pub struct UuidGenerator;

impl UuidGenerator {
    /// 生成 UUID v4（随机 UUID）
    ///
    /// # 返回值
    /// * `Uuid` - 随机生成的 UUID
    ///
    /// # 示例
    /// ```rust
    /// use cmx_utils::id::UuidGenerator;
    /// let id = UuidGenerator::new_v4();
    /// println!("{}", id);
    /// ```
    pub fn new_v4() -> Uuid {
        Uuid::new_v4()
    }

    /// 生成 UUID v4 并转换为带连字符的标准字符串格式
    ///
    /// # 返回值
    /// * `String` - 格式化的 UUID 字符串 (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)
    ///
    /// # 示例
    /// ```rust
    /// use cmx_utils::id::UuidGenerator;
    /// let id_str = UuidGenerator::new_v4_str();
    /// println!("{}", id_str);
    /// ```
    pub fn new_v4_str() -> String {
        Uuid::new_v4().to_string()
    }

    /// 生成 UUID v4 并转换为不带连字符的紧凑字符串格式
    ///
    /// # 返回值
    /// * `String` - 紧凑格式的 UUID 字符串 (xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx)
    ///
    /// # 示例
    /// ```rust
    /// use cmx_utils::id::UuidGenerator;
    /// let id_str = UuidGenerator::new_v4_compact();
    /// println!("{}", id_str);
    /// ```
    pub fn new_v4_compact() -> String {
        Uuid::new_v4().simple().to_string()
    }

    /// 生成 UUID v4 并转换为 Base64 编码格式
    ///
    /// # 返回值
    /// * `String` - Base64 编码的 UUID 字符串
    ///
    /// # 示例
    /// ```rust
    /// use cmx_utils::id::UuidGenerator;
    /// let id_str = UuidGenerator::new_v4_base64();
    /// println!("{}", id_str);
    /// ```
    pub fn new_v4_base64() -> String {
        let uuid = Uuid::new_v4();
        let bytes = uuid.as_bytes();
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    }

    /// 生成多个 UUID v4
    ///
    /// # 参数
    /// * `count` - 需要生成的 UUID 数量
    ///
    /// # 返回值
    /// * `Vec<Uuid>` - UUID 向量
    ///
    /// # 示例
    /// ```rust
    /// use cmx_utils::id::UuidGenerator;
    /// let ids = UuidGenerator::new_v4_batch(5);
    /// for id in ids {
    ///     println!("{}", id);
    /// }
    /// ```
    pub fn new_v4_batch(count: usize) -> Vec<Uuid> {
        (0..count).map(|_| Uuid::new_v4()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_v4() {
        let id = UuidGenerator::new_v4();
        assert_eq!(id.get_version(), Some(uuid::Version::Random));
    }

    #[test]
    fn test_new_v4_str() {
        let id_str = UuidGenerator::new_v4_str();
        assert_eq!(id_str.len(), 36);
        assert!(id_str.contains('-'));
    }

    #[test]
    fn test_new_v4_compact() {
        let id_str = UuidGenerator::new_v4_compact();
        assert_eq!(id_str.len(), 32);
        assert!(!id_str.contains('-'));
    }

    #[test]
    fn test_new_v4_base64() {
        let id_str = UuidGenerator::new_v4_base64();
        assert!(!id_str.is_empty());
    }

    #[test]
    fn test_new_v4_batch() {
        let ids = UuidGenerator::new_v4_batch(10);
        assert_eq!(ids.len(), 10);
        // 验证所有 UUID 都是唯一的
        let mut unique_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for id in &ids {
            unique_ids.insert(id.to_string());
        }
        assert_eq!(unique_ids.len(), 10);
    }
}
