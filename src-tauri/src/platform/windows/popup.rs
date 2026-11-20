// SPDX-License-Identifier: AGPL-3.0-only
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{AppHandle, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GA_ROOT, GWL_EXSTYLE,
    HWND_TOPMOST, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE,
    SW_SHOWNOACTIVATE, WM_ACTIVATE, WM_MOUSEACTIVATE, WM_NCDESTROY, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

use crate::core::error::{IpcError, IpcResult};
use crate::platform::ScreenAnchor;

const POPUP_WIDTH_LOGICAL: f64 = 420.0;
const POPUP_HEIGHT_LOGICAL: f64 = 560.0;
const POPUP_MARGIN: i32 = 8;
const POPUP_GAP: i32 = 18;
const SUBCLASS_ID: usize = 1;
/// 编辑模式允许窗口被激活。
static ALLOW_ACTIVATION: AtomicBool = AtomicBool::new(false);

/// 触发前的真实前台窗口，编辑模式关闭时用于恢复焦点。
static ORIGINAL_FOREGROUND: AtomicIsize = AtomicIsize::new(0);
/// 已安装子类化的 HWND，避免重复 SetWindowSubclass。
static SUBCLASSED_HWND: AtomicIsize = AtomicIsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// 按锚点与工作区计算弹窗左上角。anchor 为物理屏幕像素。
pub fn place_popup(anchor: POINT, work: WorkArea, width: i32, height: i32) -> (i32, i32) {
    let minimum_x = work.left + POPUP_MARGIN;
    let maximum_x = (work.right - width - POPUP_MARGIN).max(minimum_x);
    let minimum_y = work.top + POPUP_MARGIN;
    let maximum_y = (work.bottom - height - POPUP_MARGIN).max(minimum_y);

    let mut y = anchor.y + POPUP_GAP;
    if y > maximum_y {
        y = anchor.y - height - POPUP_GAP;
    }

    (
        anchor.x.clamp(minimum_x, maximum_x),
        y.clamp(minimum_y, maximum_y),
    )
}

fn window_hwnd(window: &WebviewWindow) -> Option<HWND> {
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(raw) => {
            let hwnd = HWND(raw.hwnd.get() as *mut _);
            Some(unsafe { GetAncestor(hwnd, GA_ROOT) })
        }
        _ => None,
    }
}

fn require_hwnd(window: &WebviewWindow) -> IpcResult<HWND> {
    window_hwnd(window).ok_or_else(|| IpcError::internal("无法获取弹窗 HWND"))
}

static MAIN_THREAD_ID: std::sync::OnceLock<std::thread::ThreadId> = std::sync::OnceLock::new();

/// 记录主线程 id，用于判断是否需要调度。
pub fn mark_main_thread() {
    let _ = MAIN_THREAD_ID.set(std::thread::current().id());
}

/// 子类化、显示与激活都必须发生在创建窗口的主线程上。
/// 已经在主线程时直接执行，避免 Tauri 同步命令里等待自身造成死锁。
fn run_on_main_thread<T: Send + 'static>(
    app: &AppHandle,
    task: impl FnOnce() -> T + Send + 'static,
) -> IpcResult<T> {
    if MAIN_THREAD_ID.get() == Some(&std::thread::current().id()) {
        return Ok(task());
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(task());
    })
    .map_err(|error| IpcError::internal(format!("主线程调度失败: {error}")))?;
    receiver
        .recv()
        .map_err(|error| IpcError::internal(format!("主线程任务中断: {error}")))
}

fn dpi_scale(hwnd: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f64 / 96.0
    }
}

fn monitor_work_area(point: POINT) -> WorkArea {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            WorkArea {
                left: info.rcWork.left,
                top: info.rcWork.top,
                right: info.rcWork.right,
                bottom: info.rcWork.bottom,
            }
        } else {
            WorkArea {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            }
        }
    }
}

fn cursor_point() -> POINT {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return POINT::default();
    }
    point
}

fn ensure_no_activate_style(hwnd: HWND) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let updated = (style | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize)
            & !(WS_EX_APPWINDOW.0 as isize);
        if updated != style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        }
    }
}

fn clear_no_activate_style(hwnd: HWND) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let updated = style & !(WS_EX_NOACTIVATE.0 as isize);
        if updated != style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        }
    }
}

/// 点击 WebView2 的按钮时保持原应用前台。
///
/// WebView2 的子窗口会在点击时自行激活宿主窗口，因此除顶层窗口外，
/// 还要对所有子窗口拦截 WM_MOUSEACTIVATE；万一仍被激活，则在 WM_ACTIVATE
/// 后把前台窗口还回去。编辑模式通过 ALLOW_ACTIVATION 放行。
unsafe extern "system" fn mouse_activate_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _reference: usize,
) -> LRESULT {
    if message == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if message == WM_ACTIVATE && !ALLOW_ACTIVATION.load(Ordering::SeqCst) {
        // 低字非 0 表示正在被激活。
        if (wparam.0 & 0xFFFF) != 0 {
            restore_foreground_async();
        }
    }
    if message == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(mouse_activate_proc), SUBCLASS_ID);
        SUBCLASSED_HWND.store(0, Ordering::SeqCst);
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}

/// 被 WebView2 抢走前台后，短暂延迟再把原窗口还回去。
fn restore_foreground_async() {
    let raw = ORIGINAL_FOREGROUND.load(Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    std::thread::spawn(move || {
        // 等待点击（按下/抬起）完成后再归还前台，否则 WebView2 会丢弃这次点击。
        std::thread::sleep(std::time::Duration::from_millis(350));
        if ALLOW_ACTIVATION.load(Ordering::SeqCst) {
            // 已进入编辑模式，保留焦点。
            return;
        }
        unsafe {
            let _ = SetForegroundWindow(HWND(raw as *mut _));
        }
    });
}

fn ensure_subclass(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    if SUBCLASSED_HWND.load(Ordering::SeqCst) == raw {
        return;
    }
    let installed = unsafe { SetWindowSubclass(hwnd, Some(mouse_activate_proc), SUBCLASS_ID, 0) };
    if installed.as_bool() {
        SUBCLASSED_HWND.store(raw, Ordering::SeqCst);
    } else {
        tracing::warn!("弹窗子类化失败，点击可能抢焦点");
    }
}

/// 显示弹窗：不激活、置顶、按锚点定位。
pub fn show_no_activate(
    app: &AppHandle,
    window: &WebviewWindow,
    anchor: Option<ScreenAnchor>,
) -> IpcResult<()> {
    let hwnd = require_hwnd(window)?;
    let scale = dpi_scale(hwnd);
    let width = (POPUP_WIDTH_LOGICAL * scale).round() as i32;
    let height = (POPUP_HEIGHT_LOGICAL * scale).round() as i32;

    ALLOW_ACTIVATION.store(false, Ordering::SeqCst);
    let foreground = unsafe { GetForegroundWindow() };
    if !foreground.0.is_null() {
        ORIGINAL_FOREGROUND.store(foreground.0 as isize, Ordering::SeqCst);
    }

    let anchor_point = anchor
        .map(|value| POINT {
            x: value.x.round() as i32,
            y: value.y.round() as i32,
        })
        .unwrap_or_else(cursor_point);
    let work = monitor_work_area(anchor_point);
    let (x, y) = place_popup(anchor_point, work, width, height);

    let raw = hwnd.0 as isize;
    // 1) 先安装 NOACTIVATE 样式与子类化，避免 show 时被激活。
    run_on_main_thread(app, move || {
        let hwnd = HWND(raw as *mut _);
        ensure_no_activate_style(hwnd);
        ensure_subclass(hwnd);
    })?;
    // 2) 定位并显式使用 SW_SHOWNOACTIVATE（不走 Tauri show，避免它重置扩展样式）。
    run_on_main_thread(app, move || -> IpcResult<()> {
        let hwnd = HWND(raw as *mut _);
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .map_err(|error| IpcError::internal(format!("定位弹窗失败: {error}")))?;
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            if GetForegroundWindow() == hwnd {
                // 少数情况下 WebView2 仍会激活宿主窗口，立即把前台还回去。
                restore_foreground_async();
            }
        }
        Ok(())
    })?
}

/// 编辑模式：清除 NOACTIVATE 并尝试激活，必要时用 AttachThreadInput 绕过前台锁。
pub fn activate(app: &AppHandle, window: &WebviewWindow) -> IpcResult<()> {
    let hwnd = require_hwnd(window)?;
    let raw = hwnd.0 as isize;
    ALLOW_ACTIVATION.store(true, Ordering::SeqCst);
    run_on_main_thread(app, move || -> IpcResult<()> {
        let hwnd = HWND(raw as *mut _);
        unsafe {
            clear_no_activate_style(hwnd);
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
            .map_err(|error| IpcError::internal(format!("置顶弹窗失败: {error}")))?;
            if SetForegroundWindow(hwnd).as_bool() {
                return Ok(());
            }

            let foreground = GetForegroundWindow();
            if foreground.0.is_null() {
                return Ok(());
            }
            let foreground_thread = GetWindowThreadProcessId(foreground, None);
            let current_thread = GetCurrentThreadId();
            let attached = foreground_thread != 0
                && foreground_thread != current_thread
                && AttachThreadInput(current_thread, foreground_thread, true).as_bool();
            let activated = SetForegroundWindow(hwnd).as_bool();
            if attached {
                let _ = AttachThreadInput(current_thread, foreground_thread, false);
            }
            if !activated {
                tracing::warn!("激活弹窗失败，回退为非激活显示");
            }
        }
        Ok(())
    })?
}

/// 直接 ShowWindow(SW_HIDE)。弹窗是用 SW_SHOWNOACTIVATE 显示的，
/// tao 内部仍认为它隐藏，走 WebviewWindow::hide 会成为空操作。
pub fn hide(app: &AppHandle, window: &WebviewWindow) -> IpcResult<()> {
    let hwnd = require_hwnd(window)?;
    let raw = hwnd.0 as isize;
    run_on_main_thread(app, move || unsafe {
        let hwnd = HWND(raw as *mut _);
        let _ = ShowWindow(hwnd, SW_HIDE);
    })
}

/// 编辑模式结束后，把焦点还给触发前的窗口。
pub fn restore_foreground(app: &AppHandle) {
    let raw = ORIGINAL_FOREGROUND.swap(0, Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    if let Err(error) = run_on_main_thread(app, move || unsafe {
        let hwnd = HWND(raw as *mut _);
        let _ = SetForegroundWindow(hwnd);
    }) {
        tracing::warn!(code = %error.code, "恢复前台窗口失败");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(left: i32, top: i32, right: i32, bottom: i32) -> WorkArea {
        WorkArea {
            left,
            top,
            right,
            bottom,
        }
    }

    fn anchor(x: i32, y: i32) -> POINT {
        POINT { x, y }
    }

    #[test]
    fn places_below_anchor_when_space_allows() {
        let (x, y) = place_popup(anchor(100, 200), work(0, 0, 1920, 1080), 420, 560);
        assert_eq!(x, 100);
        assert_eq!(y, 218);
    }

    #[test]
    fn flips_above_anchor_when_bottom_overflows() {
        let (_, y) = place_popup(anchor(100, 900), work(0, 0, 1920, 1080), 420, 560);
        assert_eq!(y, 900 - 560 - 18);
    }

    #[test]
    fn clamps_to_right_edge() {
        let (x, _) = place_popup(anchor(1900, 200), work(0, 0, 1920, 1080), 420, 560);
        assert_eq!(x, 1920 - 420 - 8);
    }

    #[test]
    fn keeps_negative_secondary_monitor_coordinates() {
        let (x, y) = place_popup(anchor(-1600, 100), work(-1920, 0, 0, 1080), 420, 560);
        assert_eq!(x, -1600);
        assert_eq!(y, 118);
    }

    #[test]
    fn clamps_inside_secondary_monitor() {
        let (x, _) = place_popup(anchor(-10, 100), work(-1920, 0, 0, 1080), 420, 560);
        assert_eq!(x, 0 - 420 - 8);
    }

    #[test]
    fn falls_back_to_work_area_origin_when_smaller_than_popup() {
        let (x, y) = place_popup(anchor(0, 0), work(0, 0, 300, 400), 420, 560);
        assert_eq!(x, 8);
        assert_eq!(y, 8);
    }
}
