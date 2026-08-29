#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod markdown;
mod render;

use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use markdown::Document;
use render::{FontChoice, LayoutDocument, LineSpacingChoice, Renderer, ViewSettings};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow, PAINTSTRUCT, UpdateWindow,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Controls::Dialogs::{
    FINDMSGSTRINGW, FINDREPLACE_FLAGS, FINDREPLACEW, FR_DIALOGTERM, FR_DOWN, FR_FINDNEXT,
    FR_MATCHCASE, FR_WHOLEWORD, FindTextW, GetOpenFileNameW, OFN_DONTADDTORECENT, OFN_EXPLORER,
    OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Controls::SetScrollInfo;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_A, VK_C, VK_CONTROL, VK_DOWN, VK_END,
    VK_ESCAPE, VK_F, VK_F3, VK_F5, VK_HOME, VK_NEXT, VK_O, VK_OEM_COMMA, VK_PRIOR, VK_Q, VK_SHIFT,
    VK_SPACE, VK_UP, VK_W,
};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreatePopupMenu,
    CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetClientRect, GetCursorPos, GetMessageW, GetScrollInfo, HMENU, IDC_IBEAM, IsDialogMessageW,
    KillTimer, LoadCursorW, LoadIconW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, MessageBoxW, PostQuitMessage,
    RegisterClassExW, RegisterWindowMessageW, SB_BOTTOM, SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN,
    SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP, SB_VERT, SCROLLINFO, SIF_PAGE, SIF_POS,
    SIF_RANGE, SIF_TRACKPOS, SIZE_MINIMIZED, SW_RESTORE, SW_SHOWDEFAULT, SWP_NOACTIVATE,
    SWP_NOZORDER, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
    ShowWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE,
    WM_CONTEXTMENU, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_DROPFILES, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WM_SIZE, WM_TIMER, WM_VSCROLL, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
    WS_VSCROLL,
};
use windows::core::{PCWSTR, PWSTR, w};

const CLASS_NAME: PCWSTR = w!("QuietMD.Native.Window");
const DEFAULT_DPI: u32 = 96;
const INITIAL_WINDOW_WIDTH: i32 = 1000;
const INITIAL_WINDOW_HEIGHT: i32 = 900;
const RELOAD_TIMER: usize = 1;
const RELAYOUT_TIMER: usize = 2;
const RELAYOUT_DELAY_MS: u32 = 100;
// winresource's application icon ID, also set explicitly in build.rs.
const APP_ICON_RESOURCE_ID: usize = 1;
const FIND_BUFFER_LENGTH: usize = 512;
const FONT_SIZES: [u8; 6] = [11, 13, 15, 17, 19, 21];
const MAX_FILE_SIZE: u64 = 32 * 1024 * 1024;
const EMPTY_DOCUMENT_MARKDOWN: &str =
    "# Open Markdown\n\nDrop a file here\n\nCtrl+O to browse / right-click for menu";
const CMD_COPY: u32 = 901;
const CMD_SELECT_ALL: u32 = 902;
const CMD_CLEAR_SELECTION: u32 = 903;
const CMD_FIND: u32 = 904;
const CMD_FONT_BIZ_UD_GOTHIC: u32 = 1000;
const CMD_FONT_SEGOE_UI: u32 = 1001;
const CMD_FONT_YU_GOTHIC_UI: u32 = 1002;
const CMD_FONT_MEIRYO: u32 = 1003;
const CMD_FONT_ARIAL: u32 = 1004;
const CMD_FONT_GEORGIA: u32 = 1005;
const CMD_FONT_CONSOLAS: u32 = 1006;
const CMD_SIZE_11: u32 = 1099;
const CMD_SIZE_13: u32 = 1100;
const CMD_SIZE_15: u32 = 1101;
const CMD_SIZE_17: u32 = 1102;
const CMD_SIZE_19: u32 = 1103;
const CMD_SIZE_21: u32 = 1104;
const CMD_SPACING_COMPACT: u32 = 1201;
const CMD_SPACING_STANDARD: u32 = 1202;
const CMD_SPACING_RELAXED: u32 = 1203;
const CMD_SETTINGS_RESET: u32 = 1301;
const CMD_WINDOW_LEFT_THIRD: u32 = 1401;
const CMD_WINDOW_RIGHT_THIRD: u32 = 1402;
const CMD_WINDOW_LEFT_HALF: u32 = 1403;
const CMD_WINDOW_RIGHT_HALF: u32 = 1404;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowLayout {
    LeftThird,
    RightThird,
    LeftHalf,
    RightHalf,
}

struct AppState {
    renderer: Renderer,
    document: Document,
    layout: Option<LayoutDocument>,
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
    client_width_px: u32,
    client_height_px: u32,
    dpi: u32,
    scroll_y: f32,
    selection_anchor: Option<u32>,
    selection_active: Option<u32>,
    selecting: bool,
    find_message: u32,
    find_dialog: Option<HWND>,
    find_request: FINDREPLACEW,
    find_buffer: [u16; FIND_BUFFER_LENGTH],
    find_options: u32,
    zoom_wheel_delta: i32,
}

impl AppState {
    fn new(initial_path: Option<PathBuf>, find_message: u32) -> Result<Self, String> {
        let renderer = Renderer::new().map_err(|error| error.to_string())?;
        let mut state = Self {
            renderer,
            document: markdown::parse(EMPTY_DOCUMENT_MARKDOWN),
            layout: None,
            path: None,
            modified: None,
            client_width_px: 900,
            client_height_px: 700,
            dpi: DEFAULT_DPI,
            scroll_y: 0.0,
            selection_anchor: None,
            selection_active: None,
            selecting: false,
            find_message,
            find_dialog: None,
            find_request: FINDREPLACEW::default(),
            find_buffer: [0; FIND_BUFFER_LENGTH],
            find_options: FR_DOWN.0,
            zoom_wheel_delta: 0,
        };
        if let Some(path) = initial_path {
            state.load_path(path, false)?;
        }
        Ok(state)
    }

    fn load_path(&mut self, path: PathBuf, preserve_view: bool) -> Result<(), String> {
        let previous_scroll = self.scroll_y;
        let previous_selection = if preserve_view {
            self.selection_range().and_then(|(anchor, active)| {
                self.layout.as_ref().map(|layout| {
                    let (start, end) = if anchor <= active {
                        (anchor, active)
                    } else {
                        (active, anchor)
                    };
                    (start, end, layout.selected_text(start, end))
                })
            })
        } else {
            None
        };
        let metadata = std::fs::metadata(&path)
            .map_err(|error| format!("Could not inspect the file.\n\n{error}"))?;
        if metadata.len() > MAX_FILE_SIZE {
            return Err("The file is too large (maximum: 32 MiB).".to_string());
        }
        let bytes =
            std::fs::read(&path).map_err(|error| format!("Could not read the file.\n\n{error}"))?;
        let text = decode_text(bytes)?;
        self.document = markdown::parse(&text);
        self.modified = metadata.modified().ok();
        self.path = Some(path);
        self.selection_anchor = None;
        self.selection_active = None;
        self.selecting = false;
        self.relayout();
        if preserve_view {
            self.scroll_y = previous_scroll.clamp(0.0, self.max_scroll());
            if let (Some((start, end, selected)), Some(layout)) =
                (previous_selection, self.layout.as_ref())
                && end <= layout.text_len()
                && layout.selected_text(start, end) == selected
            {
                self.selection_anchor = Some(start);
                self.selection_active = Some(end);
            }
        } else {
            self.scroll_y = 0.0;
        }
        Ok(())
    }

    fn open_path(&mut self, hwnd: HWND, path: PathBuf) {
        match self.load_path(path, false) {
            Ok(()) => {
                unsafe {
                    SetTimer(Some(hwnd), RELOAD_TIMER, 1000, None);
                }
                self.update_title(hwnd);
                self.update_scrollbar(hwnd);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            Err(message) => show_error(Some(hwnd), &message),
        }
    }

    fn reload(&mut self, hwnd: HWND, only_if_changed: bool) {
        let Some(path) = self.path.clone() else {
            return;
        };
        if only_if_changed {
            let next_modified = std::fs::metadata(&path)
                .ok()
                .and_then(|metadata| metadata.modified().ok());
            if next_modified.is_none() || next_modified == self.modified {
                return;
            }
        }
        match self.load_path(path, true) {
            Ok(()) => {
                self.update_title(hwnd);
                self.update_scrollbar(hwnd);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            Err(message) => show_error(Some(hwnd), &message),
        }
    }

    fn resize(&mut self, hwnd: HWND, width: u32, height: u32) {
        self.resize_surface(width, height);
        self.relayout();
        self.set_scroll(hwnd, self.scroll_y);
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.client_width_px = width.max(1);
        self.client_height_px = height.max(1);
        let _ = self
            .renderer
            .resize(self.client_width_px, self.client_height_px);
    }

    fn finish_deferred_resize(&mut self, hwnd: HWND) {
        self.relayout();
        self.set_scroll(hwnd, self.scroll_y);
    }

    fn set_dpi(&mut self, dpi: u32) {
        self.dpi = normalized_dpi(dpi);
        self.renderer.set_dpi(self.dpi);
    }

    fn client_width_dip(&self) -> f32 {
        pixels_to_dips(self.client_width_px as f32, self.dpi)
    }

    fn client_height_dip(&self) -> f32 {
        pixels_to_dips(self.client_height_px as f32, self.dpi)
    }

    fn relayout(&mut self) {
        self.layout = self
            .renderer
            .layout(&self.document, self.client_width_dip())
            .ok();
    }

    fn max_scroll(&self) -> f32 {
        self.layout
            .as_ref()
            .map(|layout| (layout.total_height - self.client_height_dip()).max(0.0))
            .unwrap_or(0.0)
    }

    fn set_scroll(&mut self, hwnd: HWND, value: f32) {
        self.scroll_y = value.clamp(0.0, self.max_scroll());
        self.update_scrollbar(hwnd);
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    fn selection_range(&self) -> Option<(u32, u32)> {
        let anchor = self.selection_anchor?;
        let active = self.selection_active?;
        (anchor != active).then_some((anchor, active))
    }

    fn text_position_at(&self, x: f32, y: f32) -> Option<u32> {
        let layout = self.layout.as_ref()?;
        let x = pixels_to_dips(x, self.dpi);
        let y = pixels_to_dips(y, self.dpi);
        self.renderer.hit_test(layout, x, y + self.scroll_y).ok()
    }

    fn begin_selection(&mut self, hwnd: HWND, x: f32, y: f32) {
        let Some(position) = self.text_position_at(x, y) else {
            return;
        };
        self.selection_anchor = Some(position);
        self.selection_active = Some(position);
        self.selecting = true;
        unsafe {
            let _ = SetFocus(Some(hwnd));
            SetCapture(hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    fn update_selection(&mut self, hwnd: HWND, x: f32, y: f32) {
        if !self.selecting {
            return;
        }
        if let Some(position) = self.text_position_at(x, y) {
            self.selection_active = Some(position);
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }

    fn end_selection(&mut self, hwnd: HWND, x: f32, y: f32) {
        self.update_selection(hwnd, x, y);
        self.selecting = false;
        unsafe {
            let _ = ReleaseCapture();
        }
    }

    fn select_line_at(&mut self, hwnd: HWND, y: f32) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let y = pixels_to_dips(y, self.dpi) + self.scroll_y;
        let Ok(Some((start, end))) = self.renderer.line_range_at(layout, y) else {
            return;
        };
        if start == end {
            return;
        }
        self.selection_anchor = Some(start);
        self.selection_active = Some(end);
        self.selecting = false;
        unsafe {
            let _ = SetFocus(Some(hwnd));
            let _ = ReleaseCapture();
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    fn clear_selection(&mut self, hwnd: HWND) {
        self.selection_anchor = None;
        self.selection_active = None;
        self.selecting = false;
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    fn select_all(&mut self, hwnd: HWND) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        self.selection_anchor = Some(0);
        self.selection_active = Some(layout.text_len());
        self.selecting = false;
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    fn copy_selection(&self, hwnd: HWND) {
        let Some((anchor, active)) = self.selection_range() else {
            return;
        };
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let text = layout.selected_text(anchor, active);
        if text.is_empty() {
            return;
        }
        if let Err(message) = copy_utf16_to_clipboard(hwnd, &text) {
            show_error(Some(hwnd), &message);
        }
    }

    fn apply_view_settings(&mut self, hwnd: HWND, settings: ViewSettings) {
        if settings == self.renderer.settings() {
            return;
        }
        if let Err(error) = self.renderer.set_settings(settings) {
            show_error(
                Some(hwnd),
                &format!("Could not apply display settings.\n\n{error}"),
            );
            return;
        }
        self.relayout();
        self.set_scroll(hwnd, self.scroll_y);
    }

    fn adjust_text_size_from_wheel(&mut self, hwnd: HWND, delta: i32) {
        self.zoom_wheel_delta += delta;
        let steps = self.zoom_wheel_delta / 120;
        self.zoom_wheel_delta %= 120;
        if steps == 0 {
            return;
        }
        let current = self.renderer.settings();
        let mut next = current;
        next.font_size = stepped_font_size(current.font_size, steps);
        self.apply_view_settings(hwnd, next);
    }

    fn show_find_dialog(&mut self, hwnd: HWND) {
        if let Some(dialog) = self.find_dialog {
            unsafe {
                let _ = SetForegroundWindow(dialog);
            }
            return;
        }

        self.find_request = FINDREPLACEW {
            lStructSize: size_of::<FINDREPLACEW>() as u32,
            hwndOwner: hwnd,
            Flags: FINDREPLACE_FLAGS(self.find_options),
            lpstrFindWhat: PWSTR(self.find_buffer.as_mut_ptr()),
            wFindWhatLen: FIND_BUFFER_LENGTH as u16,
            ..Default::default()
        };
        let dialog = unsafe { FindTextW(&mut self.find_request) };
        if dialog.0.is_null() {
            show_error(Some(hwnd), "Could not open Find.");
            return;
        }
        self.find_dialog = Some(dialog);
    }

    fn handle_find_message(&mut self, hwnd: HWND) {
        let flags = self.find_request.Flags;
        if flags.contains(FR_DIALOGTERM) {
            self.find_dialog = None;
            return;
        }
        if !flags.contains(FR_FINDNEXT) {
            return;
        }
        self.find_options = flags.0 & (FR_DOWN.0 | FR_MATCHCASE.0 | FR_WHOLEWORD.0);
        self.find_next(
            hwnd,
            flags.contains(FR_DOWN),
            flags.contains(FR_MATCHCASE),
            flags.contains(FR_WHOLEWORD),
        );
    }

    fn repeat_find(&mut self, hwnd: HWND, reverse: bool) {
        if self.find_buffer.first() == Some(&0) {
            self.show_find_dialog(hwnd);
            return;
        }
        let default_forward = self.find_options & FR_DOWN.0 != 0;
        self.find_next(
            hwnd,
            if reverse {
                !default_forward
            } else {
                default_forward
            },
            self.find_options & FR_MATCHCASE.0 != 0,
            self.find_options & FR_WHOLEWORD.0 != 0,
        );
    }

    fn find_next(&mut self, hwnd: HWND, forward: bool, match_case: bool, whole_word: bool) {
        let query_length = self
            .find_buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(self.find_buffer.len());
        if query_length == 0 {
            return;
        }
        let start = self
            .selection_range()
            .map(|(anchor, active)| {
                if forward {
                    anchor.max(active)
                } else {
                    anchor.min(active)
                }
            })
            .unwrap_or_else(|| {
                if forward {
                    0
                } else {
                    self.layout
                        .as_ref()
                        .map(LayoutDocument::text_len)
                        .unwrap_or(0)
                }
            });
        let found = self.layout.as_ref().and_then(|layout| {
            let range = layout.find_text(
                &self.find_buffer[..query_length],
                start,
                forward,
                match_case,
                whole_word,
            )?;
            let vertical = layout.vertical_range_for_position(range.0).ok().flatten();
            Some((range, vertical))
        });
        let Some(((start, end), vertical)) = found else {
            let query = String::from_utf16_lossy(&self.find_buffer[..query_length]);
            show_information(Some(hwnd), &format!("Cannot find “{query}”."));
            return;
        };

        self.selection_anchor = Some(start);
        self.selection_active = Some(end);
        self.selecting = false;
        let mut next_scroll = self.scroll_y;
        if let Some((top, bottom)) = vertical {
            let margin = 20.0;
            if top < self.scroll_y + margin {
                next_scroll = (top - margin).max(0.0);
            } else if bottom > self.scroll_y + self.client_height_dip() - margin {
                next_scroll = bottom - self.client_height_dip() + margin;
            }
        }
        self.set_scroll(hwnd, next_scroll);
    }

    fn update_scrollbar(&self, hwnd: HWND) {
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: self
                .layout
                .as_ref()
                .map(|layout| layout.total_height.ceil() as i32)
                .unwrap_or(0),
            nPage: self.client_height_dip().round().max(1.0) as u32,
            nPos: self.scroll_y.round() as i32,
            ..Default::default()
        };
        unsafe {
            SetScrollInfo(hwnd, SB_VERT, &info, true);
        }
    }

    fn update_title(&self, hwnd: HWND) {
        let title = window_title(self.path.as_deref());
        let wide = wide_null(&title);
        unsafe {
            let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
        }
    }

    fn paint(&mut self, hwnd: HWND) {
        let selection = self.selection_range();
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let _ = self.renderer.paint(
            hwnd,
            layout,
            self.client_width_px,
            self.client_height_px,
            self.client_height_dip(),
            self.scroll_y,
            selection,
        );
    }
}

fn main() {
    if let Err(message) = run() {
        show_error(None, &message);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|value| value == "--self-test") {
        let document = markdown::parse("# test\n\n- one\n- two");
        if document.blocks.len() != 3 {
            return Err("Internal self-test failed.".to_string());
        }
        let mut renderer = Renderer::new().map_err(|error| error.to_string())?;
        renderer
            .set_settings(ViewSettings {
                font: FontChoice::Meiryo,
                font_size: 21,
                line_spacing: LineSpacingChoice::Relaxed,
            })
            .map_err(|error| error.to_string())?;
        let layout = renderer
            .layout(&document, 900.0)
            .map_err(|error| error.to_string())?;
        if String::from_utf16(&layout.selected_text(0, 4))
            .ok()
            .as_deref()
            != Some("test")
        {
            return Err("Selection self-test failed.".to_string());
        }
        return Ok(());
    }
    let initial_path = args.first().map(PathBuf::from);
    let find_message = unsafe { RegisterWindowMessageW(FINDMSGSTRINGW) };
    if find_message == 0 {
        return Err("Could not register the Find message.".to_string());
    }

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let hmodule = unsafe { GetModuleHandleW(None) }.map_err(|error| error.to_string())?;
    let hinstance = hmodule.into();
    let cursor = unsafe { LoadCursorW(None, IDC_IBEAM) }.map_err(|error| error.to_string())?;
    let icon = unsafe {
        LoadIconW(
            Some(hinstance),
            PCWSTR(std::ptr::without_provenance(APP_ICON_RESOURCE_ID)),
        )
    }
    .map_err(|error| error.to_string())?;
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_DBLCLKS | CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: hinstance,
        hCursor: cursor,
        hIcon: icon,
        hIconSm: icon,
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        return Err("Could not register the window class.".to_string());
    }

    let mut state = Box::new(AppState::new(initial_path, find_message)?);
    let state_ptr = std::ptr::from_mut(state.as_mut());
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("QuietMD"),
            WS_OVERLAPPEDWINDOW | WS_VSCROLL,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            INITIAL_WINDOW_WIDTH,
            INITIAL_WINDOW_HEIGHT,
            None,
            None,
            Some(hinstance),
            Some(state_ptr.cast()),
        )
    }
    .map_err(|error| error.to_string())?;

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
        let _ = UpdateWindow(hwnd);
    }

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        let find_dialog = state.find_dialog;
        if let Some(dialog) = find_dialog
            && unsafe { IsDialogMessageW(dialog, &message) }.as_bool()
        {
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }

    let state_ptr =
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) }
            as *mut AppState;
    let mut state = unsafe { state_ptr.as_mut() };

    if state
        .as_ref()
        .is_some_and(|state| message == state.find_message)
    {
        if let Some(state) = state.as_deref_mut() {
            state.handle_find_message(hwnd);
        }
        return LRESULT(0);
    }

    match message {
        WM_CREATE => {
            unsafe {
                DragAcceptFiles(hwnd, true);
            }
            if let Some(state) = state {
                if state.path.is_some() {
                    unsafe {
                        SetTimer(Some(hwnd), RELOAD_TIMER, 1000, None);
                    }
                }
                state.set_dpi(unsafe { GetDpiForWindow(hwnd) });
                let mut rect = RECT::default();
                if unsafe { GetClientRect(hwnd, &mut rect) }.is_ok() {
                    state.resize(
                        hwnd,
                        (rect.right - rect.left).max(1) as u32,
                        (rect.bottom - rect.top).max(1) as u32,
                    );
                }
                state.update_title(hwnd);
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(state) = state {
                state.set_dpi((wparam.0 & 0xffff) as u32);
            }
            let suggested = unsafe { &*(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }
        WM_SIZE => {
            if wparam.0 as u32 != SIZE_MINIMIZED
                && let Some(state) = state
            {
                let packed = lparam.0 as usize;
                state.resize_surface(
                    (packed & 0xffff).max(1) as u32,
                    ((packed >> 16) & 0xffff).max(1) as u32,
                );
                unsafe {
                    SetTimer(Some(hwnd), RELAYOUT_TIMER, RELAYOUT_DELAY_MS, None);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            unsafe {
                BeginPaint(hwnd, &mut paint);
            }
            if let Some(state) = state {
                state.paint(hwnd);
            }
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            if let Some(state) = state {
                let (x, y) = mouse_coordinates(lparam);
                state.begin_selection(hwnd, x, y);
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            if let Some(state) = state {
                let (_, y) = mouse_coordinates(lparam);
                state.select_line_at(hwnd, y);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(state) = state {
                let (x, y) = mouse_coordinates(lparam);
                state.update_selection(hwnd, x, y);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(state) = state {
                let (x, y) = mouse_coordinates(lparam);
                state.end_selection(hwnd, x, y);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(state) = state {
                let delta = ((wparam.0 >> 16) & 0xffff) as u16 as i16 as i32;
                let control = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                if control {
                    state.adjust_text_size_from_wheel(hwnd, delta);
                } else {
                    state.zoom_wheel_delta = 0;
                    state.set_scroll(hwnd, state.scroll_y - delta as f32 * 64.0 / 120.0);
                }
            }
            LRESULT(0)
        }
        WM_VSCROLL => {
            if let Some(state) = state {
                let command = (wparam.0 & 0xffff) as u32;
                let next = if command == SB_LINEUP.0 as u32 {
                    state.scroll_y - 48.0
                } else if command == SB_LINEDOWN.0 as u32 {
                    state.scroll_y + 48.0
                } else if command == SB_PAGEUP.0 as u32 {
                    state.scroll_y - state.client_height_dip() * 0.85
                } else if command == SB_PAGEDOWN.0 as u32 {
                    state.scroll_y + state.client_height_dip() * 0.85
                } else if command == SB_TOP.0 as u32 {
                    0.0
                } else if command == SB_BOTTOM.0 as u32 {
                    state.max_scroll()
                } else if command == SB_THUMBTRACK.0 as u32 || command == SB_THUMBPOSITION.0 as u32
                {
                    let mut info = SCROLLINFO {
                        cbSize: size_of::<SCROLLINFO>() as u32,
                        fMask: SIF_TRACKPOS,
                        ..Default::default()
                    };
                    if unsafe { GetScrollInfo(hwnd, SB_VERT, &mut info) }.is_ok() {
                        info.nTrackPos as f32
                    } else {
                        state.scroll_y
                    }
                } else {
                    state.scroll_y
                };
                state.set_scroll(hwnd, next);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(state) = state {
                let key = wparam.0 as u16;
                let control = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
                if control && key == VK_A.0 {
                    state.select_all(hwnd);
                    return LRESULT(0);
                }
                if control && key == VK_C.0 {
                    state.copy_selection(hwnd);
                    return LRESULT(0);
                }
                if control && key == VK_O.0 {
                    if let Some(path) = choose_file(hwnd) {
                        state.open_path(hwnd, path);
                    }
                    return LRESULT(0);
                }
                if control && key == VK_F.0 {
                    state.show_find_dialog(hwnd);
                    return LRESULT(0);
                }
                if control && key == VK_OEM_COMMA.0 {
                    show_context_menu(hwnd, state);
                    return LRESULT(0);
                }
                if is_close_shortcut(control, key) {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    return LRESULT(0);
                }
                if key == VK_ESCAPE.0 {
                    state.clear_selection(hwnd);
                    return LRESULT(0);
                }
                if key == VK_F5.0 {
                    state.reload(hwnd, false);
                    return LRESULT(0);
                }
                if key == VK_F3.0 {
                    state.repeat_find(hwnd, shift);
                    return LRESULT(0);
                }
                let next = if key == VK_UP.0 {
                    Some(state.scroll_y - 48.0)
                } else if key == VK_DOWN.0 {
                    Some(state.scroll_y + 48.0)
                } else if key == VK_PRIOR.0 {
                    Some(state.scroll_y - state.client_height_dip() * 0.85)
                } else if key == VK_NEXT.0 {
                    Some(state.scroll_y + state.client_height_dip() * 0.85)
                } else if let Some(backward) = space_page_direction(control, shift, key) {
                    let distance = state.client_height_dip() * 0.85;
                    Some(if backward {
                        state.scroll_y - distance
                    } else {
                        state.scroll_y + distance
                    })
                } else if key == VK_HOME.0 {
                    Some(0.0)
                } else if key == VK_END.0 {
                    Some(state.max_scroll())
                } else {
                    None
                };
                if let Some(next) = next {
                    state.set_scroll(hwnd, next);
                    return LRESULT(0);
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_CONTEXTMENU => {
            if let Some(state) = state {
                show_context_menu(hwnd, state);
            }
            LRESULT(0)
        }
        WM_DROPFILES => {
            if let Some(state) = state {
                let drop = HDROP(wparam.0 as *mut _);
                if let Some(path) = first_dropped_file(drop) {
                    state.open_path(hwnd, path);
                }
                unsafe {
                    DragFinish(drop);
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == RELOAD_TIMER
                && let Some(state) = state
            {
                state.reload(hwnd, true);
            } else if wparam.0 == RELAYOUT_TIMER {
                unsafe {
                    let _ = KillTimer(Some(hwnd), RELAYOUT_TIMER);
                }
                if let Some(state) = state {
                    state.finish_deferred_resize(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), RELOAD_TIMER);
                let _ = KillTimer(Some(hwnd), RELAYOUT_TIMER);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            if !state_ptr.is_null() {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn choose_file(hwnd: HWND) -> Option<PathBuf> {
    let filter = wide_null(
        "Markdown (*.md;*.markdown)\0*.md;*.markdown\0Text (*.txt)\0*.txt\0All files (*.*)\0*.*\0",
    );
    let mut path_buffer = vec![0u16; 32_768];
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(path_buffer.as_mut_ptr()),
        nMaxFile: path_buffer.len() as u32,
        Flags: OFN_EXPLORER
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_HIDEREADONLY
            | OFN_NOCHANGEDIR
            | OFN_DONTADDTORECENT,
        ..Default::default()
    };
    if !unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        return None;
    }
    let length = path_buffer.iter().position(|value| *value == 0)?;
    Some(PathBuf::from(OsString::from_wide(&path_buffer[..length])))
}

fn show_context_menu(hwnd: HWND, state: &mut AppState) {
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };
    let Ok(font_menu) = (unsafe { CreatePopupMenu() }) else {
        unsafe {
            let _ = DestroyMenu(menu);
        }
        return;
    };
    let Ok(size_menu) = (unsafe { CreatePopupMenu() }) else {
        unsafe {
            let _ = DestroyMenu(font_menu);
            let _ = DestroyMenu(menu);
        }
        return;
    };
    let Ok(spacing_menu) = (unsafe { CreatePopupMenu() }) else {
        unsafe {
            let _ = DestroyMenu(size_menu);
            let _ = DestroyMenu(font_menu);
            let _ = DestroyMenu(menu);
        }
        return;
    };
    let Ok(window_menu) = (unsafe { CreatePopupMenu() }) else {
        unsafe {
            let _ = DestroyMenu(spacing_menu);
            let _ = DestroyMenu(size_menu);
            let _ = DestroyMenu(font_menu);
            let _ = DestroyMenu(menu);
        }
        return;
    };

    let has_selection = state.selection_range().is_some();
    let has_text = state
        .layout
        .as_ref()
        .is_some_and(|layout| layout.text_len() > 0);
    append_action_item(menu, CMD_FIND, "Find…\tCtrl+F", has_text);
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
    append_action_item(menu, CMD_COPY, "Copy\tCtrl+C", has_selection);
    append_action_item(menu, CMD_SELECT_ALL, "Select all\tCtrl+A", has_text);
    append_action_item(
        menu,
        CMD_CLEAR_SELECTION,
        "Clear selection\tEsc",
        has_selection,
    );
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }

    let settings = state.renderer.settings();
    append_menu_item(
        font_menu,
        CMD_FONT_SEGOE_UI,
        "Segoe UI",
        settings.font == FontChoice::SegoeUi,
    );
    append_menu_item(
        font_menu,
        CMD_FONT_ARIAL,
        "Arial",
        settings.font == FontChoice::Arial,
    );
    append_menu_item(
        font_menu,
        CMD_FONT_GEORGIA,
        "Georgia",
        settings.font == FontChoice::Georgia,
    );
    append_menu_item(
        font_menu,
        CMD_FONT_CONSOLAS,
        "Consolas",
        settings.font == FontChoice::Consolas,
    );
    unsafe {
        let _ = AppendMenuW(font_menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
    append_menu_item(
        font_menu,
        CMD_FONT_BIZ_UD_GOTHIC,
        "BIZ UDGothic",
        settings.font == FontChoice::BizUdGothic,
    );
    append_menu_item(
        font_menu,
        CMD_FONT_YU_GOTHIC_UI,
        "Yu Gothic UI",
        settings.font == FontChoice::YuGothicUi,
    );
    append_menu_item(
        font_menu,
        CMD_FONT_MEIRYO,
        "Meiryo",
        settings.font == FontChoice::Meiryo,
    );

    append_menu_item(size_menu, CMD_SIZE_11, "11 px", settings.font_size == 11);
    append_menu_item(size_menu, CMD_SIZE_13, "13 px", settings.font_size == 13);
    append_menu_item(size_menu, CMD_SIZE_15, "15 px", settings.font_size == 15);
    append_menu_item(size_menu, CMD_SIZE_17, "17 px", settings.font_size == 17);
    append_menu_item(size_menu, CMD_SIZE_19, "19 px", settings.font_size == 19);
    append_menu_item(size_menu, CMD_SIZE_21, "21 px", settings.font_size == 21);

    append_menu_item(
        spacing_menu,
        CMD_SPACING_COMPACT,
        "Compact",
        settings.line_spacing == LineSpacingChoice::Compact,
    );
    append_menu_item(
        spacing_menu,
        CMD_SPACING_STANDARD,
        "Standard",
        settings.line_spacing == LineSpacingChoice::Standard,
    );
    append_menu_item(
        spacing_menu,
        CMD_SPACING_RELAXED,
        "Relaxed",
        settings.line_spacing == LineSpacingChoice::Relaxed,
    );

    append_menu_item(window_menu, CMD_WINDOW_LEFT_THIRD, "Left third", false);
    append_menu_item(window_menu, CMD_WINDOW_RIGHT_THIRD, "Right third", false);
    append_menu_item(window_menu, CMD_WINDOW_LEFT_HALF, "Left half", false);
    append_menu_item(window_menu, CMD_WINDOW_RIGHT_HALF, "Right half", false);

    append_submenu(menu, font_menu, "Font");
    append_submenu(menu, size_menu, "Text size");
    append_submenu(menu, spacing_menu, "Line spacing");
    append_submenu(menu, window_menu, "Window layout");
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
    append_menu_item(menu, CMD_SETTINGS_RESET, "Reset", false);

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        point.x = 32;
        point.y = 32;
    }
    let command = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        )
        .0 as u32
    };
    unsafe {
        let _ = DestroyMenu(menu);
    }

    match command {
        CMD_FIND => {
            state.show_find_dialog(hwnd);
            return;
        }
        CMD_COPY => {
            state.copy_selection(hwnd);
            return;
        }
        CMD_SELECT_ALL => {
            state.select_all(hwnd);
            return;
        }
        CMD_CLEAR_SELECTION => {
            state.clear_selection(hwnd);
            return;
        }
        _ => {}
    }

    if let Some(layout) = window_layout_for_command(command) {
        if let Err(message) = arrange_window(hwnd, layout) {
            show_error(Some(hwnd), &message);
        }
        return;
    }

    let Some(next) = settings_for_command(settings, command) else {
        return;
    };
    state.apply_view_settings(hwnd, next);
}

fn append_action_item(menu: HMENU, command: u32, label: &str, enabled: bool) {
    let label = wide_null(label);
    let flags = if enabled {
        MF_STRING
    } else {
        MF_STRING | MF_GRAYED
    };
    unsafe {
        let _ = AppendMenuW(menu, flags, command as usize, PCWSTR(label.as_ptr()));
    }
}

fn append_menu_item(menu: HMENU, command: u32, label: &str, checked: bool) {
    let label = wide_null(label);
    let flags = if checked {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    };
    unsafe {
        let _ = AppendMenuW(menu, flags, command as usize, PCWSTR(label.as_ptr()));
    }
}

fn append_submenu(menu: HMENU, submenu: HMENU, label: &str) {
    let label = wide_null(label);
    unsafe {
        let _ = AppendMenuW(menu, MF_POPUP, submenu.0 as usize, PCWSTR(label.as_ptr()));
    }
}

fn settings_for_command(current: ViewSettings, command: u32) -> Option<ViewSettings> {
    let mut next = current;
    match command {
        CMD_FONT_BIZ_UD_GOTHIC => next.font = FontChoice::BizUdGothic,
        CMD_FONT_SEGOE_UI => next.font = FontChoice::SegoeUi,
        CMD_FONT_ARIAL => next.font = FontChoice::Arial,
        CMD_FONT_GEORGIA => next.font = FontChoice::Georgia,
        CMD_FONT_CONSOLAS => next.font = FontChoice::Consolas,
        CMD_FONT_YU_GOTHIC_UI => next.font = FontChoice::YuGothicUi,
        CMD_FONT_MEIRYO => next.font = FontChoice::Meiryo,
        CMD_SIZE_11 => next.font_size = 11,
        CMD_SIZE_13 => next.font_size = 13,
        CMD_SIZE_15 => next.font_size = 15,
        CMD_SIZE_17 => next.font_size = 17,
        CMD_SIZE_19 => next.font_size = 19,
        CMD_SIZE_21 => next.font_size = 21,
        CMD_SPACING_COMPACT => next.line_spacing = LineSpacingChoice::Compact,
        CMD_SPACING_STANDARD => next.line_spacing = LineSpacingChoice::Standard,
        CMD_SPACING_RELAXED => next.line_spacing = LineSpacingChoice::Relaxed,
        CMD_SETTINGS_RESET => next = ViewSettings::default(),
        _ => return None,
    }
    Some(next)
}

fn window_layout_for_command(command: u32) -> Option<WindowLayout> {
    match command {
        CMD_WINDOW_LEFT_THIRD => Some(WindowLayout::LeftThird),
        CMD_WINDOW_RIGHT_THIRD => Some(WindowLayout::RightThird),
        CMD_WINDOW_LEFT_HALF => Some(WindowLayout::LeftHalf),
        CMD_WINDOW_RIGHT_HALF => Some(WindowLayout::RightHalf),
        _ => None,
    }
}

fn arrange_window(hwnd: HWND, layout: WindowLayout) -> Result<(), String> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut monitor_info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool() {
        return Err(format!(
            "Could not determine the monitor work area.\n\n{}",
            windows::core::Error::from_win32()
        ));
    }
    let (x, y, width, height) = window_placement(monitor_info.rcWork, layout);
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .map_err(|error| format!("Could not arrange the window.\n\n{error}"))?;
    }
    Ok(())
}

fn window_placement(work_area: RECT, layout: WindowLayout) -> (i32, i32, i32, i32) {
    let work_width = (work_area.right - work_area.left).max(1);
    let work_height = (work_area.bottom - work_area.top).max(1);
    let width = match layout {
        WindowLayout::LeftThird | WindowLayout::RightThird => (work_width / 3).max(1),
        WindowLayout::LeftHalf | WindowLayout::RightHalf => (work_width / 2).max(1),
    };
    let x = match layout {
        WindowLayout::LeftThird | WindowLayout::LeftHalf => work_area.left,
        WindowLayout::RightThird | WindowLayout::RightHalf => work_area.right - width,
    };
    (x, work_area.top, width, work_height)
}

fn mouse_coordinates(lparam: LPARAM) -> (f32, f32) {
    let packed = lparam.0 as usize;
    let x = (packed & 0xffff) as u16 as i16 as f32;
    let y = ((packed >> 16) & 0xffff) as u16 as i16 as f32;
    (x, y)
}

fn is_close_shortcut(control: bool, key: u16) -> bool {
    control && (key == VK_W.0 || key == VK_Q.0)
}

fn space_page_direction(control: bool, shift: bool, key: u16) -> Option<bool> {
    (!control && key == VK_SPACE.0).then_some(shift)
}

fn stepped_font_size(current: u8, steps: i32) -> u8 {
    let current_index = FONT_SIZES
        .iter()
        .position(|size| *size == current)
        .unwrap_or_else(|| {
            FONT_SIZES
                .iter()
                .enumerate()
                .min_by_key(|(_, size)| size.abs_diff(current))
                .map(|(index, _)| index)
                .unwrap_or(0)
        });
    let next_index = (current_index as i32 + steps).clamp(0, FONT_SIZES.len() as i32 - 1);
    FONT_SIZES[next_index as usize]
}

fn normalized_dpi(dpi: u32) -> u32 {
    if dpi == 0 { DEFAULT_DPI } else { dpi }
}

fn pixels_to_dips(pixels: f32, dpi: u32) -> f32 {
    pixels * DEFAULT_DPI as f32 / normalized_dpi(dpi) as f32
}

fn copy_utf16_to_clipboard(hwnd: HWND, text: &[u16]) -> Result<(), String> {
    let mut payload = Vec::with_capacity(text.len() + 1);
    payload.extend_from_slice(text);
    payload.push(0);

    unsafe {
        OpenClipboard(Some(hwnd))
            .map_err(|error| format!("Could not open the clipboard.\n\n{error}"))?;
        let result = (|| -> Result<(), String> {
            EmptyClipboard()
                .map_err(|error| format!("Could not clear the clipboard.\n\n{error}"))?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, payload.len() * size_of::<u16>())
                .map_err(|error| format!("Could not allocate memory for copying.\n\n{error}"))?;
            let destination = GlobalLock(memory).cast::<u16>();
            if destination.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(format!(
                    "Could not access the copy buffer.\n\n{}",
                    windows::core::Error::from_win32()
                ));
            }
            std::ptr::copy_nonoverlapping(payload.as_ptr(), destination, payload.len());
            let _ = GlobalUnlock(memory);
            if let Err(error) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(format!("Could not write to the clipboard.\n\n{error}"));
            }
            Ok(())
        })();
        let close_result =
            CloseClipboard().map_err(|error| format!("Could not close the clipboard.\n\n{error}"));
        result.and(close_result)
    }
}

fn first_dropped_file(drop: HDROP) -> Option<PathBuf> {
    let length = unsafe { DragQueryFileW(drop, 0, None) };
    if length == 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize + 1];
    unsafe {
        DragQueryFileW(drop, 0, Some(&mut buffer));
    }
    Some(PathBuf::from(OsString::from_wide(
        &buffer[..length as usize],
    )))
}

fn decode_text(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8(bytes[3..].to_vec())
            .map_err(|_| "Could not decode the file as UTF-8.".to_string());
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return decode_utf16(&bytes[2..], true);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return decode_utf16(&bytes[2..], false);
    }
    String::from_utf8(bytes).map_err(|_| {
        "Unsupported text encoding. Save the file as UTF-8 or UTF-16 with a BOM.".to_string()
    })
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("The UTF-16 file has an invalid byte length.".to_string());
    }
    let values = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&values).map_err(|_| "Could not decode the file as UTF-16.".to_string())
}

fn show_error(hwnd: Option<HWND>, message: &str) {
    let message = wide_null(message);
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR(message.as_ptr()),
            w!("QuietMD"),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn show_information(hwnd: Option<HWND>, message: &str) {
    let message = wide_null(message);
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR(message.as_ptr()),
            w!("QuietMD"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn window_title(path: Option<&Path>) -> String {
    path.and_then(Path::file_name)
        .map(|name| format!("{} - QuietMD", name.to_string_lossy()))
        .unwrap_or_else(|| "QuietMD".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempFile(PathBuf);

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn temporary_markdown_path() -> TempFile {
        let unique = format!(
            "quietmd-test-{}-{}.md",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        TempFile(std::env::temp_dir().join(unique))
    }

    #[test]
    fn decodes_utf8_and_bom() {
        assert_eq!(decode_text("日本語".as_bytes().to_vec()).unwrap(), "日本語");
        assert_eq!(decode_text(vec![0xef, 0xbb, 0xbf, b'a']).unwrap(), "a");
    }

    #[test]
    fn rejects_unknown_text_encoding_instead_of_replacing_characters() {
        let error = decode_text(vec![0x82, 0xa0]).unwrap_err();
        assert!(error.contains("Unsupported text encoding"));
    }

    #[test]
    fn reload_preserves_scroll_and_only_keeps_unchanged_selection() {
        let path = temporary_markdown_path();
        let text = (0..300)
            .map(|index| format!("Paragraph {index}: searchable marker text."))
            .collect::<Vec<_>>()
            .join("\n\n");
        std::fs::write(&path.0, &text).unwrap();

        let mut state = AppState::new(None, 1).unwrap();
        state.load_path(path.0.clone(), false).unwrap();
        let marker = "searchable marker".encode_utf16().collect::<Vec<_>>();
        let selection = state
            .layout
            .as_ref()
            .unwrap()
            .find_text(&marker, 0, true, true, false)
            .unwrap();
        state.selection_anchor = Some(selection.0);
        state.selection_active = Some(selection.1);
        state.scroll_y = 240.0;

        state.load_path(path.0.clone(), true).unwrap();
        assert_eq!(state.scroll_y, 240.0);
        assert_eq!(state.selection_range(), Some(selection));

        std::fs::write(&path.0, format!("Inserted before the document.\n\n{text}")).unwrap();
        state.load_path(path.0.clone(), true).unwrap();
        assert_eq!(state.scroll_y, 240.0);
        assert_eq!(state.selection_range(), None);
    }

    #[test]
    fn decodes_utf16_both_endians() {
        assert_eq!(decode_text(vec![0xff, 0xfe, 0x41, 0x00]).unwrap(), "A");
        assert_eq!(decode_text(vec![0xfe, 0xff, 0x00, 0x41]).unwrap(), "A");
    }

    #[test]
    fn formats_window_title_with_ascii_separator() {
        assert_eq!(
            window_title(Some(Path::new("lighting-system-prep.md"))),
            "lighting-system-prep.md - QuietMD"
        );
        assert_eq!(window_title(None), "QuietMD");
    }

    #[test]
    fn changes_one_view_setting_at_a_time() {
        let defaults = ViewSettings::default();
        let smaller = settings_for_command(defaults, CMD_SIZE_11).unwrap();
        assert_eq!(smaller.font_size, 11);
        assert_eq!(smaller.font, defaults.font);
        assert_eq!(smaller.line_spacing, defaults.line_spacing);

        let serif = settings_for_command(defaults, CMD_FONT_GEORGIA).unwrap();
        assert_eq!(serif.font, FontChoice::Georgia);
        assert_eq!(serif.font_size, defaults.font_size);
        assert_eq!(serif.line_spacing, defaults.line_spacing);

        let larger = settings_for_command(defaults, CMD_SIZE_21).unwrap();
        assert_eq!(larger.font_size, 21);
        assert_eq!(larger.font, defaults.font);
        assert_eq!(larger.line_spacing, defaults.line_spacing);

        let relaxed = settings_for_command(larger, CMD_SPACING_RELAXED).unwrap();
        assert_eq!(relaxed.line_spacing, LineSpacingChoice::Relaxed);
        assert_eq!(
            settings_for_command(relaxed, CMD_SETTINGS_RESET),
            Some(ViewSettings::default())
        );
    }

    #[test]
    fn steps_and_clamps_wheel_font_sizes() {
        assert_eq!(stepped_font_size(15, 1), 17);
        assert_eq!(stepped_font_size(15, -2), 11);
        assert_eq!(stepped_font_size(21, 3), 21);
        assert_eq!(stepped_font_size(11, -1), 11);
    }

    #[test]
    fn empty_document_message_is_short_and_actionable() {
        assert_eq!(
            EMPTY_DOCUMENT_MARKDOWN,
            "# Open Markdown\n\nDrop a file here\n\nCtrl+O to browse / right-click for menu"
        );
    }

    #[test]
    fn extracts_signed_mouse_coordinates() {
        let packed = ((20u32 << 16) | u16::MAX as u32) as isize;
        assert_eq!(mouse_coordinates(LPARAM(packed)), (-1.0, 20.0));
    }

    #[test]
    fn recognizes_close_and_quit_shortcuts_only_with_control() {
        assert!(is_close_shortcut(true, VK_W.0));
        assert!(is_close_shortcut(true, VK_Q.0));
        assert!(!is_close_shortcut(false, VK_W.0));
        assert!(!is_close_shortcut(true, VK_C.0));
    }

    #[test]
    fn recognizes_forward_and_backward_space_page_shortcuts() {
        assert_eq!(space_page_direction(false, false, VK_SPACE.0), Some(false));
        assert_eq!(space_page_direction(false, true, VK_SPACE.0), Some(true));
        assert_eq!(space_page_direction(true, false, VK_SPACE.0), None);
        assert_eq!(space_page_direction(false, false, VK_NEXT.0), None);
    }

    #[test]
    fn calculates_window_layouts_from_the_monitor_work_area() {
        let work_area = RECT {
            left: -1920,
            top: 24,
            right: 0,
            bottom: 1064,
        };
        assert_eq!(
            window_placement(work_area, WindowLayout::LeftThird),
            (-1920, 24, 640, 1040)
        );
        assert_eq!(
            window_placement(work_area, WindowLayout::RightThird),
            (-640, 24, 640, 1040)
        );
        assert_eq!(
            window_placement(work_area, WindowLayout::LeftHalf),
            (-1920, 24, 960, 1040)
        );
        assert_eq!(
            window_placement(work_area, WindowLayout::RightHalf),
            (-960, 24, 960, 1040)
        );
    }

    #[test]
    fn converts_physical_pixels_to_dips_at_150_percent() {
        assert_eq!(pixels_to_dips(960.0, 144), 640.0);
        assert_eq!(pixels_to_dips(300.0, 144), 200.0);
    }

    #[test]
    fn pixel_conversion_is_identity_at_default_or_unknown_dpi() {
        assert_eq!(pixels_to_dips(640.0, DEFAULT_DPI), 640.0);
        assert_eq!(pixels_to_dips(640.0, 0), 640.0);
    }
}
