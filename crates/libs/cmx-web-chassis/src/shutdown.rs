//! 优雅关闭信号：SIGINT (Ctrl+C) + SIGTERM（Unix）。收到即触发 axum graceful shutdown。

/// 等待关闭信号（Ctrl+C 或 SIGTERM）。抽自 web-server 的 shutdown_signal。
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("安装 Ctrl+C 处理器失败");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("安装 SIGTERM 处理器失败")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("收到 Ctrl+C 信号，开始优雅关闭...");
        },
        _ = terminate => {
            tracing::info!("收到 SIGTERM 信号，开始优雅关闭...");
        },
    }
}
