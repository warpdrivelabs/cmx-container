//! 共享文件 I/O 助手：JSON 读、原子写、存在性判断。
//!
//! 复刻 Node 后端各 `*Store.js` 的「读 JSON / 临时文件 + rename 原子写」语义。
//! 所有门户 store 都应通过这里读写，保证一致的错误映射与原子性。

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt;

use crate::error::{PortalError, PortalResult};

/// 读取并解析一个 JSON 文件为目标类型。
///
/// 文件不存在 → [`PortalError::NotFound`]；内容非法 JSON → [`PortalError::Json`]。
pub async fn read_json<T: DeserializeOwned>(path: &Path) -> PortalResult<T> {
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PortalError::not_found(format!("{} 不存在", path.display())));
        }
        Err(e) => return Err(PortalError::Io(e)),
    };
    let value = serde_json::from_slice::<T>(&bytes)?;
    Ok(value)
}

/// 读取一个 JSON 文件为 [`serde_json::Value`]，文件不存在时返回 `None`。
///
/// 适合「可选索引文件」「首次运行尚无数据」等场景。
pub async fn read_json_opt(path: &Path) -> PortalResult<Option<serde_json::Value>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let v = serde_json::from_slice::<serde_json::Value>(&bytes)?;
            Ok(Some(v))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PortalError::Io(e)),
    }
}

/// 原子写入 JSON：先写同目录临时文件，再 `rename` 覆盖目标（POSIX 原子）。
///
/// 自动创建父目录。`pretty` 为 true 时输出缩进 JSON（与 Node 后端落盘格式一致，便于 diff）。
pub async fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> PortalResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(PortalError::Io)?;
    }
    let data = if pretty {
        serde_json::to_vec_pretty(value)?
    } else {
        serde_json::to_vec(value)?
    };

    // 同目录临时文件：用 pid + 纳秒时间戳避免并发碰撞，写完 fsync 再 rename。
    let tmp = tmp_sibling(path);
    {
        let mut f = tokio::fs::File::create(&tmp)
            .await
            .map_err(PortalError::Io)?;
        f.write_all(&data).await.map_err(PortalError::Io)?;
        f.flush().await.map_err(PortalError::Io)?;
        f.sync_all().await.map_err(PortalError::Io)?;
    }
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(()),
        Err(e) => {
            // rename 失败则尽力清理临时文件，避免残留。
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(PortalError::Io(e))
        }
    }
}

/// 原子写入纯文本（用于 native page 源文件 js/html）：临时文件 + rename，自动建父目录。
pub async fn write_text_atomic(path: &Path, text: &str) -> PortalResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(PortalError::Io)?;
    }
    let tmp = tmp_sibling(path);
    {
        let mut f = tokio::fs::File::create(&tmp)
            .await
            .map_err(PortalError::Io)?;
        f.write_all(text.as_bytes())
            .await
            .map_err(PortalError::Io)?;
        f.flush().await.map_err(PortalError::Io)?;
        f.sync_all().await.map_err(PortalError::Io)?;
    }
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(PortalError::Io(e))
        }
    }
}

/// 读取纯文本文件；不存在时返回 `None`。
pub async fn read_text_opt(path: &Path) -> PortalResult<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PortalError::Io(e)),
    }
}

/// 生成与目标文件同目录的临时文件名 `<name>.tmp.<pid>.<nanos>`。
fn tmp_sibling(path: &Path) -> std::path::PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "tmp".to_string());
    let mut p = path.to_path_buf();
    p.set_file_name(format!("{file_name}.tmp.{pid}.{nanos}"));
    p
}
