use std::path::{Path, PathBuf};

/// 应用目录解析。测试可用 `LEXIGRAB_HOME` 覆盖，避免写到真实用户目录。
pub struct Paths {
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        if let Some(root) = std::env::var_os("LEXIGRAB_HOME") {
            let base = PathBuf::from(root);
            return Self {
                config_dir: base.join("config"),
                log_dir: base.join("logs"),
            };
        }
        let config_dir = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LexiGrab");
        let log_dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LexiGrab")
            .join("logs");
        Self {
            config_dir,
            log_dir,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        ensure_dir(&self.config_dir)?;
        ensure_dir(&self.log_dir)?;
        Ok(())
    }
}

pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_lexigrab_home_override() {
        let root = std::env::temp_dir().join("lexigrab-paths-test");
        // 只在当前测试内读取环境变量，独立进程内不会与其他测试冲突。
        std::env::set_var("LEXIGRAB_HOME", &root);
        let paths = Paths::resolve();
        std::env::remove_var("LEXIGRAB_HOME");
        assert_eq!(paths.config_dir, root.join("config"));
        assert_eq!(paths.log_dir, root.join("logs"));
    }
}
