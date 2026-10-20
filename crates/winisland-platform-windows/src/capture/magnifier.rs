use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{HRGN, InvalidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Magnification::{
    MAGIMAGEHEADER, MAGTRANSFORM, MW_FILTERMODE_EXCLUDE, MagInitialize, MagSetImageScalingCallback,
    MagSetWindowFilterList, MagSetWindowSource, MagSetWindowTransform, MagUninitialize,
    WC_MAGNIFIERW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
    GetWindowThreadProcessId, IsWindowVisible, LWA_ALPHA, MSG, PM_REMOVE, PeekMessageW,
    RegisterClassW, SWP_NOACTIVATE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowPos,
    TranslateMessage, WINDOW_EX_STYLE, WNDCLASSW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};
use windows::core::{BOOL, PCWSTR, w};

const FILTER_REFRESH: Duration = Duration::from_millis(500);

/// One frame from the magnifier image callback as tightly packed BGRA rows (the header reports
/// RGBA, but the bytes are BGRA).
pub(super) struct PixelFrame {
    pub(super) pixels: Vec<u8>,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) origin: (i32, i32),
}

pub(super) type FrameSink = Box<dyn Fn(PixelFrame) + Send>;

thread_local! {
    static SINK: RefCell<Option<(FrameSink, (i32, i32))>> = const { RefCell::new(None) };
}

struct State {
    source: Mutex<RECT>,
    rate_hz: AtomicU32,
    stop: AtomicBool,
}

/// Magnifier that renders the desktop beneath `source` without this process's windows. With a
/// sink, frames arrive through the image callback from a hidden host, which works where
/// Windows.Graphics.Capture cannot read cloaked windows (Windows 10); otherwise the host is
/// cloaked and captured as an ordinary window.
pub(super) struct Magnifier {
    state: Arc<State>,
    host: HWND,
    thread: Option<JoinHandle<()>>,
}

impl Magnifier {
    pub(super) fn start(
        source: RECT,
        rate_hz: u32,
        sink: Option<FrameSink>,
    ) -> Result<Self, String> {
        let state = Arc::new(State {
            source: Mutex::new(source),
            rate_hz: AtomicU32::new(rate_hz.max(1)),
            stop: AtomicBool::new(false),
        });
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread_state = state.clone();
        let thread = std::thread::Builder::new()
            .name("liquid-magnifier".into())
            .spawn(move || run(thread_state, sender, sink))
            .map_err(|error| format!("Magnifier thread failed: {error}"))?;
        match receiver.recv() {
            Ok(Ok(host)) => Ok(Self {
                state,
                host: HWND(host as *mut _),
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err("Magnifier thread exited during setup".to_string())
            }
        }
    }

    pub(super) fn host(&self) -> HWND {
        self.host
    }

    pub(super) fn set_source(&self, source: RECT) {
        *self.state.source.lock() = source;
    }

    pub(super) fn set_rate(&self, rate_hz: u32) {
        self.state.rate_hz.store(rate_hz.max(1), Ordering::Relaxed);
    }
}

impl Drop for Magnifier {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

extern "system" fn host_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: Forwards the unmodified message to the default window procedure.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn on_frame(
    _hwnd: HWND,
    source: *mut c_void,
    header: MAGIMAGEHEADER,
    _destination: *mut c_void,
    _destination_header: MAGIMAGEHEADER,
    _unclipped: RECT,
    _clipped: RECT,
    _dirty: HRGN,
) -> BOOL {
    let row = header.width as usize * 4;
    let stride = header.stride as usize;
    let height = header.height as usize;
    let end = (header.offset as usize)
        .saturating_add(stride.saturating_mul(height.saturating_sub(1)))
        .saturating_add(row);
    if source.is_null() || row == 0 || height == 0 || stride < row || end > header.cbSize {
        return BOOL(1);
    }
    SINK.with(|slot| {
        let slot = slot.borrow();
        let Some((sink, origin)) = slot.as_ref() else {
            return;
        };
        let mut pixels = Vec::with_capacity(row * header.height as usize);
        for y in 0..header.height as usize {
            // SAFETY: The magnifier guarantees `height` rows of `stride` bytes after `offset`, and
            // each copied row stays within its stride.
            pixels.extend_from_slice(unsafe {
                std::slice::from_raw_parts(
                    source
                        .cast::<u8>()
                        .add(header.offset as usize + y * header.stride as usize),
                    row,
                )
            });
        }
        sink(PixelFrame {
            pixels,
            width: header.width,
            height: header.height,
            origin: *origin,
        });
    });
    BOOL(1)
}

fn run(
    state: Arc<State>,
    ready: std::sync::mpsc::Sender<Result<isize, String>>,
    sink: Option<FrameSink>,
) {
    let callback = sink.is_some();
    // SAFETY: Every handle below is created, used, and destroyed on this thread; the magnifier
    // runtime is initialized before the control is created and released after it is destroyed.
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if !MagInitialize().as_bool() {
            let _ = ready.send(Err("MagInitialize failed".to_string()));
            return;
        }
        let setup = (|| -> Result<(HWND, HWND), String> {
            let instance = GetModuleHandleW(None).map_err(|error| error.to_string())?;
            let class = w!("WinIslandLiquidMagnifierHost");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(host_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let source = *state.source.lock();
            let host = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("WinIsland Liquid Backdrop"),
                if callback {
                    WS_POPUP
                } else {
                    WS_POPUP | WS_VISIBLE
                },
                source.left,
                source.top,
                (source.right - source.left).max(1),
                (source.bottom - source.top).max(1),
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|error| format!("Magnifier host failed: {error}"))?;
            let _ = SetLayeredWindowAttributes(host, COLORREF(0), 255, LWA_ALPHA);
            if !callback {
                let cloak = 1_i32;
                let _ = DwmSetWindowAttribute(
                    host,
                    DWMWA_CLOAK,
                    (&cloak as *const i32).cast(),
                    size_of::<i32>() as u32,
                );
            }
            let magnifier = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(WC_MAGNIFIERW.as_ptr()),
                w!("WinIsland Liquid Magnifier"),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                (source.right - source.left).max(1),
                (source.bottom - source.top).max(1),
                Some(host),
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|error| {
                let _ = DestroyWindow(host);
                format!("Magnifier control failed: {error}")
            })?;
            let mut transform = MAGTRANSFORM::default();
            transform.v[0] = 1.0;
            transform.v[4] = 1.0;
            transform.v[8] = 1.0;
            let _ = MagSetWindowTransform(magnifier, &mut transform);
            Ok((host, magnifier))
        })();
        let (host, magnifier) = match setup {
            Ok(handles) => handles,
            Err(error) => {
                let _ = MagUninitialize();
                let _ = ready.send(Err(error));
                return;
            }
        };
        if let Some(sink) = sink {
            let source = *state.source.lock();
            SINK.with(|slot| *slot.borrow_mut() = Some((sink, (source.left, source.top))));
            let _ = MagSetImageScalingCallback(magnifier, Some(on_frame));
        }
        let mut excluded = own_windows();
        let mut filter_ok = apply_filter(magnifier, &excluded, true);
        let _ = ready.send(Ok(host.0 as isize));

        let mut size = (0, 0);
        let mut next = Instant::now();
        let mut filter_at = Instant::now();
        let mut refilter = false;
        let mut message = MSG::default();
        while !state.stop.load(Ordering::Acquire) {
            let source = *state.source.lock();
            let current = (source.right - source.left, source.bottom - source.top);
            if current != size && current.0 > 0 && current.1 > 0 {
                size = current;
                let _ = SetWindowPos(
                    host,
                    None,
                    source.left,
                    source.top,
                    size.0,
                    size.1,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let _ = SetWindowPos(
                    magnifier,
                    None,
                    0,
                    0,
                    size.0,
                    size.1,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                refilter = true;
            }
            let windows = own_windows();
            if refilter || windows != excluded || filter_at.elapsed() >= FILTER_REFRESH {
                refilter = false;
                filter_at = Instant::now();
                excluded = windows;
                filter_ok = apply_filter(magnifier, &excluded, filter_ok);
            }
            SINK.with(|slot| {
                if let Some((_, origin)) = slot.borrow_mut().as_mut() {
                    *origin = (source.left, source.top);
                }
            });
            let _ = MagSetWindowSource(magnifier, source);
            let _ = InvalidateRect(Some(magnifier), None, false);
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let rate = state.rate_hz.load(Ordering::Relaxed).max(1);
            next += Duration::from_nanos(1_000_000_000 / u64::from(rate));
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now;
            }
        }
        let _ = DestroyWindow(host);
        let _ = MagUninitialize();
        SINK.with(|slot| slot.borrow_mut().take());
    }
}

/// Re-sends the exclusion list. DWM can drop a window's exclusion when it is hidden and shown
/// again or when the magnifier resizes, so this runs on every change and on a timer. Returns
/// whether the call succeeded and warns once when it starts failing.
fn apply_filter(magnifier: HWND, windows: &[(HWND, bool)], was_ok: bool) -> bool {
    let mut handles: Vec<HWND> = windows.iter().map(|(hwnd, _)| *hwnd).collect();
    if handles.is_empty() {
        return was_ok;
    }
    // SAFETY: The magnifier control is live on this thread and the list outlives the call.
    let ok = unsafe {
        MagSetWindowFilterList(
            magnifier,
            MW_FILTERMODE_EXCLUDE,
            handles.len() as i32,
            handles.as_mut_ptr(),
        )
    }
    .as_bool();
    if was_ok && !ok {
        log::warn!("Magnifier could not exclude WinIsland windows");
    }
    ok
}

fn own_windows() -> Vec<(HWND, bool)> {
    extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: data points to the Vec owned by own_windows for the duration of EnumWindows.
        unsafe {
            let windows = &mut *(data.0 as *mut Vec<(HWND, bool)>);
            let mut process = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut process));
            if process == GetCurrentProcessId() {
                windows.push((hwnd, IsWindowVisible(hwnd).as_bool()));
            }
        }
        BOOL(1)
    }
    let mut windows = Vec::new();
    // SAFETY: The callback only appends to the local Vec passed through LPARAM.
    unsafe {
        let _ = EnumWindows(
            Some(visit),
            LPARAM(&mut windows as *mut Vec<(HWND, bool)> as isize),
        );
    }
    windows
}
