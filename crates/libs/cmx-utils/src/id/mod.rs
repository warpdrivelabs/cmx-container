/*
 * @Author: yqs
 * @Date: 2026-03-18 10:16:55
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-18 10:53:35
 */
//! ID 生成模块
//!
//! 提供 UUID 和雪花算法 ID 生成功能。

mod snowflake;
mod uuid_gen;

use rand::Rng;
use std::sync::OnceLock;

pub use snowflake::SnowflakeGenerator;
pub use uuid_gen::UuidGenerator;

static SNOWFLAKE_GENERATOR: OnceLock<SnowflakeGenerator> = OnceLock::new();

fn get_snowflake_generator() -> &'static SnowflakeGenerator {
    SNOWFLAKE_GENERATOR.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let node_id = rng.gen_range(0..1024) as i64;
        SnowflakeGenerator::new(node_id)
    })
}

/// 生成雪花 ID 字符串（全局单例）
///
/// # 返回值
/// * `String` - 雪花 ID 字符串
///
/// # 示例
/// ```rust
/// use cmx_utils::id::snowflake_id;
/// let id = snowflake_id();
/// println!("{}", id);
/// ```
pub fn snowflake_id_str() -> String {
    get_snowflake_generator().next_id_str()
}

/// 生成雪花 ID（全局单例）
///
/// # 返回值
/// * `i64` - 雪花 ID
pub fn snowflake_id() -> i64 {
    get_snowflake_generator().next_id()
}
