//! The Settings window: a modal dialog from an in-memory template, run on its own thread with no
//! owner (an owner on the event-loop thread would attach the two threads' input queues). The
//! dialog manager handles Tab, Enter (Save) and Esc (Cancel), and rescales the dialog per
//! monitor in this per-monitor-v2 process.
//!
//! The dialog procedure is re-entered whenever it calls something that sends messages
//! (SetWindowText, MessageBox, the folder picker), so `State` is only borrowed briefly and never
//! across such a call.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::size_of;
use std::num::NonZeroIsize;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicIsize, Ordering};

use log::{error, info, warn};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetMonitorInfoW, GetSysColor, GetSysColorBrush, MonitorFromPoint, SetBkMode,
    SetTextColor, COLOR_BTNFACE, COLOR_GRAYTEXT, FW_SEMIBOLD, HDC, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    TRANSPARENT,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{SS_CENTERIMAGE, SS_ICON, SS_LEFT, SS_NOPREFIX};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, LoadIconWithScaleDown, BST_CHECKED, ICC_LINK_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, NMHDR,
    NMLINK, NM_CLICK, NM_RETURN,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetDialogControlDpiChangeBehavior, SetThreadDpiAwarenessContext, SystemParametersInfoForDpi,
    DCDC_DISABLE_FONT_UPDATE, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_SNAPSHOT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{
    DialogBoxIndirectParamW, EndDialog, GetCursorPos, GetDlgCtrlID, GetDlgItem, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, MessageBoxW, SendDlgItemMessageW, SendMessageW, SetDlgItemTextW,
    SetWindowLongPtrW, SetWindowPos, BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX,
    BS_DEFPUSHBUTTON, BS_GROUPBOX, BS_PUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL,
    DLGC_WANTALLKEYS, DLGC_WANTCHARS, DLGTEMPLATE, DS_FIXEDSYS, DS_MODALFRAME, DS_SETFONT, ES_AUTOHSCROLL,
    ES_READONLY, GWLP_USERDATA, HICON, ICON_BIG, ICON_SMALL, IDCANCEL, IDOK, MB_ICONWARNING, MB_OK,
    MESSAGEBOX_RESULT, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
    STM_SETICON, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNORMAL, WM_CHAR, WM_CLOSE, WM_COMMAND,
    WM_CTLCOLORSTATIC, WM_DEADCHAR, WM_DESTROY, WM_DPICHANGED, WM_GETDLGCODE, WM_INITDIALOG, WM_KEYDOWN, WM_KEYUP,
    WM_KILLFOCUS, WM_NCDESTROY, WM_NEXTDLGCTL, WM_NOTIFY, WM_SETFONT, WM_SETICON, WM_SYSCHAR, WM_SYSDEADCHAR,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WS_BORDER, WS_CAPTION, WS_CHILD, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    WS_VSCROLL,
};
use winit::raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};

use super::form::{Form, Saved};
use super::hotkey_text::{self, Mods};
use super::template::{Class, Template};
use crate::config::Config;
use crate::{strings, version};

const ID_STATIC: u32 = u32::MAX;
const ID_ICON: i32 = 100;
const ID_TITLE: i32 = 101;
const ID_BUILD: i32 = 102;
/// Hotkey fields and their Change buttons: region, full screen, window.
const ID_HOTKEY: i32 = 110;
const ID_CHANGE: i32 = 120;
const ID_FOLDER: i32 = 130;
const ID_BROWSE: i32 = 131;
const ID_FILENAME: i32 = 132;
const ID_HINT: i32 = 133;
const ID_FULLSCREEN: i32 = 140;
const ID_WINDOW: i32 = 141;
const ID_NOTIFICATIONS: i32 = 142;
const ID_AUTOSTART: i32 = 150;
const ID_LINK: i32 = 151;

/// The application icon from the exe's resources (`build.rs`, `set_icon` uses id 1), as
/// `MAKEINTRESOURCE(1)`: an integer id in a pointer.
const ICON_RESOURCE: PCWSTR = PCWSTR(std::ptr::without_provenance(1));

struct State {
    base: Config,
    form: Form,
    slot: AtomicSlot,
    dialog: Cell<HWND>,
    /// Stored form (`Super`, not `Win`); empty = disabled.
    hotkeys: RefCell<[String; 3]>,
    /// The hotkey field waiting for a key press.
    capturing: Cell<Option<usize>>,
    title_font: Cell<HFONT>,
    /// Header, small and big window icon, for the current DPI.
    icons: Cell<[HICON; 3]>,
    saved: RefCell<Option<Saved>>,
}

type AtomicSlot = std::sync::Arc<AtomicIsize>;

impl State {
    /// After the dialog is gone: the font and icons are no longer in use.
    fn free(&self) {
        unsafe {
            let font = self.title_font.take();
            if !font.is_invalid() {
                let _ = DeleteObject(font.into());
            }
            for icon in self.icons.take() {
                destroy_icon(icon);
            }
        }
    }
}

/// Runs the dialog on the calling thread until it closes; the saved settings, or `None` for
/// Cancel. `slot` holds the dialog's HWND while it is open.
pub fn run(config: Config, autostart: bool, slot: AtomicSlot) -> Option<Saved> {
    unsafe {
        // Inherited from the process anyway; dialogs scale per monitor only in this mode.
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // The folder picker is a COM dialog.
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LINK_CLASS | ICC_STANDARD_CLASSES,
        };
        if !InitCommonControlsEx(&controls).as_bool() {
            warn!("InitCommonControlsEx failed; the GitHub link may be missing");
        }

        let form = Form::from_config(&config, autostart);
        let state = State {
            hotkeys: RefCell::new(form.hotkeys.clone()),
            base: config,
            form,
            slot,
            dialog: Cell::new(HWND::default()),
            capturing: Cell::new(None),
            title_font: Cell::new(HFONT::default()),
            icons: Cell::new([HICON::default(); 3]),
            saved: RefCell::new(None),
        };
        let template = template();
        let result = DialogBoxIndirectParamW(
            instance(),
            template.as_ptr() as *const DLGTEMPLATE,
            None,
            Some(dialog_proc),
            LPARAM(&state as *const State as isize),
        );
        state.free();
        if com.is_ok() {
            CoUninitialize();
        }
        if result == -1 {
            let e = windows::core::Error::from_thread();
            error!("cannot open the Settings window: {e}");
            crate::dialog::error(&format!("Could not open the Settings window: {e}"));
        }
        state.saved.take()
    }
}

fn instance() -> Option<HINSTANCE> {
    unsafe { GetModuleHandleW(None).ok().map(|module| HINSTANCE(module.0)) }
}

/// The layout from TASK-v3 A3.2, in dialog units. Tab order is the item order.
fn template() -> Vec<u32> {
    let style = WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | (DS_MODALFRAME | DS_SETFONT | DS_FIXEDSYS) as u32;
    // DS_CENTER would centre on the primary monitor; WM_INITDIALOG places the dialog instead.
    let mut t = Template::new(strings::SETTINGS_TITLE, style, 0, (0, 0, 300, 270), "Segoe UI", 9);
    let child = WS_CHILD.0 | WS_VISIBLE.0;
    let tab = child | WS_TABSTOP.0;
    let label = child | SS_LEFT.0 | SS_NOPREFIX.0;
    let group = child | BS_GROUPBOX as u32;
    let button = tab | BS_PUSHBUTTON as u32;

    t.item(Class::Static, "", ID_ICON as u32, child | SS_ICON.0 | SS_CENTERIMAGE.0, 0, (7, 6, 22, 22));
    t.item(Class::Static, &version::title(), ID_TITLE as u32, label, 0, (34, 5, 250, 14));
    t.item(Class::Static, &version::build_line(), ID_BUILD as u32, label, 0, (34, 19, 250, 9));

    t.item(Class::Button, strings::SETTINGS_HOTKEYS, ID_STATIC, group, 0, (7, 33, 286, 63));
    for (i, name) in strings::SETTINGS_HOTKEY_LABELS.iter().enumerate() {
        let y = 46 + 16 * i as i16;
        let i = i as i32;
        t.item(Class::Static, name, ID_STATIC, label, 0, (14, y + 2, 80, 9));
        // Read-only and no tab stop: "Change" is the way in.
        let edit = child | WS_BORDER.0 | (ES_READONLY | ES_AUTOHSCROLL) as u32;
        t.item(Class::Edit, "", (ID_HOTKEY + i) as u32, edit, 0, (96, y, 132, 13));
        t.item(Class::Button, strings::SETTINGS_CHANGE, (ID_CHANGE + i) as u32, button, 0, (232, y - 1, 54, 15));
    }

    t.item(Class::Button, strings::SETTINGS_SAVING, ID_STATIC, group, 0, (7, 100, 286, 62));
    let edit = tab | WS_BORDER.0 | ES_AUTOHSCROLL as u32;
    t.item(Class::Static, strings::SETTINGS_FOLDER, ID_STATIC, label, 0, (14, 115, 80, 9));
    t.item(Class::Edit, "", ID_FOLDER as u32, edit, 0, (96, 113, 132, 13));
    t.item(Class::Button, strings::SETTINGS_BROWSE, ID_BROWSE as u32, button, 0, (232, 112, 54, 15));
    t.item(Class::Static, strings::SETTINGS_FILE_NAME, ID_STATIC, label, 0, (14, 131, 80, 9));
    t.item(Class::Edit, "", ID_FILENAME as u32, edit, 0, (96, 129, 190, 13));
    t.item(Class::Static, strings::SETTINGS_STRFTIME_HINT, ID_HINT as u32, label, 0, (96, 145, 190, 9));

    t.item(Class::Button, strings::SETTINGS_CAPTURE, ID_STATIC, group, 0, (7, 166, 286, 62));
    let combo = tab | WS_VSCROLL.0 | CBS_DROPDOWNLIST as u32;
    let rows = [
        (strings::SETTINGS_FULLSCREEN, ID_FULLSCREEN),
        (strings::SETTINGS_WINDOW, ID_WINDOW),
        (strings::SETTINGS_NOTIFICATIONS, ID_NOTIFICATIONS),
    ];
    for (i, (name, id)) in rows.into_iter().enumerate() {
        let y = 179 + 16 * i as i16;
        t.item(Class::Static, name, ID_STATIC, label, 0, (14, y + 2, 80, 9));
        // The height includes the drop-down list.
        t.item(Class::ComboBox, "", id as u32, combo, 0, (96, y, 190, 80));
    }

    t.item(Class::Button, strings::SETTINGS_AUTOSTART, ID_AUTOSTART as u32, tab | BS_AUTOCHECKBOX as u32, 0, (9, 234, 140, 10));
    t.item(Class::Named("SysLink"), strings::SETTINGS_LINK, ID_LINK as u32, tab, 0, (9, 253, 150, 9));
    t.item(Class::Button, strings::SETTINGS_SAVE, IDOK.0 as u32, tab | BS_DEFPUSHBUTTON as u32, 0, (180, 249, 54, 15));
    t.item(Class::Button, strings::SETTINGS_CANCEL, IDCANCEL.0 as u32, button, 0, (239, 249, 54, 15));
    t.into_dwords()
}

unsafe extern "system" fn dialog_proc(hdlg: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    // A panic must not unwind into user32; the panic hook has already logged it.
    catch_unwind(AssertUnwindSafe(|| unsafe { handle(hdlg, msg, wparam, lparam) })).unwrap_or(0)
}

unsafe fn handle(hdlg: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    unsafe {
        if msg == WM_INITDIALOG {
            SetWindowLongPtrW(hdlg, GWLP_USERDATA, lparam.0);
            init(hdlg, &*(lparam.0 as *const State));
            // Focus was set in `init`.
            return 0;
        }
        let state = GetWindowLongPtrW(hdlg, GWLP_USERDATA) as *const State;
        let Some(state) = state.as_ref() else { return 0 };
        match msg {
            WM_COMMAND => on_command(hdlg, state, (wparam.0 & 0xFFFF) as i32, ((wparam.0 >> 16) & 0xFFFF) as u32),
            WM_NOTIFY => on_notify(hdlg, lparam),
            WM_CTLCOLORSTATIC => on_ctlcolor(wparam, lparam),
            WM_DPICHANGED => {
                apply_dpi(hdlg, state);
                // FALSE: the dialog manager still rescales and moves everything else.
                0
            }
            WM_CLOSE => {
                end(hdlg, IDCANCEL);
                1
            }
            WM_DESTROY => {
                state.slot.store(0, Ordering::SeqCst);
                0
            }
            _ => 0,
        }
    }
}

unsafe fn init(hdlg: HWND, state: &State) {
    unsafe {
        state.dialog.set(hdlg);
        state.slot.store(hdlg.0 as isize, Ordering::SeqCst);
        let form = &state.form;
        set_text(hdlg, ID_FOLDER, &form.save_dir);
        set_text(hdlg, ID_FILENAME, &form.filename_format);
        for i in 0..3 {
            show_hotkey(hdlg, state, i);
            let edit = item(hdlg, ID_HOTKEY + i as i32);
            let _ = SetWindowSubclass(edit, Some(hotkey_proc), i, state as *const State as usize);
        }
        fill_combo(hdlg, ID_FULLSCREEN, &strings::SETTINGS_FULLSCREEN_CHOICES, form.fullscreen_mode);
        fill_combo(hdlg, ID_WINDOW, &strings::SETTINGS_WINDOW_CHOICES, form.window_capture);
        fill_combo(hdlg, ID_NOTIFICATIONS, &strings::SETTINGS_NOTIFICATIONS_CHOICES, form.notifications);
        let check = if form.autostart { BST_CHECKED.0 as usize } else { 0 };
        SendDlgItemMessageW(hdlg, ID_AUTOSTART, BM_SETCHECK, WPARAM(check), LPARAM(0));

        // The dialog manager would reset the title font to the dialog font on a DPI change.
        let title = item(hdlg, ID_TITLE);
        if let Err(e) = SetDialogControlDpiChangeBehavior(title, DCDC_DISABLE_FONT_UPDATE, DCDC_DISABLE_FONT_UPDATE) {
            warn!("SetDialogControlDpiChangeBehavior failed: {e}");
        }
        apply_dpi(hdlg, state);
        place_on_cursor_monitor(hdlg);
        focus(hdlg, item(hdlg, ID_FOLDER));
        info!("settings window open at {} dpi", GetDpiForWindow(hdlg));
    }
}

/// Centres the dialog on the work area of the monitor under the cursor. It moves there first,
/// so a monitor with another DPI rescales it (WM_DPICHANGED) before its size is measured.
unsafe fn place_on_cursor_monitor(hdlg: HWND) {
    unsafe {
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let work = info.rcWork;
        let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
        let _ = SetWindowPos(hdlg, None, work.left, work.top, 0, 0, flags);
        let mut rect = RECT::default();
        let _ = GetWindowRect(hdlg, &mut rect);
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        let x = work.left + ((work.right - work.left - w) / 2).max(0);
        let y = work.top + ((work.bottom - work.top - h) / 2).max(0);
        let _ = SetWindowPos(hdlg, None, x, y, 0, 0, flags);
    }
}

/// Title font and icons for the dialog's current DPI.
unsafe fn apply_dpi(hdlg: HWND, state: &State) {
    unsafe {
        let dpi = GetDpiForWindow(hdlg).max(96);
        if let Some(font) = title_font(dpi) {
            SendDlgItemMessageW(hdlg, ID_TITLE, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            let old = state.title_font.replace(font);
            if !old.is_invalid() {
                let _ = DeleteObject(old.into());
            }
        }
        let size = |px: u32| (px * dpi / 96) as i32;
        let [header, small, big] = [size(32), size(16), size(32)].map(load_icon);
        SendDlgItemMessageW(hdlg, ID_ICON, STM_SETICON, WPARAM(header.0 as usize), LPARAM(0));
        SendMessageW(hdlg, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(small.0 as isize)));
        SendMessageW(hdlg, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(big.0 as isize)));
        for icon in state.icons.replace([header, small, big]) {
            destroy_icon(icon);
        }
    }
}

/// The message font at `dpi`, 1.6 times as large and semibold.
unsafe fn title_font(dpi: u32) -> Option<HFONT> {
    unsafe {
        let mut metrics = NONCLIENTMETRICSW { cbSize: size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
        let pointer = Some(&mut metrics as *mut NONCLIENTMETRICSW as *mut c_void);
        if let Err(e) = SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS.0, metrics.cbSize, pointer, 0, dpi) {
            warn!("cannot read the message font: {e}");
            return None;
        }
        let mut font = metrics.lfMessageFont;
        font.lfHeight = (font.lfHeight as f32 * 1.6).round() as i32;
        font.lfWeight = FW_SEMIBOLD.0 as i32;
        let font = CreateFontIndirectW(&font);
        (!font.is_invalid()).then_some(font)
    }
}

fn load_icon(size: i32) -> HICON {
    unsafe { LoadIconWithScaleDown(instance(), ICON_RESOURCE, size, size).unwrap_or_default() }
}

unsafe fn destroy_icon(icon: HICON) {
    if !icon.is_invalid() {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyIcon(icon) };
    }
}

unsafe fn on_command(hdlg: HWND, state: &State, id: i32, code: u32) -> isize {
    unsafe {
        match id {
            _ if id == IDOK.0 => save(hdlg, state),
            _ if id == IDCANCEL.0 => end(hdlg, IDCANCEL),
            ID_BROWSE if code == BN_CLICKED => browse(hdlg),
            _ if code == BN_CLICKED && (ID_CHANGE..ID_CHANGE + 3).contains(&id) => {
                start_capture(hdlg, state, (id - ID_CHANGE) as usize)
            }
            _ => return 0,
        }
        1
    }
}

unsafe fn save(hdlg: HWND, state: &State) {
    unsafe {
        let form = read_form(hdlg, state);
        match form.validate(&state.base) {
            Ok(saved) => {
                *state.saved.borrow_mut() = Some(saved);
                end(hdlg, IDOK);
            }
            Err(reason) => {
                info!("settings not saved: {reason}");
                MessageBoxW(Some(hdlg), &HSTRING::from(reason), &HSTRING::from(strings::APP_NAME), MB_OK | MB_ICONWARNING);
            }
        }
    }
}

unsafe fn end(hdlg: HWND, result: MESSAGEBOX_RESULT) {
    let _ = unsafe { EndDialog(hdlg, result.0 as isize) };
}

unsafe fn read_form(hdlg: HWND, state: &State) -> Form {
    unsafe {
        let selected = |id: i32| SendDlgItemMessageW(hdlg, id, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0.max(0) as usize;
        let checked = SendDlgItemMessageW(hdlg, ID_AUTOSTART, BM_GETCHECK, WPARAM(0), LPARAM(0)).0;
        Form {
            save_dir: get_text(hdlg, ID_FOLDER),
            filename_format: get_text(hdlg, ID_FILENAME),
            hotkeys: state.hotkeys.borrow().clone(),
            fullscreen_mode: selected(ID_FULLSCREEN),
            window_capture: selected(ID_WINDOW),
            notifications: selected(ID_NOTIFICATIONS),
            autostart: checked == BST_CHECKED.0 as isize,
        }
    }
}

/// The folder picker, modal to the dialog.
unsafe fn browse(hdlg: HWND) {
    unsafe {
        let current = get_text(hdlg, ID_FOLDER);
        let current = current.trim();
        let mut picker = rfd::FileDialog::new().set_title(strings::SETTINGS_PICK_FOLDER).set_parent(&Parent(hdlg));
        if Path::new(current).is_dir() {
            picker = picker.set_directory(current);
        }
        if let Some(dir) = picker.pick_folder() {
            info!("settings: folder picked: {}", dir.display());
            set_text(hdlg, ID_FOLDER, &dir.display().to_string());
        }
    }
}

/// The dialog as the folder picker's owner.
struct Parent(HWND);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.0 .0 as isize).ok_or(HandleError::Unavailable)?;
        // The dialog outlives the picker, which runs while this handle is borrowed.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(hwnd))) })
    }
}

impl HasDisplayHandle for Parent {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}

/// Opens the GitHub link.
unsafe fn on_notify(hdlg: HWND, lparam: LPARAM) -> isize {
    unsafe {
        let header = &*(lparam.0 as *const NMHDR);
        if header.idFrom != ID_LINK as usize || !(header.code == NM_CLICK || header.code == NM_RETURN) {
            return 0;
        }
        let link = &*(lparam.0 as *const NMLINK);
        let url = PCWSTR(link.item.szUrl.as_ptr());
        let result = ShellExecuteW(Some(hdlg), w!("open"), url, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        // ShellExecute reports success as a value above 32.
        if result.0 as isize > 32 {
            info!("opened {}", url.display());
        } else {
            warn!("cannot open {}: ShellExecute returned {}", url.display(), result.0 as isize);
        }
        1
    }
}

/// Grey text for the build line and the strftime hint. The read-only hotkey edits send this
/// message too and keep the default colours.
unsafe fn on_ctlcolor(wparam: WPARAM, lparam: LPARAM) -> isize {
    unsafe {
        let id = GetDlgCtrlID(HWND(lparam.0 as *mut c_void));
        if id != ID_BUILD && id != ID_HINT {
            return 0;
        }
        let hdc = HDC(wparam.0 as *mut c_void);
        SetTextColor(hdc, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
        SetBkMode(hdc, TRANSPARENT);
        GetSysColorBrush(COLOR_BTNFACE).0 as isize
    }
}

/// "Change": the field shows "Press keys…" and records the next key press.
unsafe fn start_capture(hdlg: HWND, state: &State, index: usize) {
    unsafe {
        end_capture(hdlg, state);
        state.capturing.set(Some(index));
        let id = ID_HOTKEY + index as i32;
        set_text(hdlg, id, strings::SETTINGS_PRESS_KEYS);
        focus(hdlg, item(hdlg, id));
    }
}

/// Leaves capture mode, showing the field's hotkey (unchanged unless a key was accepted).
unsafe fn end_capture(hdlg: HWND, state: &State) {
    if let Some(index) = state.capturing.take() {
        unsafe { show_hotkey(hdlg, state, index) };
    }
}

unsafe fn show_hotkey(hdlg: HWND, state: &State, index: usize) {
    let text = hotkey_text::display(&state.hotkeys.borrow()[index]);
    unsafe { set_text(hdlg, ID_HOTKEY + index as i32, &text) };
}

/// The hotkey fields while they record: every key goes to the field, no character is typed,
/// and Alt does not open the window menu.
unsafe extern "system" fn hotkey_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    index: usize,
    data: usize,
) -> LRESULT {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        let state = &*(data as *const State);
        let capturing = state.capturing.get() == Some(index);
        let vk = VIRTUAL_KEY(wparam.0 as u16);
        match msg {
            WM_GETDLGCODE if capturing => return LRESULT((DLGC_WANTALLKEYS | DLGC_WANTCHARS) as isize),
            WM_KEYDOWN | WM_SYSKEYDOWN if capturing => {
                on_key(state, index, vk);
                return LRESULT(0);
            }
            // Print Screen only arrives as a key-up.
            WM_KEYUP | WM_SYSKEYUP if capturing => {
                if vk == VK_SNAPSHOT {
                    on_key(state, index, vk);
                }
                return LRESULT(0);
            }
            WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR if capturing => return LRESULT(0),
            WM_KILLFOCUS if capturing => end_capture(state.dialog.get(), state),
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(hotkey_proc), index);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }))
    .unwrap_or(LRESULT(0))
}

unsafe fn on_key(state: &State, index: usize, vk: VIRTUAL_KEY) {
    if hotkey_text::is_modifier(vk) {
        return;
    }
    let down = |key: VIRTUAL_KEY| unsafe { GetKeyState(key.0 as i32) } < 0;
    let mods = Mods { ctrl: down(VK_CONTROL), shift: down(VK_SHIFT), alt: down(VK_MENU), win: down(VK_LWIN) || down(VK_RWIN) };
    let hdlg = state.dialog.get();
    let plain = mods == Mods::default();
    let accepted = if plain && vk == VK_ESCAPE {
        None
    } else if plain && vk == VK_BACK {
        Some(String::new())
    } else {
        match hotkey_text::format(mods, vk) {
            Some(text) => Some(text),
            None => {
                let _ = unsafe { MessageBeep(MB_OK) };
                return;
            }
        }
    };
    if let Some(text) = accepted {
        let label = strings::SETTINGS_HOTKEY_LABELS[index];
        info!("settings: {label} hotkey set to {text:?}");
        state.hotkeys.borrow_mut()[index] = text;
    }
    unsafe { end_capture(hdlg, state) };
}

fn item(hdlg: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(hdlg), id).unwrap_or_default() }
}

unsafe fn focus(hdlg: HWND, control: HWND) {
    // WM_NEXTDLGCTL rather than SetFocus, so the dialog manager updates the default button.
    unsafe { SendMessageW(hdlg, WM_NEXTDLGCTL, Some(WPARAM(control.0 as usize)), Some(LPARAM(1))) };
}

unsafe fn get_text(hdlg: HWND, id: i32) -> String {
    unsafe {
        let control = item(hdlg, id);
        let mut buffer = vec![0u16; GetWindowTextLengthW(control).max(0) as usize + 1];
        let len = GetWindowTextW(control, &mut buffer).max(0) as usize;
        String::from_utf16_lossy(&buffer[..len])
    }
}

unsafe fn set_text(hdlg: HWND, id: i32, text: &str) {
    let _ = unsafe { SetDlgItemTextW(hdlg, id, &HSTRING::from(text)) };
}

unsafe fn fill_combo(hdlg: HWND, id: i32, choices: &[&str], selected: usize) {
    unsafe {
        for choice in choices {
            let text = HSTRING::from(*choice);
            SendDlgItemMessageW(hdlg, id, CB_ADDSTRING, WPARAM(0), LPARAM(text.as_ptr() as isize));
        }
        SendDlgItemMessageW(hdlg, id, CB_SETCURSEL, WPARAM(selected), LPARAM(0));
    }
}
