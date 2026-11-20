// SPDX-License-Identifier: AGPL-3.0-only
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::{debug, warn};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HGLOBAL, HWND, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VK_C, VK_CONTROL, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
};

use crate::platform::selection::{
    normalize_captured_text, CaptureError, CaptureMethod, CaptureResult, ScreenAnchor,
    SelectionContext,
};

/// 捕获总预算：超时返回可重试错误。
const CAPTURE_BUDGET: Duration = Duration::from_millis(180);
/// 轮询剪贴板序列号的间隔。
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(5);
/// 剪贴板被占用时的重试次数与间隔。
const CLIPBOARD_OPEN_ATTEMPTS: usize = 5;
const CLIPBOARD_OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);
/// CF_UNICODETEXT 的固定格式号；避免依赖 windows crate 的常量位置。
const CF_UNICODETEXT_FORMAT: u32 = 13;

pub fn capture_selection() -> CaptureResult {
    let started = Instant::now();
    match capture_inner(started) {
        Ok(selection) => {
            debug!(
                len = selection.text.chars().count(),
                app = %selection.app_name,
                elapsed_ms = selection.elapsed_ms,
                has_anchor = selection.anchor.is_some(),
                "选区捕获成功"
            );
            CaptureResult::success(selection)
        }
        Err(error) => {
            warn!(code = %error.code, "选区捕获失败");
            CaptureResult::failure(error)
        }
    }
}

fn capture_inner(started: Instant) -> Result<SelectionContext, CaptureError> {
    let deadline = started + CAPTURE_BUDGET;

    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return Err(CaptureError {
            code: "CAPTURE_NO_FOREGROUND".to_string(),
            message: "没有检测到前台窗口，请先选中文字再按热键".to_string(),
            detail: None,
        });
    }

    let mut process_id = 0u32;
    let thread_id = unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
    let (app_name, access_denied) = unsafe { query_app_name(process_id) };

    let anchor = unsafe { caret_anchor(foreground, thread_id) }
        .or_else(|| unsafe { cursor_anchor(foreground) });

    let raw_text = capture_via_clipboard(deadline, access_denied)?;
    let text = normalize_captured_text(&raw_text);
    if text.is_empty() {
        return Err(CaptureError {
            code: "CAPTURE_EMPTY".to_string(),
            message: "没有捕获到文本，请先在目标应用选中文字".to_string(),
            detail: None,
        });
    }

    Ok(SelectionContext {
        text,
        // Sentence capture (UIA) is fetched separately by the runtime so the
        // clipboard path stays within its own budget.
        sentence: None,
        url: None,
        title: None,
        domain: None,
        anchor,
        foreground_hwnd: foreground.0 as isize as i64,
        app_name: app_name.unwrap_or_else(|| "unknown".to_string()),
        method: CaptureMethod::Clipboard,
        captured_at_ms: now_ms(),
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// 读取剪贴板文本，失败时清空并关闭剪贴板。
fn snapshot_clipboard() -> (u32, Option<String>) {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    if !open_clipboard() {
        return (sequence, None);
    }
    let text = unsafe { read_clipboard_text_open() };
    unsafe {
        let _ = CloseClipboard();
    };
    (sequence, text)
}

fn open_clipboard() -> bool {
    for attempt in 0..CLIPBOARD_OPEN_ATTEMPTS {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return true;
        }
        if attempt + 1 < CLIPBOARD_OPEN_ATTEMPTS {
            std::thread::sleep(CLIPBOARD_OPEN_RETRY_DELAY);
        }
    }
    false
}

/// 发送 Ctrl+C。返回 false 表示 SendInput 被拒绝（常见于更高完整性级别的窗口）。
///
/// 热键触发时 Shift 仍可能处于按下状态，先补一个 Shift 抬起，
/// 否则目标控件收到的是 Ctrl+Shift+C，不会复制。
fn send_copy_shortcut() -> bool {
    // 热键触发时 Shift 仍可能按着，先单独抬起，避免目标收到 Ctrl+Shift+C。
    let clear_modifier = [key_input(VK_SHIFT, true)];
    let _ = unsafe { SendInput(&clear_modifier, std::mem::size_of::<INPUT>() as i32) };
    std::thread::sleep(Duration::from_millis(5));

    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_C, false),
        key_input(VK_C, true),
        key_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    sent == inputs.len() as u32
}

fn key_input(
    key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    released: bool,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if released {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn capture_via_clipboard(deadline: Instant, access_denied: bool) -> Result<String, CaptureError> {
    let (sequence_before, original_text) = snapshot_clipboard();
    // 让热键的修饰键先被目标窗口消化，再复制。
    std::thread::sleep(Duration::from_millis(20));

    if !send_copy_shortcut() {
        return Err(CaptureError {
            code: "TARGET_ELEVATED".to_string(),
            message: "目标应用权限更高，无法发送复制按键，请以相同权限启动 LexiGrab".to_string(),
            detail: None,
        });
    }

    let mut sequence_changes = 0u32;
    let mut read_failures = 0u32;
    loop {
        if Instant::now() >= deadline {
            let (code, message) = if access_denied {
                (
                    "TARGET_ELEVATED",
                    "目标应用以管理员权限运行，请以相同权限启动 LexiGrab",
                )
            } else {
                ("CAPTURE_TIMEOUT", "未能读取选区，请重试")
            };
            return Err(CaptureError {
                code: code.to_string(),
                message: message.to_string(),
                detail: Some(format!(
                    "access_denied={access_denied}, sequence_changes={sequence_changes}, read_failures={read_failures}, seq_before={sequence_before}, seq_now={}",
                    unsafe { GetClipboardSequenceNumber() }
                )),
            });
        }

        let sequence = unsafe { GetClipboardSequenceNumber() };
        if sequence != sequence_before {
            sequence_changes += 1;
            if !open_clipboard() {
                return Err(CaptureError {
                    code: "CAPTURE_CLIPBOARD_BUSY".to_string(),
                    message: "剪贴板被其他程序占用，请稍后重试".to_string(),
                    detail: None,
                });
            }
            let text = unsafe { read_clipboard_text_open() };
            unsafe {
                let _ = CloseClipboard();
            };

            match text {
                Some(text) if !text.trim().is_empty() => {
                    restore_clipboard(original_text.as_deref(), &text);
                    return Ok(text);
                }
                _ => read_failures += 1,
            }
        }

        std::thread::sleep(CLIPBOARD_POLL_INTERVAL);
    }
}

/// 仅在剪贴板仍是本次捕获文本时恢复原内容；原本没有文本时不清空，避免破坏图片或文件列表。
fn restore_clipboard(original: Option<&str>, captured: &str) {
    let Some(original) = original else {
        return;
    };
    if !open_clipboard() {
        return;
    }
    let current = unsafe { read_clipboard_text_open() };
    if current.as_deref() == Some(captured) {
        let restored = unsafe { write_clipboard_text_open(original) };
        if !restored {
            debug!("恢复剪贴板文本失败");
        }
    }
    unsafe {
        let _ = CloseClipboard();
    };
}

unsafe fn read_clipboard_text_open() -> Option<String> {
    let handle = GetClipboardData(CF_UNICODETEXT_FORMAT).ok()?;
    if handle.is_invalid() {
        return None;
    }
    let memory = HGLOBAL(handle.0);
    let pointer = GlobalLock(memory) as *const u16;
    if pointer.is_null() {
        return None;
    }
    let mut length = 0usize;
    while *pointer.add(length) != 0 {
        length += 1;
    }
    let slice = std::slice::from_raw_parts(pointer, length);
    let text = String::from_utf16_lossy(slice);
    let _ = GlobalUnlock(memory);
    Some(text)
}

unsafe fn write_clipboard_text_open(text: &str) -> bool {
    let _ = EmptyClipboard();
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();
    let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
        return false;
    };
    let pointer = GlobalLock(memory) as *mut u16;
    if pointer.is_null() {
        return false;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer, wide.len());
    let _ = GlobalUnlock(memory);
    if SetClipboardData(CF_UNICODETEXT_FORMAT, Some(HANDLE(memory.0))).is_err() {
        return false;
    }
    // 成功后内存所有权转移给系统，不再释放。
    true
}

unsafe fn query_app_name(process_id: u32) -> (Option<String>, bool) {
    match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
        Ok(handle) => {
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);
            if result.is_ok() && size > 0 {
                let path = String::from_utf16_lossy(&buffer[..size as usize]);
                let name = Path::new(&path)
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string());
                (name, false)
            } else {
                (None, false)
            }
        }
        Err(_) => (None, true),
    }
}

unsafe fn caret_anchor(foreground: HWND, thread_id: u32) -> Option<ScreenAnchor> {
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    if GetGUIThreadInfo(thread_id, &mut info).is_err() {
        return None;
    }
    if info.hwndCaret.0.is_null() {
        return None;
    }
    let mut point = POINT {
        x: info.rcCaret.right,
        y: info.rcCaret.bottom,
    };
    if ClientToScreen(info.hwndCaret, &mut point).0 == 0 {
        return None;
    }
    Some(ScreenAnchor {
        x: point.x as f64,
        y: point.y as f64,
        dpi_scale: dpi_scale(foreground),
    })
}

unsafe fn cursor_anchor(foreground: HWND) -> Option<ScreenAnchor> {
    let mut point = POINT::default();
    if GetCursorPos(&mut point).is_err() {
        return None;
    }
    Some(ScreenAnchor {
        x: point.x as f64,
        y: point.y as f64,
        dpi_scale: dpi_scale(foreground),
    })
}

fn dpi_scale(hwnd: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f64 / 96.0
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
