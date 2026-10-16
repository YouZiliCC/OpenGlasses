//! Selected-text grabbing (DESIGN §3.3): UIA first, clipboard fallback second.
//! Everything here is blocking and must run on a worker thread.

use crate::types::*;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows::core::{w, Owned, PCWSTR, PWSTR};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{DeleteEnhMetaFile, GetEnhMetaFileBits, SetEnhMetaFileBits, HENHMETAFILE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayDestroy, SafeArrayUnaccessData, CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE,
    CF_DSPMETAFILEPICT, CF_ENHMETAFILE, CF_GDIOBJLAST, CF_METAFILEPICT, CF_OWNERDISPLAY, CF_PALETTE, CF_PRIVATEFIRST,
    CF_UNICODETEXT,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation2, IUIAutomationTextPattern, IUIAutomationTextRange, UIA_TextPatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_INSERT, VK_LWIN,
    VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};

pub enum GrabResult {
    /// `anchor` = union of UIA selection rects (physical px), when available and ≤2000 chars.
    Text { text: String, anchor: Option<PhysRect>, source: SourceInfo },
    /// Definitely nothing selected (UIA TextPattern with an empty selection, or copy produced nothing).
    Empty,
    /// Abort with a toast code: "RELEASE KEYS", "ELEVATED APP", "NO SELECTION".
    Toast(&'static str),
    /// Foreground window is ours (or none): do nothing.
    Ignore,
}

const UIA_MAX_CHARS: i32 = 8000;
const ANCHOR_MAX_CHARS: usize = 2000;
const UIA_WAIT: Duration = Duration::from_millis(500);
const MODIFIER_WAIT: Duration = Duration::from_millis(400);
const SNAPSHOT_BUDGET: Duration = Duration::from_millis(200);
const MAX_FORMAT_BYTES: usize = 16 << 20;
const COPY_WAIT: Duration = Duration::from_millis(250);
const OPEN_RETRY: Duration = Duration::from_millis(100);
/// Longer retry after our copy replaced the user's clipboard: the read and the restore need it open.
const READ_RETRY: Duration = Duration::from_secs(1);

/// Ctrl+C would interrupt a running command in these when nothing is selected.
const CTRL_INSERT_PROCESSES: [&str; 7] = [
    "WindowsTerminal.exe",
    "OpenConsole.exe",
    "conhost.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
    "Code.exe",
];
const CTRL_INSERT_CLASSES: [&str; 2] = ["ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS"];

/// Formats whose clipboard handle is a GDI object, not an HGLOBAL.
const GDI_FORMATS: [u16; 8] = [
    CF_BITMAP.0,
    CF_METAFILEPICT.0,
    CF_PALETTE.0,
    CF_ENHMETAFILE.0,
    CF_DSPBITMAP.0,
    CF_DSPMETAFILEPICT.0,
    CF_DSPENHMETAFILE.0,
    CF_OWNERDISPLAY.0,
];

/// Foreground window info (hwnd, process exe name, title, DWM extended frame bounds,
/// minimized). None if there is no foreground window.
pub fn foreground_source() -> Option<SourceInfo> {
    source_of(foreground_hwnd()).map(|(s, _)| s)
}

/// Current GetForegroundWindow as isize.
pub fn foreground_hwnd() -> isize {
    unsafe { GetForegroundWindow() }.0 as isize
}

/// Rect (DWM extended frame bounds) and minimized flag of a window; None if it no longer exists.
pub fn window_state(hwnd: isize) -> Option<(PhysRect, bool)> {
    let h = hwnd_of(hwnd);
    unsafe {
        if h.is_invalid() || !IsWindow(Some(h)).as_bool() {
            return None;
        }
        let mut r = RECT::default();
        let size = size_of::<RECT>() as u32;
        if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut r).cast(), size).is_err() {
            GetWindowRect(h, &mut r).ok()?;
        }
        Some((phys(r), IsIconic(h).as_bool()))
    }
}

/// Full grab pipeline. Steps 1–4 of DESIGN §3.3, including the elevation check,
/// the terminal Ctrl+Insert rule, modifier wait, clipboard snapshot/restore.
pub fn grab_selection() -> GrabResult {
    let Some((source, pid)) = source_of(foreground_hwnd()) else {
        return GrabResult::Ignore;
    };
    // pid 0: the window died between calls.
    if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
        return GrabResult::Ignore;
    }
    // UIPI silently drops our input and UIA calls into a higher-integrity process.
    if !self_elevated() && target_elevated(pid) {
        return GrabResult::Toast("ELEVATED APP");
    }
    let (text, anchor) = match uia_selection() {
        Some(hit) => hit,
        None => match clipboard_selection(&source) {
            Ok(text) => (text, None),
            Err(code) => return GrabResult::Toast(code),
        },
    };
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    match text.trim() {
        "" => GrabResult::Empty,
        t => GrabResult::Text { text: t.to_string(), anchor, source },
    }
}

/// Window class name of a window (used for terminal / shell detection).
pub fn class_name(hwnd: isize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd_of(hwnd), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

// ---------------------------------------------------------------- window / process

fn hwnd_of(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

fn phys(r: RECT) -> PhysRect {
    PhysRect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
}

/// SourceInfo plus the owning process id.
fn source_of(hwnd: isize) -> Option<(SourceInfo, u32)> {
    let (rect, minimized) = window_state(hwnd)?;
    let h = hwnd_of(hwnd);
    let mut pid = 0;
    let mut title = [0u16; 512];
    let n = unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
        GetWindowTextW(h, &mut title)
    };
    let process = open_process(pid).map(|p| process_name(*p)).unwrap_or_default();
    let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
    Some((SourceInfo { hwnd, process, title, rect, minimized }, pid))
}

fn open_process(pid: u32) -> Option<Owned<HANDLE>> {
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().map(|h| Owned::new(h)) }
}

/// Exe file name (no path).
fn process_name(p: HANDLE) -> String {
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    if unsafe { QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }.is_err() {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit('\\').next().unwrap_or_default().to_string()
}

fn token_elevated(p: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(p, TOKEN_QUERY, &mut token).ok()?;
        let token = Owned::new(token);
        let mut e = TOKEN_ELEVATION::default();
        let mut len = 0;
        let size = size_of::<TOKEN_ELEVATION>() as u32;
        GetTokenInformation(*token, TokenElevation, Some((&raw mut e).cast()), size, &mut len).ok()?;
        Some(e.TokenIsElevated != 0)
    }
}

fn self_elevated() -> bool {
    static ELEVATED: OnceLock<bool> = OnceLock::new();
    // GetCurrentProcess is a pseudo handle: never closed.
    *ELEVATED.get_or_init(|| token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false))
}

/// Elevated, or we are not even allowed to read its token (same outcome for us).
fn target_elevated(pid: u32) -> bool {
    open_process(pid).and_then(|p| token_elevated(*p)).unwrap_or(true)
}

// ---------------------------------------------------------------- UIA

/// `Some((text, anchor))` from UIA (empty text = definitely no selection);
/// None = UIA unavailable, failed, busy or too slow.
type UiaHit = Option<(String, Option<PhysRect>)>;

static UIA_BUSY: AtomicBool = AtomicBool::new(false);

fn uia_selection() -> UiaHit {
    static UIA: OnceLock<Sender<Sender<UiaHit>>> = OnceLock::new();
    if UIA_BUSY.swap(true, Ordering::AcqRel) {
        return None;
    }
    let tx = UIA.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("og-uia".into()).spawn(move || uia_thread(rx)).expect("spawn og-uia");
        tx
    });
    let (reply_tx, reply_rx) = mpsc::channel();
    let _ = tx.send(reply_tx);
    // On timeout the UIA thread stays busy until its call returns; grabs meanwhile skip UIA.
    reply_rx.recv_timeout(UIA_WAIT).ok().flatten()
}

/// The one long-lived MTA thread that owns the UIA client.
fn uia_thread(rx: Receiver<Sender<UiaHit>>) {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let automation = unsafe { new_automation() }.ok();
    for reply in rx {
        let hit = automation.as_ref().and_then(|a| unsafe { focused_selection(a) }.ok());
        UIA_BUSY.store(false, Ordering::Release);
        let _ = reply.send(hit);
    }
}

unsafe fn new_automation() -> windows::core::Result<IUIAutomation2> {
    let a: IUIAutomation2 = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
    a.SetConnectionTimeout(150)?;
    a.SetTransactionTimeout(300)?;
    Ok(a)
}

/// Err = no focused element / no TextPattern / provider error -> clipboard fallback.
unsafe fn focused_selection(a: &IUIAutomation2) -> windows::core::Result<(String, Option<PhysRect>)> {
    let el = a.GetFocusedElement()?;
    if el.CurrentIsPassword()?.as_bool() {
        // Never read password fields, not even through the clipboard.
        return Ok((String::new(), None));
    }
    let pattern: IUIAutomationTextPattern = el.GetCurrentPatternAs(UIA_TextPatternId)?;
    let ranges = pattern.GetSelection()?;
    let (mut text, mut used, mut hits) = (String::new(), 0, Vec::new());
    for i in 0..ranges.Length()? {
        if used >= UIA_MAX_CHARS {
            break;
        }
        let range = ranges.GetElement(i)?;
        let t = range.GetText(UIA_MAX_CHARS - used)?;
        if t.is_empty() {
            continue; // degenerate range (caret)
        }
        used += t.len() as i32;
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&String::from_utf16_lossy(&t));
        hits.push(range);
    }
    let anchor = if text.chars().count() <= ANCHOR_MAX_CHARS { selection_rect(&hits) } else { None };
    Ok((text, anchor))
}

/// Union of the UIA selection ranges' bounding rects (SAFEARRAY of f64 x,y,w,h quadruples,
/// physical px).
unsafe fn selection_rect(ranges: &[IUIAutomationTextRange]) -> Option<PhysRect> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for range in ranges {
        let Ok(psa) = range.GetBoundingRectangles() else { continue };
        if psa.is_null() {
            continue;
        }
        let n = (*psa).rgsabound[0].cElements as usize;
        let mut data = std::ptr::null_mut();
        if (*psa).cDims == 1 && n >= 4 && SafeArrayAccessData(psa, &mut data).is_ok() {
            for &[x, y, w, h] in std::slice::from_raw_parts(data.cast::<f64>(), n).as_chunks::<4>().0 {
                if w > 0.0 && h > 0.0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + w);
                    y1 = y1.max(y + h);
                }
            }
            let _ = SafeArrayUnaccessData(psa);
        }
        let _ = SafeArrayDestroy(psa);
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (l, t) = (x0.floor() as i32, y0.floor() as i32);
    Some(PhysRect { x: l, y: t, w: x1.ceil() as i32 - l, h: y1.ceil() as i32 - t })
}

// ---------------------------------------------------------------- clipboard fallback

/// Ok("") = the copy produced nothing; Err = toast code.
fn clipboard_selection(src: &SourceInfo) -> Result<String, &'static str> {
    if !modifiers_released() {
        return Err("RELEASE KEYS");
    }
    let win = MsgWindow::new().ok_or("NO SELECTION")?;
    let (seq, saved) = bounded_snapshot().ok_or("NO SELECTION")?;
    // The user may have switched windows while we waited; never copy from (or signal) another app.
    if foreground_hwnd() != src.hwnd {
        return Err("NO SELECTION");
    }
    send_copy(wants_ctrl_insert(src));
    let t0 = Instant::now();
    while unsafe { GetClipboardSequenceNumber() } == seq {
        if t0.elapsed() >= COPY_WAIT {
            return Ok(String::new()); // nothing changed: nothing to restore
        }
        sleep(Duration::from_millis(5));
    }
    // From here the user's clipboard has been replaced, so the restore is mandatory.
    let _open = open_clipboard(win.0, READ_RETRY).ok_or("NO SELECTION")?;
    let text = unsafe { read_unicode_text() }.unwrap_or_default();
    unsafe { restore(&saved) };
    Ok(text)
}

/// Snapshot on a helper thread: GetClipboardData on a delay-rendered format (e.g. a large Excel
/// copy) can block for seconds, and the grab must not hang with it. On timeout the helper
/// finishes on its own and closes the clipboard from its own thread.
fn bounded_snapshot() -> Option<(u32, Saved)> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = MsgWindow::new().and_then(|w| snapshot(w.0));
        let _ = tx.send(r);
    });
    rx.recv_timeout(SNAPSHOT_BUDGET + OPEN_RETRY).ok().flatten()
}

/// Wait until Alt/Ctrl/Shift/Win are physically up; false if still held after MODIFIER_WAIT.
fn modifiers_released() -> bool {
    let t0 = Instant::now();
    loop {
        let held = [VK_MENU, VK_CONTROL, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|k| unsafe { GetAsyncKeyState(k.0 as i32) } < 0);
        if !held {
            return true;
        }
        if t0.elapsed() >= MODIFIER_WAIT {
            return false;
        }
        sleep(Duration::from_millis(10));
    }
}

fn wants_ctrl_insert(src: &SourceInfo) -> bool {
    CTRL_INSERT_PROCESSES.iter().any(|p| p.eq_ignore_ascii_case(&src.process))
        || CTRL_INSERT_CLASSES.contains(&class_name(src.hwnd).as_str())
}

fn send_copy(ctrl_insert: bool) {
    // Insert must be sent as an extended key, or it arrives as numpad 0/Ins.
    let (vk, ext) = if ctrl_insert { (VK_INSERT, KEYEVENTF_EXTENDEDKEY) } else { (VK_C, KEYBD_EVENT_FLAGS(0)) };
    let key = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        key(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key(vk, ext),
        key(vk, ext | KEYEVENTF_KEYUP),
        key(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
}

/// Message-only window to own the clipboard while we hold it open (destroyed on drop).
struct MsgWindow(HWND);

impl MsgWindow {
    fn new() -> Option<Self> {
        let (ex, style, parent) = (WINDOW_EX_STYLE(0), WINDOW_STYLE(0), Some(HWND_MESSAGE));
        unsafe { CreateWindowExW(ex, w!("STATIC"), PCWSTR::null(), style, 0, 0, 0, 0, parent, None, None, None) }
            .ok()
            .map(Self)
    }
}

impl Drop for MsgWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

struct ClipboardOpen;

impl Drop for ClipboardOpen {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// OpenClipboard, retrying every 10 ms for up to `timeout`.
fn open_clipboard(owner: HWND, timeout: Duration) -> Option<ClipboardOpen> {
    let t0 = Instant::now();
    loop {
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            return Some(ClipboardOpen);
        }
        if t0.elapsed() >= timeout {
            return None;
        }
        sleep(Duration::from_millis(10));
    }
}

/// (format, bytes) of every HGLOBAL-backed clipboard format.
type Saved = Vec<(u32, Vec<u8>)>;

/// (sequence number, saved formats). None = too big, too slow or clipboard busy.
/// The sequence number is read while we hold the clipboard, so it matches the snapshot.
fn snapshot(owner: HWND) -> Option<(u32, Saved)> {
    let t0 = Instant::now();
    let _open = open_clipboard(owner, OPEN_RETRY)?;
    let seq = unsafe { GetClipboardSequenceNumber() };
    let mut saved = Vec::new();
    let mut fmt = 0;
    loop {
        fmt = unsafe { EnumClipboardFormats(fmt) };
        if fmt == 0 {
            break;
        }
        if t0.elapsed() > SNAPSHOT_BUDGET {
            return None;
        }
        if fmt == CF_ENHMETAFILE.0 as u32 {
            // "Copy as Picture" content is an EMF handle, not an HGLOBAL: save its bits.
            let Ok(h) = (unsafe { GetClipboardData(fmt) }) else { continue };
            let emf = HENHMETAFILE(h.0);
            let n = unsafe { GetEnhMetaFileBits(emf, None) } as usize;
            if n > MAX_FORMAT_BYTES {
                return None;
            }
            let mut bytes = vec![0u8; n];
            if n > 0 && unsafe { GetEnhMetaFileBits(emf, Some(&mut bytes)) } as usize == n {
                saved.push((fmt, bytes));
            }
            continue;
        }
        if !is_hglobal_format(fmt) {
            continue;
        }
        let Ok(h) = (unsafe { GetClipboardData(fmt) }) else { continue };
        let g = HGLOBAL(h.0);
        let size = unsafe { GlobalSize(g) };
        if size > MAX_FORMAT_BYTES {
            return None;
        }
        if let Some(bytes) = unsafe { global_bytes(g, size) } {
            saved.push((fmt, bytes));
        }
    }
    (t0.elapsed() <= SNAPSHOT_BUDGET).then_some((seq, saved))
}

/// GDI formats hold GDI handles; the private (0x200–0x2FF) and GDI-object (0x300–0x3FF)
/// ranges hold app-defined handles the system does not manage.
fn is_hglobal_format(fmt: u32) -> bool {
    !GDI_FORMATS.contains(&(fmt as u16)) && !(CF_PRIVATEFIRST.0 as u32..=CF_GDIOBJLAST.0 as u32).contains(&fmt)
}

unsafe fn global_bytes(g: HGLOBAL, size: usize) -> Option<Vec<u8>> {
    if size == 0 {
        return Some(Vec::new());
    }
    let p = GlobalLock(g);
    if p.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(p.cast::<u8>(), size).to_vec();
    let _ = GlobalUnlock(g);
    Some(bytes)
}

/// Clipboard must be open.
unsafe fn read_unicode_text() -> Option<String> {
    let g = HGLOBAL(GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?.0);
    let bytes = global_bytes(g, GlobalSize(g))?;
    let wide: Vec<u16> =
        bytes.as_chunks::<2>().0.iter().map(|&c| u16::from_ne_bytes(c)).take_while(|&c| c != 0).collect();
    Some(String::from_utf16_lossy(&wide))
}

/// Clipboard must be open with our window as owner.
unsafe fn restore(saved: &[(u32, Vec<u8>)]) {
    let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
    if EmptyClipboard().is_err() {
        return;
    }
    for (fmt, bytes) in saved.iter().filter(|(f, _)| *f != exclude) {
        set_data(*fmt, bytes);
    }
    // Keeps the restore itself out of Win+V history and cloud clipboard sync.
    if exclude != 0 {
        set_data(exclude, &[]);
    }
}

unsafe fn set_data(fmt: u32, bytes: &[u8]) {
    if fmt == CF_ENHMETAFILE.0 as u32 {
        // CF_METAFILEPICT is re-synthesized by the system from the EMF.
        let emf = SetEnhMetaFileBits(bytes);
        if !emf.is_invalid() && SetClipboardData(fmt, Some(HANDLE(emf.0))).is_err() {
            let _ = DeleteEnhMetaFile(Some(emf));
        }
        return;
    }
    let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else { return };
    if !bytes.is_empty() {
        let p = GlobalLock(g);
        if p.is_null() {
            let _ = GlobalFree(Some(g));
            return;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(g);
    }
    // On success the system owns the memory.
    if SetClipboardData(fmt, Some(HANDLE(g.0))).is_err() {
        let _ = GlobalFree(Some(g));
    }
}
