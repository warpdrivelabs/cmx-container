/*
 * @Author: yqs
 * @Date: 2026-03-18 10:16:55
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-18 11:29:09
 */
//! ID 生成模块
//!
//! 提供 UUID 和雪花算法 ID 生成功能。

mod pk52;
mod snowflake;
mod uuid_gen;

use rand::Rng;
use std::sync::OnceLock;

pub use pk52::Pk52Generator;
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

static PK52_GENERATOR: OnceLock<Pk52Generator> = OnceLock::new();

/// 全局 52 位主键生成器（DCT/DOC 的 `bigint id` 列专用）。
///
/// node id 解析顺序（首次调用时确定，之后固定）：
///   1. 配置 `id_gen.node_id`（推荐，`dev-local.toml` 注入，每部署唯一分配）——
///      这是「导出包直插他库不撞、不改号」的根基。
///   2. 配置缺失 → 随机 node（0..=255）。仍全局唯一（不撞），但无法跨部署稳定追溯来源，
///      并在日志告警提示应显式配置。
fn get_pk52_generator() -> &'static Pk52Generator {
    PK52_GENERATOR.get_or_init(|| {
        // 配置未初始化时 try_global() 返回 None，绝不 panic（id 生成永不阻断保存）。
        let configured = crate::ConfigManager::try_global()
            .and_then(|cfg| cfg.get_int("id_gen.node_id").ok());
        match configured {
            Some(n) => Pk52Generator::new(n),
            None => {
                let mut rng = rand::thread_rng();
                let node_id = rng.gen_range(0..256) as i64;
                tracing::warn!(
                    node_id,
                    "id_gen.node_id 未配置，PK52 生成器回落随机 node（仍全局唯一，但跨部署无法稳定追溯来源，建议在配置中显式分配）"
                );
                Pk52Generator::new(node_id)
            }
        }
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

/// 生成 52 位 JS 安全主键 id（DCT/DOC 的 `bigint id` 列专用，全局单例）。
///
/// 与 [`snowflake_id`]（63 位，值超 JS `2^53`，须字符串承载）不同：本函数返回值
/// **始终 < 2^53-1**，可安全作为 JSON number 传给 JS 前端而不丢精度。
///
/// # 返回值
/// * `i64` - 52 位主键 id（正数、时间有序、跨部署不重叠）
pub fn next_pk_id() -> i64 {
    get_pk52_generator().next_id()
}
