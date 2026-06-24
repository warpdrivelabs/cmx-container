//! 认证服务初始化
//!
//! 初始化 AuthServiceImpl 并返回 Arc<dyn AuthService>。

use std::sync::Arc;

use cmx_auth::{AuthConfig, AuthServiceImpl, SuperAdminConfig, StaticApiKeyConfig};
use cmx_buffer::GlobalCacheManager;
use cmx_buffer::GlobalSubscriberManager;
use cmx_traits::auth::{AuthService, UserAuthQuery};
use tracing::{info, warn};

/// 初始化认证服务
///
/// 从配置文件加载 AuthConfig，创建 AuthServiceImpl 实例。
/// 返回 Arc<dyn AuthService> 供注入 CmxAppState。
///
/// # 参数
/// * `user_query` - 用户认证查询实现（由 IAM 模块创建并共享）
/// * `audit_logger` - 审计日志器，注入到 `AuthServiceImpl`
pub async fn init_auth_service(
    user_query: Arc<dyn UserAuthQuery>,
    audit_logger: Arc<dyn cmx_audit::AuditLogger>,
) -> Result<Arc<dyn AuthService>, crate::error::Error> {
    // 1. 加载 AuthConfig（从配置文件或使用默认值）
    let auth_config = load_auth_config();

    // 1.5 初始化全局认证白名单（合并内置白名单 + TOML 自定义白名单）
    // 必须在 mw_auth 中间件被首次请求前完成初始化
    if let Err(e) = cmx_api::middleware::GlobalAuthService::initialize_whitelist(
        auth_config.whitelist.clone(),
    ) {
        warn!("全局认证白名单初始化失败: {}", e);
    }

    // 2. 获取 CacheManager（解引用 Arc）
    let cache = (**GlobalCacheManager::get()).clone();

    // 3. 使用外部传入的 UserAuthQuery 实现（由 IAM 模块创建并共享）
    // user_query 已通过参数传入，无需在此创建

    // 4. 创建 AuthServiceImpl
    // 4.1 初始化第三方 OAuth2 Provider 注册表（在创建 AuthServiceImpl 之前，因为 auth_config 会被 move）
    if let Some(ref oauth2_config) = auth_config.oauth2
        && !oauth2_config.providers.is_empty() {
            let mut registry = cmx_auth::oauth2::OAuth2ProviderRegistry::new();
            for provider_config in &oauth2_config.providers {
                if !provider_config.enabled {
                    continue;
                }
                let provider: std::sync::Arc<dyn cmx_auth::oauth2::provider::OAuth2Provider> = match provider_config.provider_type.as_str() {
                    "google" => std::sync::Arc::new(
                        cmx_auth::oauth2::provider::google::GoogleProvider::new(provider_config.clone())
                    ),
                    "github" => std::sync::Arc::new(
                        cmx_auth::oauth2::provider::github::GitHubProvider::new(provider_config.clone())
                    ),
                    _ => {
                        // 校验 generic 类型必需字段
                        if provider_config.authorize_url.is_empty() {
                            warn!("Provider '{}' (generic) 缺少 authorize_url 配置，跳过", provider_config.name);
                            continue;
                        }
                        if provider_config.token_url.is_empty() {
                            warn!("Provider '{}' (generic) 缺少 token_url 配置，跳过", provider_config.name);
                            continue;
                        }
                        if provider_config.userinfo_url.is_empty() {
                            warn!("Provider '{}' (generic) 缺少 userinfo_url 配置，跳过", provider_config.name);
                            continue;
                        }
                        std::sync::Arc::new(
                            cmx_auth::oauth2::provider::generic::GenericOAuth2Provider::new(provider_config.clone())
                        )
                    }
                };
                registry.register(provider);
            }

            cmx_api::middleware::GlobalAuthService::initialize_provider_registry(registry)
                .map_err(crate::error::Error::ServerSetup)?;

            info!("第三方 OAuth2 Provider 注册表初始化完成");
        }

    // 4.2 创建 AuthServiceImpl 并注入审计日志器
    let auth_service_impl = AuthServiceImpl::new(cache, auth_config, user_query)
        .map_err(|e| crate::error::Error::ServerSetup(format!("认证服务初始化失败: {}", e)))?
        .with_audit_logger(audit_logger);

    // 4.5 2.6 修复：初始化 Prometheus 指标（注册到全局默认注册表）
    if let Err(e) = cmx_auth::metrics::init_metrics() {
        warn!("Prometheus 指标初始化失败: {}", e);
    }

    // 5. 注册全局 OAuth2 策略（供 OAuth2 handler 使用）
    // 5.6 修复：从 AuthServiceImpl 获取已创建的 OAuth2Policy，避免重复创建
    let oauth2_policy = Arc::new(auth_service_impl.oauth2_policy().clone());
    cmx_api::middleware::GlobalAuthService::initialize_oauth2(oauth2_policy)
        .map_err(crate::error::Error::ServerSetup)?;

    let auth_service: Arc<dyn AuthService> = Arc::new(auth_service_impl);

    // 6. 注册全局认证服务（供 mw_auth 中间件使用）
    cmx_api::middleware::GlobalAuthService::initialize(auth_service.clone())
        .map_err(crate::error::Error::ServerSetup)?;

    // 7. 注册 Pub/Sub 订阅（缓存失效回调）
    if GlobalSubscriberManager::is_initialized() {
        let subscriber = GlobalSubscriberManager::get();
        let auth_svc = auth_service.clone();
        subscriber.register_channel_fn("auth:cache:invalidate", move |_channel, payload| {
            let auth_svc = auth_svc.clone();
            let payload = payload.to_string();
            tokio::spawn(async move {
                auth_svc.invalidate_local_cache(&payload).await;
            });
        }).await.map_err(|e| crate::error::Error::ServerSetup(format!("Pub/Sub 订阅注册失败: {}", e)))?;
        info!("Pub/Sub 缓存失效订阅已注册");
    }

    // 8. 确保超管账号存在
    if let Err(e) = auth_service.ensure_super_admin().await {
        warn!("超管初始化失败: {}", e);
    }

    // 9. 导入静态 API Key
    if let Err(e) = auth_service.import_static_api_keys().await {
        warn!("静态 API Key 导入失败: {}", e);
    }

    // 10. 启动过期会话定时清理任务
    auth_service.start_cleanup_task().await;

    info!("认证服务初始化完成");
    Ok(auth_service)
}

/// 从配置文件加载 AuthConfig
fn load_auth_config() -> AuthConfig {
    // 尝试从配置管理器加载 JWT secret
    let config = cmx_utils::ConfigManager::global();

    let mut auth_config = AuthConfig::default();

    // JWT 配置
    if let Ok(algorithm) = config.get_string("auth.jwt.algorithm") {
        auth_config.jwt.algorithm = algorithm;
    }
    if let Ok(secret) = config.get_string("auth.jwt.secret") {
        auth_config.jwt.secret = Some(secret);
    }
    if let Ok(issuer) = config.get_string("auth.jwt.issuer") {
        auth_config.jwt.issuer = issuer;
    }
    if let Ok(audience) = config.get_string("auth.jwt.audience") {
        auth_config.jwt.audience = audience;
    }
    if let Ok(private_key) = config.get_string("auth.jwt.private_key") {
        auth_config.jwt.private_key = Some(private_key);
    }
    if let Ok(public_key) = config.get_string("auth.jwt.public_key") {
        auth_config.jwt.public_key = Some(public_key);
    }
    if let Ok(current_kid) = config.get_string("auth.jwt.current_kid") {
        auth_config.jwt.current_kid = Some(current_kid);
    }

    // JWT 旧密钥列表（密钥轮换宽限期）
    // 2.5 修复：config crate 数组索引使用方括号语法 [0] 而非点号 .0.
    // 格式: auth.jwt.legacy_public_keys[0].kid / auth.jwt.legacy_public_keys[0].pem
    // 简化实现：支持最多 5 个旧密钥对
    let mut legacy_keys = Vec::new();
    for i in 0..5 {
        let kid_key = format!("auth.jwt.legacy_public_keys[{}].kid", i);
        let pem_key = format!("auth.jwt.legacy_public_keys[{}].pem", i);
        if let (Ok(kid), Ok(pem)) = (config.get_string(&kid_key), config.get_string(&pem_key)) {
            legacy_keys.push((kid, pem));
        }
    }
    if !legacy_keys.is_empty() {
        auth_config.jwt.legacy_public_keys = legacy_keys;
    }

    // Token 过期配置
    if let Ok(ttl) = config.get_int("auth.token.access_ttl_secs") {
        auth_config.token.access_ttl_secs = ttl as u64;
    }
    if let Ok(ttl) = config.get_int("auth.token.refresh_ttl_secs") {
        auth_config.token.refresh_ttl_secs = ttl as u64;
    }

    // Argon2 配置
    if let Ok(memory_cost) = config.get_int("auth.argon2.memory_cost") {
        auth_config.argon2.memory_cost = memory_cost as u32;
    }
    if let Ok(time_cost) = config.get_int("auth.argon2.time_cost") {
        auth_config.argon2.time_cost = time_cost as u32;
    }
    if let Ok(parallelism) = config.get_int("auth.argon2.parallelism") {
        auth_config.argon2.parallelism = parallelism as u32;
    }

    // Session 配置
    if let Ok(single) = config.get_bool("auth.session.single_session_per_device_type") {
        auth_config.session.single_session_per_device_type = single;
    }
    if let Ok(max) = config.get_int("auth.session.max_sessions") {
        auth_config.session.max_sessions = max as usize;
    }
    if let Ok(idle) = config.get_int("auth.session.idle_timeout_secs") {
        auth_config.session.idle_timeout_secs = idle as u64;
    }
    if let Ok(interval) = config.get_int("auth.session.heartbeat_interval_secs") {
        auth_config.session.heartbeat_interval_secs = interval as u64;
    }

    // 缓存配置
    if let Ok(enabled) = config.get_bool("auth.cache.enable_local_cache") {
        auth_config.cache.enable_local_cache = enabled;
    }
    if let Ok(ttl) = config.get_int("auth.cache.local_ttl_secs") {
        auth_config.cache.local_ttl_secs = ttl as u64;
    }
    if let Ok(max) = config.get_int("auth.cache.local_cache_max_entries") {
        auth_config.cache.local_cache_max_entries = max as u64;
    }
    if let Ok(attempts) = config.get_int("auth.cache.max_login_attempts") {
        auth_config.cache.max_login_attempts = attempts as u32;
    }
    if let Ok(secs) = config.get_int("auth.cache.lock_duration_secs") {
        auth_config.cache.lock_duration_secs = secs as u64;
    }



    // 确保 [auth.oauth2] 节存在时初始化 oauth2 配置
    // 当 auth_code_ttl_secs / pkce_required 被注释时，上面的 get_or_insert_with 不会执行，
    // 但 [auth.oauth2] 节下的其他配置（providers、frontend_callback_url 等）仍需加载
    if auth_config.oauth2.is_none() && config.inner().get_table("auth.oauth2").is_ok() {
        auth_config.oauth2 = Some(Default::default());
    }

    // OAuth2 配置
    if let Ok(ttl) = config.get_int("auth.oauth2.auth_code_ttl_secs") {
        auth_config.oauth2.get_or_insert_with(Default::default).auth_code_ttl_secs = ttl as u64;
    }
    if let Ok(pkce) = config.get_bool("auth.oauth2.pkce_required") {
        auth_config.oauth2.get_or_insert_with(Default::default).pkce_required = pkce;
    }

    // OAuth2 Provider 配置
    if let Some(oauth2) = auth_config.oauth2.as_mut() {

        // 读取 providers 数组
        let mut providers = Vec::new();
        for i in 0..10 {
            let name_key = format!("auth.oauth2.providers[{}].name", i);
            if let Ok(name) = config.get_string(&name_key) {
                let provider_type = config.get_string(&format!("auth.oauth2.providers[{}].provider_type", i))
                    .unwrap_or_else(|_| "generic".to_string());
                let client_id = config.get_string(&format!("auth.oauth2.providers[{}].client_id", i))
                    .unwrap_or_default();
                let client_secret = config.get_string(&format!("auth.oauth2.providers[{}].client_secret", i))
                    .unwrap_or_default();
                let redirect_uri = config.get_string(&format!("auth.oauth2.providers[{}].redirect_uri", i))
                    .unwrap_or_default();
                let display_name = config.get_string(&format!("auth.oauth2.providers[{}].display_name", i))
                    .unwrap_or_else(|_| name.clone());
                let authorize_url = config.get_string(&format!("auth.oauth2.providers[{}].authorize_url", i))
                    .unwrap_or_default();
                let token_url = config.get_string(&format!("auth.oauth2.providers[{}].token_url", i))
                    .unwrap_or_default();
                let userinfo_url = config.get_string(&format!("auth.oauth2.providers[{}].userinfo_url", i))
                    .unwrap_or_default();
                let scopes = config.get_string(&format!("auth.oauth2.providers[{}].scopes", i))
                    .map(|s| s.split(',').map(|sc| sc.trim().to_string()).collect())
                    .unwrap_or_default();
                let enabled = config.get_bool(&format!("auth.oauth2.providers[{}].enabled", i))
                    .unwrap_or(true);
                let icon_url = config.get_string(&format!("auth.oauth2.providers[{}].icon_url", i))
                    .ok();
                let brand_color = config.get_string(&format!("auth.oauth2.providers[{}].brand_color", i))
                    .ok();
                let token_endpoint_auth_method = config.get_string(&format!("auth.oauth2.providers[{}].token_endpoint_auth_method", i))
                    .unwrap_or_else(|_| "client_secret_post".to_string());

                // field_mapping 从配置读取内联表
                let field_mapping = config.inner()
                    .get_table(&format!("auth.oauth2.providers[{}].field_mapping", i))
                    .map(|table| {
                        table.into_iter()
                            .filter_map(|(k, v)| {
                                v.clone().into_string().map(|s| (k, s)).ok()
                            })
                            .collect::<std::collections::HashMap<String, String>>()
                    })
                    .unwrap_or_default();

                // token_field_mapping 从配置读取内联表
                let token_field_mapping = config.inner()
                    .get_table(&format!("auth.oauth2.providers[{}].token_field_mapping", i))
                    .map(|table| {
                        table.into_iter()
                            .filter_map(|(k, v)| {
                                v.clone().into_string().map(|s| (k, s)).ok()
                            })
                            .collect::<std::collections::HashMap<String, String>>()
                    })
                    .unwrap_or_default();

                // userinfo_extra_params 从配置读取内联表
                let userinfo_extra_params = config.inner()
                    .get_table(&format!("auth.oauth2.providers[{}].userinfo_extra_params", i))
                    .map(|table| {
                        table.into_iter()
                            .filter_map(|(k, v)| {
                                v.clone().into_string().map(|s| (k, s)).ok()
                            })
                            .collect::<std::collections::HashMap<String, String>>()
                    })
                    .unwrap_or_default();

                // authorize_extra_params 从配置读取内联表
                let authorize_extra_params = config.inner()
                    .get_table(&format!("auth.oauth2.providers[{}].authorize_extra_params", i))
                    .map(|table| {
                        table.into_iter()
                            .filter_map(|(k, v)| {
                                v.clone().into_string().map(|s| (k, s)).ok()
                            })
                            .collect::<std::collections::HashMap<String, String>>()
                    })
                    .unwrap_or_default();

                providers.push(cmx_auth::OAuth2ProviderConfig {
                    name,
                    display_name,
                    provider_type,
                    client_id,
                    client_secret,
                    redirect_uri,
                    authorize_url,
                    token_url,
                    userinfo_url,
                    scopes,
                    field_mapping,
                    token_endpoint_auth_method,
                    icon_url,
                    brand_color,
                    enabled,
                    token_response_path: config.get_string(&format!("auth.oauth2.providers[{}].token_response_path", i)).unwrap_or_default(),
                    token_field_mapping,
                    userinfo_method: config.get_string(&format!("auth.oauth2.providers[{}].userinfo_method", i)).unwrap_or_else(|_| "GET".to_string()),
                    userinfo_token_param: config.get_string(&format!("auth.oauth2.providers[{}].userinfo_token_param", i)).unwrap_or_else(|_| "bearer".to_string()),
                    userinfo_extra_params,
                    userinfo_response_path: config.get_string(&format!("auth.oauth2.providers[{}].userinfo_response_path", i)).unwrap_or_default(),
                    authorize_extra_params,
                    skip_ssl_verification: config.get_bool(&format!("auth.oauth2.providers[{}].skip_ssl_verification", i)).unwrap_or(false),
                });
            }
        }
        oauth2.providers = providers;

        // 读取 account_link 配置
        if let Ok(auto_link) = config.get_bool("auth.oauth2.account_link.auto_link_by_email") {
            oauth2.account_link.auto_link_by_email = auto_link;
        }
        if let Ok(auto_link) = config.get_bool("auth.oauth2.account_link.auto_link_by_username") {
            oauth2.account_link.auto_link_by_username = auto_link;
        }
        if let Ok(auto_register) = config.get_bool("auth.oauth2.account_link.auto_register") {
            oauth2.account_link.auto_register = auto_register;
        }
        if let Ok(default_role) = config.get_string("auth.oauth2.account_link.default_role") {
            oauth2.account_link.default_role = Some(default_role);
        }
        if let Ok(strategy) = config.get_string("auth.oauth2.account_link.username_strategy") {
            oauth2.account_link.username_strategy = strategy;
        }

        // 读取新增的 OAuth2 配置
        if let Ok(ttl) = config.get_int("auth.oauth2.state_ttl_secs") {
            oauth2.state_ttl_secs = ttl as u64;
        }
        if let Ok(ttl) = config.get_int("auth.oauth2.callback_code_ttl_secs") {
            oauth2.callback_code_ttl_secs = ttl as u64;
        }
        if let Ok(url) = config.get_string("auth.oauth2.frontend_callback_url") {
            oauth2.frontend_callback_url = url;
        }
    }

    // 超管配置（未配置时使用默认值 admin/cmxadmin，配置 username 后按配置覆盖）
    if let Ok(username) = config.get_string("auth.super_admin.username") {
        let default_sa = SuperAdminConfig::default();
        let sa_config = SuperAdminConfig {
            username,
            password: config.get_string("auth.super_admin.password")
                .unwrap_or(default_sa.password),
            email: config.get_string("auth.super_admin.email").ok(),
            roles: config.get_string("auth.super_admin.roles")
                .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
                .unwrap_or(default_sa.roles),
        };
        auth_config.super_admin = Some(sa_config);
    }

    // 静态 API Key 配置
    // 简化用法：只需配置 `key`，key_prefix 自动从前 8 位提取。
    // 高级用法：可选显式配置 `key_prefix`（用于迁移或自定义前缀）。
    let mut static_keys = Vec::new();
    for i in 0..10 {
        let key_key = format!("auth.static_api_keys[{}].key", i);
        if let Ok(key) = config.get_string(&key_key) {
            // key 为必填，key_prefix 可选（未填时由 StaticApiKeyConfig::resolve_key_prefix 自动提取）
            let key_prefix = config
                .get_string(&format!("auth.static_api_keys[{}].key_prefix", i))
                .ok();
            static_keys.push(StaticApiKeyConfig {
                key_prefix,
                key,
                user_id: config.get_string(&format!("auth.static_api_keys[{}].user_id", i)).ok(),
                service_name: config.get_string(&format!("auth.static_api_keys[{}].service_name", i)).ok(),
                scopes: config.get_string(&format!("auth.static_api_keys[{}].scopes", i))
                    .map(|s| s.split(',').map(|sc| sc.trim().to_string()).collect())
                    .unwrap_or_default(),
                description: config.get_string(&format!("auth.static_api_keys[{}].description", i)).ok(),
            });
        }
    }
    auth_config.static_api_keys = static_keys;

    // 认证白名单（TOML `[auth].whitelist` 数组）
    // 设计：与内置白名单合并使用，不覆盖内置项。
    // - TOML 未配置时 → auth_config.whitelist 保持空（仅使用内置白名单）
    // - TOML 配置时 → 存储用户自定义项，启动时与内置白名单合并去重
    if let Ok(custom_whitelist) = config.get_as::<Vec<String>>("auth.whitelist") {
        auth_config.whitelist = custom_whitelist;
    }

    auth_config
}
