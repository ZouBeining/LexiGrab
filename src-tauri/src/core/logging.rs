// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 持有异步日志 worker，进程退出前不可丢弃。
pub struct LogGuard {
    _worker: WorkerGuard,
}

/// 初始化文件日志与（debug 构建下的）控制台日志。
///
/// 日志只记录长度、方法、应用名、耗时与错误码，不写选中文本、剪贴板内容或窗口标题。
pub fn init(log_dir: &Path) -> std::io::Result<LogGuard> {
    std::fs::create_dir_all(log_dir)?;
    let appender = tracing_appender::rolling::daily(log_dir, "lexigrab.log");
    let (writer, worker) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let file_layer = fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true);

    let subscriber = tracing_subscriber::registry().with(filter).with(file_layer);

    if cfg!(debug_assertions) {
        subscriber.with(fmt::layer().with_target(false)).init();
    } else {
        subscriber.init();
    }

    Ok(LogGuard { _worker: worker })
}
