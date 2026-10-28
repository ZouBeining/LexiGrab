use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::core::config::Paths;
use crate::platform::{CaptureMethod, CaptureResult, SelectionContext};

/// 捕获样本上限，避免诊断数据无限增长。
const MAX_SAMPLES: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct HotkeyStatus {
    pub accelerator: String,
    pub registered: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureSample {
    pub elapsed_ms: f64,
    pub method: Option<CaptureMethod>,
    pub app_name: String,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureStats {
    pub samples: Vec<CaptureSample>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub success_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub version: String,
    pub hotkey: HotkeyStatus,
    pub config_dir: String,
    pub log_dir: String,
    pub popup_visible: bool,
}

pub struct AppState {
    pub paths: Paths,
    hotkey: RwLock<HotkeyStatus>,
    last_capture: Mutex<Option<CaptureResult>>,
    samples: Mutex<Vec<CaptureSample>>,
    popup_interactive: AtomicBool,
    last_trigger: Mutex<Option<Instant>>,
}

/// 取锁失败时复用内部数据，避免一次 panic 让整个诊断功能不可用。
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl AppState {
    pub fn new(paths: Paths, accelerator: &str) -> Self {
        Self {
            paths,
            hotkey: RwLock::new(HotkeyStatus {
                accelerator: accelerator.to_string(),
                registered: false,
                message: None,
            }),
            last_capture: Mutex::new(None),
            samples: Mutex::new(Vec::new()),
            popup_interactive: AtomicBool::new(false),
            last_trigger: Mutex::new(None),
        }
    }

    pub fn hotkey_status(&self) -> HotkeyStatus {
        read(&self.hotkey).clone()
    }

    pub fn set_hotkey_status(&self, registered: bool, message: Option<String>) {
        let mut status = write(&self.hotkey);
        status.registered = registered;
        status.message = message;
    }

    pub fn record_capture(&self, result: &CaptureResult) {
        {
            let mut last = lock(&self.last_capture);
            *last = Some(result.clone());
        }
        let sample = match &result.selection {
            Some(selection) => CaptureSample {
                elapsed_ms: selection.elapsed_ms,
                method: Some(selection.method),
                app_name: selection.app_name.clone(),
                succeeded: true,
            },
            None => CaptureSample {
                elapsed_ms: 0.0,
                method: None,
                app_name: String::new(),
                succeeded: false,
            },
        };
        let mut samples = lock(&self.samples);
        samples.push(sample);
        if samples.len() > MAX_SAMPLES {
            samples.remove(0);
        }
    }

    pub fn last_selection(&self) -> Option<SelectionContext> {
        lock(&self.last_capture)
            .as_ref()
            .and_then(|result| result.selection.clone())
    }

    pub fn last_capture(&self) -> Option<CaptureResult> {
        lock(&self.last_capture).clone()
    }

    pub fn capture_stats(&self) -> CaptureStats {
        let samples = lock(&self.samples).clone();
        let mut durations: Vec<f64> = samples
            .iter()
            .filter(|sample| sample.succeeded)
            .map(|sample| sample.elapsed_ms)
            .collect();
        durations.sort_by(|left, right| left.total_cmp(right));
        let succeeded = samples.iter().filter(|sample| sample.succeeded).count();
        CaptureStats {
            p50_ms: percentile(&durations, 0.50),
            p95_ms: percentile(&durations, 0.95),
            success_rate: if samples.is_empty() {
                None
            } else {
                Some(succeeded as f64 / samples.len() as f64)
            },
            samples,
        }
    }

    pub fn set_popup_interactive(&self, interactive: bool) {
        self.popup_interactive.store(interactive, Ordering::SeqCst);
    }

    pub fn popup_interactive(&self) -> bool {
        self.popup_interactive.load(Ordering::SeqCst)
    }

    /// 250ms 去抖：返回 true 表示本次触发应当执行。
    pub fn allow_trigger(&self, window: Duration) -> bool {
        let now = Instant::now();
        let mut last = lock(&self.last_trigger);
        match *last {
            Some(previous) if now.duration_since(previous) < window => false,
            _ => {
                *last = Some(now);
                true
            }
        }
    }
}

/// 最近秩百分位（nearest-rank），样本为空时返回 None。
fn percentile(sorted: &[f64], ratio: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (sorted.len() as f64 * ratio).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        AppState::new(
            Paths {
                config_dir: std::path::PathBuf::from("config"),
                log_dir: std::path::PathBuf::from("logs"),
            },
            "CmdOrCtrl+Shift+D",
        )
    }

    #[test]
    fn percentile_handles_empty_and_single_sample() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[12.0], 0.95), Some(12.0));
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let samples = [10.0, 20.0, 30.0, 40.0];
        assert_eq!(percentile(&samples, 0.5), Some(20.0));
        assert_eq!(percentile(&samples, 0.95), Some(40.0));
    }

    #[test]
    fn debounce_ignores_repeat_within_window() {
        let state = state();
        let window = Duration::from_millis(250);
        assert!(state.allow_trigger(window));
        assert!(!state.allow_trigger(window));
    }

    #[test]
    fn debounce_allows_after_window() {
        let state = state();
        assert!(state.allow_trigger(Duration::from_millis(0)));
    }
}
