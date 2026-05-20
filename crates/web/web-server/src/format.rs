//! 日志格式化模块
//!
//! 提供自定义的日志格式化器，优化日志输出的颜色和间距。

use tracing_subscriber::fmt::{format::FormatFields, FmtContext};
use tracing_subscriber::registry::LookupSpan;

/// 自定义日志格式化器：紧凑格式，优化颜色和间距
pub struct CompactFormatter;

impl<S, N> tracing_subscriber::fmt::FormatEvent<S, N> for CompactFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: tracing_subscriber::fmt::format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let meta = event.metadata();

        // 时间戳
        self.write_timestamp(&mut writer)?;

        // 日志级别（带颜色）
        match *meta.level() {
            tracing::Level::ERROR => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[1;31mERROR\x1b[0m")
                } else {
                    write!(writer, "ERROR")
                }
            }
            tracing::Level::WARN => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[1;33m WARN\x1b[0m")
                } else {
                    write!(writer, "WARN")
                }
            }
            tracing::Level::INFO => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[1;32m INFO\x1b[0m")
                } else {
                    write!(writer, "INFO")
                }
            }
            tracing::Level::DEBUG => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[1;36m DEBUG\x1b[0m")
                } else {
                    write!(writer, "DEBUG")
                }
            }
            tracing::Level::TRACE => {
                if writer.has_ansi_escapes() {
                    write!(writer, "\x1b[1;90mTRACE\x1b[0m")
                } else {
                    write!(writer, "TRACE")
                }
            }
        }?;

        // 线程名和线程ID
        let current_thread = std::thread::current();
        if let Some(name) = current_thread.name() {
            write!(writer, " {} {:?}", name, current_thread.id())?;
        } else {
            write!(writer, " {:?}", current_thread.id())?;
        }

        // 目标文件:行号（文件黄色，行号青色）
        if let (Some(file), Some(line)) = (meta.file(), meta.line()) {
            if writer.has_ansi_escapes() {
                write!(writer, " \x1b[33m{}\x1b[0m:\x1b[36m{}\x1b[0m", file, line)?;
            } else {
                write!(writer, " {}:{}", file, line)?;
            }
        }

        write!(writer, ": ")?;

        // 日志消息
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

impl CompactFormatter {
    fn write_timestamp(&self, writer: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = chrono::Utc::now();
        // 格式: 2026-05-20T13:19:16.226370Z
        write!(writer, "{} ", now.format("%Y-%m-%dT%H:%M:%S%.6fZ"))
    }
}
