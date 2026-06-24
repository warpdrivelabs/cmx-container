//! IAM 权限校验器 — IamChecker
//!
//! 实现 cmx_traits::iam::PermissionChecker trait，通过数据库 EXISTS 查询进行权限/角色校验。
//! 支持 Redis 缓存（可选）和熔断器降级。

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_traits::iam::{DataScope, PermissionChecker};
use cmx_core::model::cell::DataValue;
use tracing::{debug, warn};

use crate::circuit_breaker::CircuitBreaker;
use crate::config::{FailureMode, IamConfig};

/// IAM 权限校验器实现。
///
/// 通过数据库 `EXISTS` 查询进行权限/角色校验，支持 `system:all` 超级权限短路。
/// 可选启用 Redis 缓存（通过 cmx-buffer）和熔断器降级。
#[derive(Clone)]
pub struct IamChecker {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,

    /// 认证库 `db_id`。
    db_id: String,

    /// IAM 配置。
    config: IamConfig,

    /// Redis 缓存管理器（可选，未初始化时直查 DB）。
    cache: Option<Arc<cmx_buffer::cache::CacheManager>>,

    /// 熔断器。
    circuit_breaker: Arc<CircuitBreaker>,
}

impl IamChecker {
    /// 构造函数（不带缓存）。
    ///
    /// # Arguments
    ///
    /// * `mm` - 数据库管理器。
    /// * `config` - IAM 配置，用于确定认证库 `db_id` 及熔断器参数。
    ///
    /// # Returns
    ///
    /// 返回未启用 Redis 缓存的新 `IamChecker` 实例。
    pub async fn new(mm: Arc<DatabaseManager>, config: IamConfig) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            config.circuit_breaker_threshold,
            config.circuit_breaker_reset_secs,
        ));
        Self {
            mm,
            db_id,
            config,
            cache: None,
            circuit_breaker,
        }
    }

    /// 设置 Redis 缓存管理器（Builder 模式）。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    ///
    /// # Returns
    ///
    /// 返回启用了缓存的新 `IamChecker` 实例。
    pub fn with_cache(mut self, cache: Arc<cmx_buffer::cache::CacheManager>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 执行 EXISTS 查询，返回布尔值
    async fn exists_check(&self, sql: &str, params: Vec<DataValue>, label: &str) -> Result<bool, TraitError> {
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, label)
            .await
            .map_err(|e| TraitError::Internal(format!("权限查询失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let exists = dataset
            .iter()
            .next()
            .and_then(|row| {
                row.get(0).and_then(|v| match v {
                    cmx_core::model::cell::DataValue::Bool(b) => Some(*b),
                    cmx_core::model::cell::DataValue::Int(i) => Some(*i != 0),
                    _ => None,
                })
            })
            .unwrap_or(false);

        let _ = schema;
        Ok(exists)
    }

    /// 计算带随机抖动的 TTL（防雪崩）
    fn calc_ttl_with_jitter(&self) -> Duration {
        let base = self.config.permission_cache_ttl_secs;
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let jitter = (base as f64 * 0.1) as u64;
        let offset = now_nanos % (jitter * 2 + 1);
        let ttl = base + offset - jitter;
        Duration::from_secs(ttl.max(60))
    }

    /// 从 Redis 缓存读取用户权限列表
    async fn get_user_permissions_cached(&self, user_id: &str) -> Option<Vec<String>> {
        let cache = self.cache.as_ref()?;
        let key = format!("iam:perm:{}", user_id);
        let ops = cache.ops();
        match ops.get_deserialized::<Vec<String>>(&key).await {
            Ok(Some(perms)) => {
                debug!("缓存命中: {}", key);
                Some(perms)
            }
            Ok(None) => None,
            Err(e) => {
                warn!("缓存读取失败: {}, err: {}", key, e);
                None
            }
        }
    }

    /// 回填用户权限到 Redis 缓存
    async fn set_user_permissions_cache(&self, user_id: &str, perms: &[String]) {
        if let Some(cache) = &self.cache {
            let key = format!("iam:perm:{}", user_id);
            let ttl = if perms.is_empty() {
                Duration::from_secs(60) // 空结果短 TTL 防穿透
            } else {
                self.calc_ttl_with_jitter()
            };
            let json = serde_json::to_string(perms).unwrap_or_default();
            let ops = cache.ops();
            if let Err(e) = ops.set_ex(&key, &json, ttl).await {
                warn!("缓存回填失败: {}, err: {}", key, e);
            }
        }
    }

    /// 从 Redis 缓存读取用户角色列表
    async fn get_user_role_codes_cached(&self, user_id: &str) -> Option<Vec<String>> {
        let cache = self.cache.as_ref()?;
        let key = format!("iam:role:{}", user_id);
        let ops = cache.ops();
        match ops.get_deserialized::<Vec<String>>(&key).await {
            Ok(Some(roles)) => {
                debug!("角色缓存命中: {}", key);
                Some(roles)
            }
            Ok(None) => None,
            Err(e) => {
                warn!("角色缓存读取失败: {}, err: {}", key, e);
                None
            }
        }
    }

    /// 回填用户角色到 Redis 缓存
    async fn set_user_role_codes_cache(&self, user_id: &str, roles: &[String]) {
        if let Some(cache) = &self.cache {
            let key = format!("iam:role:{}", user_id);
            let ttl = if roles.is_empty() {
                Duration::from_secs(60)
            } else {
                self.calc_ttl_with_jitter()
            };
            let json = serde_json::to_string(roles).unwrap_or_default();
            let ops = cache.ops();
            if let Err(e) = ops.set_ex(&key, &json, ttl).await {
                warn!("角色缓存回填失败: {}, err: {}", key, e);
            }
        }
    }

    /// 失效指定用户的权限和角色缓存。
    ///
    /// 同时删除 `iam:perm:{user_id}` 和 `iam:role:{user_id}` 两个缓存键。
    /// 未启用缓存时为空操作。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    pub async fn invalidate_user_cache(&self, user_id: &str) {
        if let Some(cache) = &self.cache {
            let perm_key = format!("iam:perm:{}", user_id);
            let role_key = format!("iam:role:{}", user_id);
            let ops = cache.ops();
            let _ = ops.del_batch(&[&perm_key, &role_key]).await;
            debug!("已失效用户缓存: {}", user_id);
        }
    }

    /// 失效指定角色关联的所有用户缓存。
    ///
    /// 查询该角色关联的所有 `user_id`（含永久与临时授权），
    /// 批量删除这些用户的权限和角色缓存键。
    /// 未启用缓存时为空操作。
    ///
    /// # Arguments
    ///
    /// * `role_id` - 目标角色 ID。
    pub async fn invalidate_role_cache(&self, role_id: &str) {
        if let Some(cache) = &self.cache {
            // 查询该角色关联的所有 user_id（永久 + 临时）
            let sql = r#"
                SELECT DISTINCT user_id FROM cmx_user_role WHERE role_id = $1 AND archived = 0
                UNION
                SELECT DISTINCT user_id FROM cmx_user_role_assignment
                WHERE role_id = $1 AND archived = 0
            "#;
            let params = vec![DataValue::String(role_id.to_string())];
            let dataset = self
                .mm
                .query_sql_with_datavalues(&self.db_id, None, sql, params, "role_user_ids")
                .await;
            if let Ok(dataset) = dataset {
                let schema = dataset.schema.as_ref();
                let mut keys: Vec<String> = Vec::new();
                for row in dataset.iter() {
                    if let Some(uid) = row.get_by_name_as::<String>(schema, "user_id") {
                        keys.push(format!("iam:perm:{}", uid));
                        keys.push(format!("iam:role:{}", uid));
                    }
                }
                if !keys.is_empty() {
                    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
                    let ops = cache.ops();
                    let _ = ops.del_batch(&key_refs).await;
                    debug!("已失效角色 {} 关联的 {} 个用户缓存", role_id, keys.len() / 2);
                }
            }
        }
    }

    /// 熔断器降级处理
    async fn handle_circuit_open(&self, user_id: &str, permission_code: &str) -> Result<bool, TraitError> {
        match self.config.failure_mode {
            FailureMode::FailOpen => {
                // 故障开放：仅放行 system:all 用户（直接查 DB 单条权限）
                warn!("熔断器打开，FailOpen 模式：尝试直查 DB 验证 system:all, user={}", user_id);
                let system_all_sql = r#"
                    SELECT EXISTS(
                      SELECT 1 FROM cmx_permission p
                      INNER JOIN cmx_role_permission rp ON p.id = rp.permission_id
                      INNER JOIN cmx_user_role ur ON rp.role_id = ur.role_id
                      INNER JOIN cmx_role r ON r.id = ur.role_id
                      WHERE ur.user_id = $1 AND p.code = 'system:all' AND p.status = 1
                        AND ur.archived = 0 AND rp.archived = 0 AND p.archived = 0
                        AND r.archived = 0 AND r.status = 1
                    ) OR EXISTS(
                      SELECT 1 FROM cmx_permission p
                      INNER JOIN cmx_role_permission rp ON p.id = rp.permission_id
                      INNER JOIN cmx_user_role_assignment ura ON rp.role_id = ura.role_id
                      INNER JOIN cmx_role r ON r.id = ura.role_id
                      WHERE ura.user_id = $1 AND p.code = 'system:all' AND p.status = 1
                        AND ura.status = 1 AND ura.archived = 0 AND rp.archived = 0 AND p.archived = 0
                        AND r.archived = 0 AND r.status = 1
                        AND NOW() BETWEEN ura.effective_from AND ura.effective_until
                    )
                "#;
                let params = vec![DataValue::String(user_id.to_string())];
                // 直查 DB，失败时返回 Err（DB 也故障的实际效果）
                match self.exists_check(system_all_sql, params, "failopen_system_all").await {
                    Ok(has_all) => Ok(has_all),
                    Err(e) => {
                        warn!("FailOpen 直查 DB 也失败: {}", e);
                        Err(e)
                    }
                }
            }
            FailureMode::FailClose => {
                // 故障封闭：全部拒绝（不查 DB，保护系统）
                warn!("熔断器打开，FailClose 模式：拒绝权限请求 user={}, code={}", user_id, permission_code);
                Ok(false)
            }
        }
    }
}

#[async_trait]
impl PermissionChecker for IamChecker {
    async fn has_permission(
        &self,
        user_id: &str,
        permission_code: &str,
    ) -> Result<bool, TraitError> {
        debug!(
            "{:<12} - IamChecker::has_permission - user: {}, code: {}",
            "IAM", user_id, permission_code
        );

        // 0. 检查熔断器状态
        if !self.circuit_breaker.allow_request() {
            return self.handle_circuit_open(user_id, permission_code).await;
        }

        // 1. 尝试从缓存读取用户完整权限列表
        if let Some(cached_perms) = self.get_user_permissions_cached(user_id).await {
            self.circuit_breaker.record_success();
            return Ok(cached_perms.iter().any(|p| p == "system:all" || p == permission_code));
        }

        // 2. 缓存未命中，查询 DB 获取用户完整权限列表（合并永久+临时授权）
        match self.get_user_permissions(user_id).await {
            Ok(perms) => {
                self.circuit_breaker.record_success();
                // 回填缓存（完整权限列表，空结果用短 TTL 防穿透）
                self.set_user_permissions_cache(user_id, &perms).await;
                Ok(perms.iter().any(|p| p == "system:all" || p == permission_code))
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(e)
            }
        }
    }

    async fn has_role(&self, user_id: &str, role_code: &str) -> Result<bool, TraitError> {
        debug!(
            "{:<12} - IamChecker::has_role - user: {}, code: {}",
            "IAM", user_id, role_code
        );

        // 0. 检查熔断器状态
        if !self.circuit_breaker.allow_request() {
            // 熔断器打开，FailClose 模式拒绝，FailOpen 模式直查 DB
            return match self.config.failure_mode {
                FailureMode::FailClose => Ok(false),
                FailureMode::FailOpen => {
                    warn!("熔断器打开，FailOpen 模式：has_role 直查 DB, user={}", user_id);
                    let roles = self.get_user_role_codes(user_id).await?;
                    Ok(roles.iter().any(|r| r == role_code))
                }
            };
        }

        // 1. 尝试从缓存读取用户角色列表
        if let Some(cached_roles) = self.get_user_role_codes_cached(user_id).await {
            self.circuit_breaker.record_success();
            return Ok(cached_roles.iter().any(|r| r == role_code));
        }

        // 2. 缓存未命中，查 DB 并回填缓存
        match self.get_user_role_codes(user_id).await {
            Ok(roles) => {
                self.circuit_breaker.record_success();
                self.set_user_role_codes_cache(user_id, &roles).await;
                Ok(roles.iter().any(|r| r == role_code))
            }
            Err(e) => {
                self.circuit_breaker.record_failure();
                Err(e)
            }
        }
    }

    async fn get_user_permissions(&self, user_id: &str) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - IamChecker::get_user_permissions - user: {}",
            "IAM", user_id
        );

        // 合并永久角色权限与临时角色权限
        let sql = r#"
            SELECT DISTINCT p.code
            FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            INNER JOIN cmx_user_role ur ON ur.role_id = rp.role_id
            INNER JOIN cmx_role r ON r.id = ur.role_id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND rp.archived = 0
              AND p.archived = 0 AND p.status = 1
              AND r.archived = 0 AND r.status = 1

            UNION

            SELECT DISTINCT p.code
            FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            INNER JOIN cmx_user_role_assignment ura ON ura.role_id = rp.role_id
            INNER JOIN cmx_role r ON r.id = ura.role_id
            WHERE ura.user_id = $1
              AND ura.status = 1
              AND ura.archived = 0
              AND NOW() BETWEEN ura.effective_from AND ura.effective_until
              AND rp.archived = 0
              AND p.archived = 0 AND p.status = 1
              AND r.archived = 0 AND r.status = 1
        "#;
        let params = vec![DataValue::String(user_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_permissions")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户权限失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let permissions: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "code"))
            .collect();

        Ok(permissions)
    }

    async fn get_user_role_codes(&self, user_id: &str) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - IamChecker::get_user_role_codes - user: {}",
            "IAM", user_id
        );

        // 合并永久角色与临时有效角色
        let sql = r#"
            SELECT DISTINCT r.code
            FROM cmx_role r
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1

            UNION

            SELECT DISTINCT r.code
            FROM cmx_role r
            INNER JOIN cmx_user_role_assignment ura ON r.id = ura.role_id
            WHERE ura.user_id = $1
              AND ura.status = 1
              AND ura.archived = 0
              AND NOW() BETWEEN ura.effective_from AND ura.effective_until
              AND r.archived = 0 AND r.status = 1
        "#;
        let params = vec![DataValue::String(user_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_role_codes")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户角色失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let roles: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "code"))
            .collect();

        Ok(roles)
    }

    /// 获取用户的数据权限范围（默认返回 All，待后续实现）
    async fn get_data_scope(&self, _user_id: &str) -> Result<DataScope, TraitError> {
        Ok(DataScope::All)
    }
}
