#[cfg(test)]
mod tests {
    use cmx_buffer::config::{CacheConfig, LockConfig, RedisConfig};

    #[test]
    fn test_redis_config_default() {
        let config = RedisConfig::default();
        assert_eq!(config.url, "redis://127.0.0.1:6379");
        assert_eq!(config.mode, cmx_buffer::config::RedisMode::Standalone);
        assert_eq!(config.key_prefix, "cmx:");
    }

    #[test]
    fn test_redis_config_builder() {
        let config = RedisConfig::new("redis://localhost:6379").with_key_prefix("app:");

        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.key_prefix, "app:");
    }

    #[test]
    fn test_lock_config_default() {
        let config = LockConfig::default();
        assert_eq!(config.expire_seconds, 30);
        assert_eq!(config.retry_interval_ms, 200);
    }

    #[test]
    fn test_lock_config_builder() {
        let config = LockConfig::new().with_expire(60).with_retry_interval(500);

        assert_eq!(config.expire_seconds, 60);
        assert_eq!(config.retry_interval_ms, 500);
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.default_ttl, 0);
        assert!(config.enable_prefix);
    }

    #[test]
    fn test_cache_config_builder() {
        let config = CacheConfig::new().with_default_ttl(3600);

        assert_eq!(config.default_ttl, 3600);
    }

    #[test]
    fn test_duration_methods() {
        let config = RedisConfig::default();
        assert_eq!(config.connection_timeout_duration().as_secs(), 5);
        assert_eq!(config.operation_timeout_duration().as_secs(), 3);

        let lock_config = LockConfig::default();
        assert_eq!(lock_config.expire_duration().as_secs(), 30);
        assert_eq!(lock_config.retry_interval_duration().as_millis(), 200);
    }
}
