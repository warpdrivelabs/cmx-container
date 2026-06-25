//! gRPC 重试工具：纯函数 + 泛型 [`with_retry`]。
//!
//! # 使用约束
//!
//! [`with_retry`] 的闭包 **只能返回原始 [`volo_grpc::Status`] 错误**。
//! 业务解析（[`volo_grpc::Response::into_inner`]、proto → domain 转换）必须在
//! [`with_retry`] 返回后做一次。否则重试分支会重复消费 response，导致 panic 或语义错误。

use std::time::{Duration, Instant};

use cmx_traits::rpc::RpcError;
use volo_grpc::{Code, Status};

/// 重试统计，供调用方补全结构化日志字段。
#[derive(Debug, Clone, Copy)]
pub struct RetryStats {
    /// 实际尝试次数（从 1 开始）。
    pub attempts: usize,
    /// 总耗时。
    pub elapsed: Duration,
}

/// 判断 gRPC 错误是否可重试。
///
/// 可重试的错误：
/// - [`Code::Unavailable`]：服务不可达。
/// - [`Code::DeadlineExceeded`]：超时。
/// - [`Code::ResourceExhausted`]：限流场景，重试可能成功。
/// - [`Code::Aborted`]：事务中止，可重试。
///
/// 不可重试的错误：`INVALID_ARGUMENT`、`NOT_FOUND`、`PERMISSION_DENIED` 等业务错误。
pub fn is_retryable_error(status: &Status) -> bool {
    matches!(
        status.code(),
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted | Code::Aborted
    )
}

/// 计算重试退避时间（指数退避，上限 800ms）。
///
/// 退避序列：50ms → 100ms → 200ms → 400ms → 800ms。
pub fn retry_backoff(attempt: usize) -> Duration {
    let backoff_ms = 50u64.saturating_mul(1u64 << attempt.min(4));
    Duration::from_millis(backoff_ms.min(800))
}

/// 执行带总时间预算的重试循环。
///
/// 成功返回 `(T, RetryStats)`，失败返回 `(RpcError, RetryStats)`。
///
/// 本函数**不记最终失败日志**（仅记中间重试 warn），最终失败日志由调用方
/// 拿到 `stats` 后用业务字段（`service_name`/`service_key`/`success=false` 等）记录，
/// 确保失败路径结构化字段零丢失。
pub async fn with_retry<F, Fut, T>(
    timeout_ms: u64,
    max_retries: usize,
    f: F,
) -> Result<(T, RetryStats), (RpcError, RetryStats)>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, Status>>,
{
    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms);

    for attempt in 0..=max_retries {
        let remaining = deadline.saturating_duration_since(Instant::now());

        if !remaining.is_zero() && attempt > 0 {
            let backoff = std::cmp::min(retry_backoff(attempt - 1), remaining);
            tokio::time::sleep(backoff).await;
        }

        if remaining.is_zero() {
            // 预算耗尽：未发起调用，elapsed 即循环耗时
            let stats = RetryStats {
                attempts: attempt + 1,
                elapsed: start.elapsed(),
            };
            return Err((
                RpcError::Timeout(format!(
                    "重试预算耗尽: 总耗时 {}ms",
                    stats.elapsed.as_millis()
                )),
                stats,
            ));
        }

        if attempt > 0 {
            // 中间重试：业务关联性弱，仅记 attempt/max_retries/remaining_ms
            tracing::debug!(
                target: "cmx_rpc",
                attempt,
                max_retries,
                remaining_ms = remaining.as_millis() as u64,
                "RPC 重试调度"
            );
        }

        match f().await {
            Ok(result) => {
                // 调用成功：在 f().await 返回后计算 elapsed，含本次调用耗时
                let stats = RetryStats {
                    attempts: attempt + 1,
                    elapsed: start.elapsed(),
                };
                return Ok((result, stats));
            }
            Err(e) => {
                if is_retryable_error(&e) && attempt < max_retries {
                    // 中间重试 warn：仅记重试调度信息，业务字段由调用方最终日志聚合
                    tracing::warn!(
                        target: "cmx_rpc",
                        attempt = attempt + 1,
                        max_retries,
                        error = %e,
                        "RPC 失败（可重试）"
                    );
                    continue;
                }
                // 最终失败：在 f().await 返回后计算 elapsed，含本次调用耗时；不在此记日志，交还调用方
                let stats = RetryStats {
                    attempts: attempt + 1,
                    elapsed: start.elapsed(),
                };
                return Err((RpcError::RpcCallFailed(e.to_string()), stats));
            }
        }
    }

    unreachable!("retry loop must return before exiting")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn status(code: Code) -> Status {
        Status::new(code, "test")
    }

    #[test]
    fn test_is_retryable() {
        for c in [
            Code::Unavailable,
            Code::DeadlineExceeded,
            Code::ResourceExhausted,
            Code::Aborted,
        ] {
            assert!(is_retryable_error(&status(c)), "{:?} 应可重试", c);
        }
        for c in [
            Code::InvalidArgument,
            Code::NotFound,
            Code::PermissionDenied,
            Code::Unimplemented,
        ] {
            assert!(!is_retryable_error(&status(c)), "{:?} 不应可重试", c);
        }
    }

    #[test]
    fn test_retry_backoff_sequence() {
        // 50 → 100 → 200 → 400 → 800 → 800（上限）
        let seq: Vec<u64> = (0..6).map(|i| retry_backoff(i).as_millis() as u64).collect();
        assert_eq!(seq, vec![50, 100, 200, 400, 800, 800]);
    }

    #[tokio::test]
    async fn test_with_retry_success_after_failures() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let (result, stats) = with_retry(10_000, 3, || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(status(Code::Unavailable))
                } else {
                    Ok(42)
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(result, 42);
        assert_eq!(stats.attempts, 3);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_fails_fast() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let (err, stats) = with_retry::<_, _, i32>(10_000, 3, || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(status(Code::InvalidArgument))
            }
        })
        .await
        .unwrap_err();
        assert!(matches!(err, RpcError::RpcCallFailed(_)));
        assert_eq!(stats.attempts, 1); // 失败也带 stats
        assert_eq!(counter.load(Ordering::SeqCst), 1); // 只调一次
    }

    #[tokio::test]
    async fn test_with_retry_budget_exhausted() {
        // 预算 0 → 立即超时，不执行闭包；失败带 stats
        let (err, stats) = with_retry::<_, _, i32>(0, 3, || async { Ok(1) })
            .await
            .unwrap_err();
        assert!(matches!(err, RpcError::Timeout(_)));
        assert_eq!(stats.attempts, 1);
    }
}
