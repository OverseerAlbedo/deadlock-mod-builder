//! Keeps the window library's hidden helper window alive when something asks
//! it to close.
//!
//! The window library creates one hidden top-level window per app (class
//! `Tao Thread Event Target`). Work that must run on the main thread, a native
//! message box among it, is handed over by posting to that window, and its
//! window procedure lets a close request destroy it. While the app's own window
//! is minimized it sits at the bottom of the window order, so tools that close
//! "the main window of a process" pick the helper instead. Once the helper is
//! gone the app keeps running but can never show a message box again.
//!
//! The guard subclasses the helper (comctl32 `SetWindowSubclass`, keyed by
//! this module's procedure and id; the library keeps its own data in the
//! window's `GWLP_USERDATA` slot, which subclassing does not touch). A close
//! request sent to the helper is forwarded to the app's own window as a
//! `WM_CLOSE`, so it behaves like a close of the app; everything else passes
//! through unchanged. Handles are passed as `isize` so the functions have the
//! same signature on every platform; elsewhere than Windows they do nothing.

/// Class name of the window library's hidden helper window.
pub const HELPER_CLASS: &str = "Tao Thread Event Target";

/// Installs the guard for the app's main window: finds the helper among the
/// top-level windows of the main window's thread and subclasses it. Must be
/// called on the thread that owns both windows.
pub fn install_for_main(main: isize) -> Result<(), String> {
    if !cfg!(windows) {
        return Ok(());
    }
    let helper = find_helper(main)
        .ok_or_else(|| format!("the window library's helper window ({HELPER_CLASS}) was not found"))?;
    install(helper, main)
}

/// The helper window on the thread that owns `main`, if there is one.
pub fn find_helper(main: isize) -> Option<isize> {
    imp::find_helper(main)
}

/// Makes a close request sent to `helper` be forwarded to `main` instead of
/// destroying `helper`. Must be called on the thread that owns `helper`.
pub fn install(helper: isize, main: isize) -> Result<(), String> {
    imp::install(helper, main)
}

#[cfg(windows)]
mod imp {
    use super::HELPER_CLASS;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetClassNameW, GetWindowThreadProcessId, IsWindow, PostMessageW, SC_CLOSE,
        WM_CLOSE, WM_NCDESTROY, WM_SYSCOMMAND,
    };

    /// This guard's subclass id on the helper window.
    const SUBCLASS_ID: usize = 0x444D_4247;

    struct Search {
        class: Vec<u16>,
        found: HWND,
    }

    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> windows_sys::core::BOOL {
        let search = &mut *(data as *mut Search);
        let mut name = [0u16; 64];
        let length = GetClassNameW(window, name.as_mut_ptr(), name.len() as i32);
        if length > 0 && name[..length as usize] == search.class[..] {
            search.found = window;
            return 0;
        }
        1
    }

    pub fn find_helper(main: isize) -> Option<isize> {
        let main = main as HWND;
        // SAFETY: plain Win32 queries; `search` outlives the enumeration, which
        // runs synchronously on this thread.
        unsafe {
            if IsWindow(main) == 0 {
                return None;
            }
            let thread = GetWindowThreadProcessId(main, std::ptr::null_mut());
            if thread == 0 {
                return None;
            }
            let mut search = Search { class: HELPER_CLASS.encode_utf16().collect(), found: std::ptr::null_mut() };
            EnumThreadWindows(thread, Some(visit), &mut search as *mut Search as LPARAM);
            (!search.found.is_null()).then_some(search.found as isize)
        }
    }

    pub fn install(helper: isize, main: isize) -> Result<(), String> {
        let (helper, main) = (helper as HWND, main as HWND);
        // SAFETY: the handles are only passed to Win32, which checks them.
        unsafe {
            if IsWindow(helper) == 0 || IsWindow(main) == 0 {
                return Err("a window handle is not a window".into());
            }
            if SetWindowSubclass(helper, Some(guard), SUBCLASS_ID, main as usize) == 0 {
                return Err(format!(
                    "the helper window could not be guarded: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(())
    }

    /// The subclass procedure. Calls only Win32 functions, so it cannot panic.
    unsafe extern "system" fn guard(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        main: usize,
    ) -> LRESULT {
        let close = message == WM_CLOSE || (message == WM_SYSCOMMAND && (wparam & 0xFFF0) as u32 == SC_CLOSE);
        if close {
            PostMessageW(main as HWND, WM_CLOSE, 0, 0);
            return 0;
        }
        if message == WM_NCDESTROY {
            RemoveWindowSubclass(window, Some(guard), SUBCLASS_ID);
        }
        DefSubclassProc(window, message, wparam, lparam)
    }

    #[cfg(test)]
    mod tests {
        use std::cell::Cell;
        use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, IsWindow, PeekMessageW,
            PostMessageW, RegisterClassW, TranslateMessage, MSG, PM_REMOVE, SC_CLOSE, WM_CLOSE, WM_SYSCOMMAND,
            WNDCLASSW, WS_OVERLAPPED,
        };

        thread_local! {
            static CLOSES: Cell<u32> = const { Cell::new(0) };
        }

        /// Stand-in for the helper: everything goes to the default handling,
        /// which destroys the window on a close request.
        unsafe extern "system" fn plain(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
            DefWindowProcW(window, message, wparam, lparam)
        }

        /// Stand-in for the app's window: records close requests and stays.
        unsafe extern "system" fn recording(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
            if message == WM_CLOSE {
                CLOSES.with(|closes| closes.set(closes.get() + 1));
                return 0;
            }
            DefWindowProcW(window, message, wparam, lparam)
        }

        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().chain(Some(0)).collect()
        }

        /// A hidden top-level window of its own class on the calling thread.
        fn window(class: &str, procedure: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT) -> HWND {
            let class = wide(class);
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let mut description: WNDCLASSW = std::mem::zeroed();
                description.lpfnWndProc = Some(procedure);
                description.hInstance = instance;
                description.lpszClassName = class.as_ptr();
                // A second registration of the same class fails harmlessly.
                RegisterClassW(&description);
                let window = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                assert!(!window.is_null(), "window not created: {}", std::io::Error::last_os_error());
                window
            }
        }

        fn pump() {
            unsafe {
                for _ in 0..20 {
                    let mut message: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }

        /// Sends `message` to a helper stand-in (guarded or not) and returns
        /// whether the helper still exists and how many closes the app's
        /// stand-in received.
        fn close_helper(guarded: bool, message: u32, wparam: WPARAM) -> (bool, u32) {
            CLOSES.with(|closes| closes.set(0));
            let helper = window("DmbGuardTestHelper", plain);
            let main = window("DmbGuardTestMain", recording);
            if guarded {
                super::install(helper as isize, main as isize).expect("guard installed");
            }
            unsafe { PostMessageW(helper, message, wparam, 0) };
            pump();
            let alive = unsafe { IsWindow(helper) != 0 };
            let closes = CLOSES.with(Cell::get);
            unsafe {
                if alive {
                    DestroyWindow(helper);
                }
                DestroyWindow(main);
            }
            (alive, closes)
        }

        #[test]
        fn a_close_sent_to_the_guarded_helper_reaches_the_app_window_instead() {
            assert_eq!(close_helper(true, WM_CLOSE, 0), (true, 1));
        }

        #[test]
        fn a_system_close_sent_to_the_guarded_helper_reaches_the_app_window_instead() {
            assert_eq!(close_helper(true, WM_SYSCOMMAND, SC_CLOSE as WPARAM), (true, 1));
        }

        #[test]
        fn without_the_guard_a_close_destroys_the_helper_and_the_app_window_hears_nothing() {
            assert_eq!(close_helper(false, WM_CLOSE, 0), (false, 0));
        }

        #[test]
        fn the_guard_finds_the_helper_by_its_class_on_the_app_window_s_thread() {
            let main = window("DmbGuardTestFindMain", recording);
            assert_eq!(super::find_helper(main as isize), None);
            let helper = window(super::HELPER_CLASS, plain);
            assert_eq!(super::find_helper(main as isize), Some(helper as isize));
            unsafe {
                DestroyWindow(helper);
                DestroyWindow(main);
            }
        }

        #[test]
        fn a_guarded_helper_can_still_be_destroyed_directly() {
            let helper = window("DmbGuardTestDestroy", plain);
            let main = window("DmbGuardTestDestroyMain", recording);
            super::install(helper as isize, main as isize).expect("guard installed");
            unsafe {
                assert_ne!(DestroyWindow(helper), 0);
                assert_eq!(IsWindow(helper), 0);
                DestroyWindow(main);
            }
        }
    }
}

#[cfg(test)]
mod crate_rules {
    use std::path::{Path, PathBuf};

    fn sources(folder: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(folder).expect("source folder readable") {
            let path = entry.expect("source entry readable").path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }

    /// The crate root says `deny`, not `forbid`, so that this one module can
    /// talk to Win32. This keeps that exception from spreading. The search
    /// words are put together at run time so this file does not contain them.
    #[test]
    fn only_the_helper_window_module_may_use_unsafe_code() {
        let word = ["un", "safe"].concat();
        let allow = format!("allow({word}_code)");
        let deny = format!("#![deny({word}_code)]");
        let uses = [" {", " fn", " extern", " impl"].map(|rest| format!("{word}{rest}"));

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        assert!(files.len() > 1, "no source files found under {}", root.display());

        let mut allows = Vec::new();
        let mut used = Vec::new();
        for file in &files {
            let text = std::fs::read_to_string(file).expect("source file readable");
            let name = file.strip_prefix(&root).unwrap_or(file).display().to_string().replace('\\', "/");
            for (index, line) in text.lines().enumerate() {
                if line.contains(&allow) {
                    allows.push((name.clone(), index));
                }
                let code = line.trim_start();
                if name != "helper_window.rs" && !code.starts_with("//") && uses.iter().any(|use_| code.contains(use_)) {
                    used.push(format!("{name}:{}: {code}", index + 1));
                }
            }
        }
        assert!(used.is_empty(), "code outside helper_window.rs uses {word}: {used:#?}");
        assert_eq!(allows.len(), 1, "{allow} must appear exactly once, found at {allows:?}");
        let (file, index) = &allows[0];
        assert_eq!(file, "lib.rs", "{allow} is only allowed in lib.rs");

        let lib = std::fs::read_to_string(root.join("lib.rs")).expect("lib.rs readable");
        let lines: Vec<&str> = lib.lines().collect();
        assert_eq!(
            lines.get(index + 1).map(|line| line.trim()),
            Some("pub mod helper_window;"),
            "{allow} must sit directly above `pub mod helper_window;`"
        );
        assert!(lines.iter().any(|line| line.trim() == deny), "lib.rs must keep {deny}");
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn find_helper(_main: isize) -> Option<isize> {
        None
    }

    pub fn install(_helper: isize, _main: isize) -> Result<(), String> {
        Ok(())
    }
}
