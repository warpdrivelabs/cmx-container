//! 数据源启动校验（五引擎 main.rs `init("datasources")` 钩子内联校验的共享单源）。
//!
//! 纯检查（不注册、不连库）——注册与建池首连验证见 [`crate::register_pg_datasources`]。
//! 各引擎按自己的语义声明 [`DatasourceRules`]：
//!
//! | 引擎 | required_db_ids | require_default | require_biz |
//! | --- | --- | --- | --- |
//! | flow | `["fico-db", "primary"]` | ✓ | — |
//! | rules | `["rule_pg"]` | — | — |
//! | report | `["fico-db"]` | — | — |
//! | model | — | ✓ | ✓ |
//! | mdm | — | ✓（补齐：原内联校验漏查，注释与代码脱节） | ✓ |

use cmx_database_pg::DbConfig;

use crate::{BaseError, Result};

/// 数据源校验规则集。
#[derive(Debug, Clone, Default)]
pub struct DatasourceRules {
    /// 必须齐备的 db_id 清单（引擎按常量寻址；如 flow 的 `["fico-db", "primary"]`）。
    pub required_db_ids: &'static [&'static str],
    /// 是否必须有 `default=true` 平台主库。
    pub require_default: bool,
    /// 是否必须有 `source_type="biz"` 业务库。
    pub require_biz: bool,
}

/// 校验 `[[databases]]` 配置满足规则集。任一不满足返回 [`BaseError::Config`]（含缺失项）。
pub fn validate_databases(configs: &[DbConfig], rules: &DatasourceRules) -> Result<()> {
    if configs.is_empty() {
        return Err(BaseError::Config(
            "[[databases]] 段为空：未配置任何数据源".to_string(),
        ));
    }
    if rules.require_default && !configs.iter().any(|d| d.default) {
        return Err(BaseError::Config(
            "[[databases]] 缺少 default=true 平台主库".to_string(),
        ));
    }
    if rules.require_biz
        && !configs
            .iter()
            .any(|d| d.source_type.as_deref() == Some("biz"))
    {
        return Err(BaseError::Config(
            "[[databases]] 缺少 source_type=\"biz\" 业务库".to_string(),
        ));
    }
    for id in rules.required_db_ids {
        if !configs.iter().any(|d| d.db_id == *id) {
            return Err(BaseError::Config(format!(
                "[[databases]] 缺少 db_id=\"{id}\"（引擎按常量寻址）"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(db_id: &str, default: bool, source_type: Option<&str>) -> DbConfig {
        // 只填校验涉及的字段（serde default 兜底其余）。
        let raw = format!(
            r#"{{"db_id":"{db_id}","db_name":"t","db_type":"postgres","db_url":"postgres://u:p@h:5432/{db_id}","default":{default},"source_type":{}}}"#,
            source_type
                .map(|s| format!("\"{s}\""))
                .unwrap_or_else(|| "null".to_string())
        );
        serde_json::from_str(&raw).expect("测试 DbConfig 构造")
    }

    #[test]
    fn empty_rejected() {
        let e = validate_databases(&[], &DatasourceRules::default()).unwrap_err();
        assert!(e.to_string().contains("段为空"));
    }

    #[test]
    fn default_and_biz_rules() {
        let dbs = vec![cfg("primary", true, None), cfg("fico-db", false, Some("biz"))];
        assert!(validate_databases(&dbs, &DatasourceRules { require_default: true, require_biz: true, ..Default::default() }).is_ok());
        // 缺 default
        let dbs2 = vec![cfg("fico-db", false, Some("biz"))];
        assert!(validate_databases(&dbs2, &DatasourceRules { require_default: true, ..Default::default() }).is_err());
        // 缺 biz
        let dbs3 = vec![cfg("primary", true, None)];
        assert!(validate_databases(&dbs3, &DatasourceRules { require_biz: true, ..Default::default() }).is_err());
    }

    #[test]
    fn required_db_ids() {
        let dbs = vec![cfg("fico-db", true, None)];
        assert!(validate_databases(
            &dbs,
            &DatasourceRules { required_db_ids: &["fico-db", "primary"], ..Default::default() }
        )
        .is_err());
        assert!(validate_databases(
            &dbs,
            &DatasourceRules { required_db_ids: &["fico-db"], ..Default::default() }
        )
        .is_ok());
    }
}
