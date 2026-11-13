//! Native ownership and modality for egui viewports.
use eframe::egui::{Context, Id};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::c_void, io};

type Hwnd = *mut c_void;
const GWLP_HWNDPARENT: i32 = -8;

#[link(name = "user32")]
extern "system" {
    fn EnumThreadWindows(
        thread: u32,
        callback: unsafe extern "system" fn(Hwnd, isize) -> i32,
        data: isize,
    ) -> i32;
    fn GetWindowThreadProcessId(window: Hwnd, process: *mut u32) -> u32;
    fn GetWindowTextW(window: Hwnd, text: *mut u16, count: i32) -> i32;
    fn EnableWindow(window: Hwnd, enabled: i32) -> i32;
    fn IsWindowEnabled(window: Hwnd) -> i32;
    fn SetActiveWindow(window: Hwnd) -> Hwnd;
    #[cfg_attr(target_pointer_width = "32", link_name = "SetWindowLongW")]
    fn SetWindowLongPtrW(window: Hwnd, index: i32, value: isize) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn SetLastError(error: u32);
}

fn owner_key() -> Id {
    Id::unique("sannygit.native_owner")
}

fn disabled_owner_key() -> Id {
    Id::unique("sannygit.modal_disabled_owner")
}

pub fn remember_owner(
    context: &eframe::CreationContext<'_>,
) -> Result<(), raw_window_handle::HandleError> {
    if let RawWindowHandle::Win32(handle) = context.window_handle()?.as_raw() {
        context
            .egui_ctx
            .data_mut(|data| data.insert_temp(owner_key(), handle.hwnd.get()));
    }
    Ok(())
}

pub fn begin_modal(context: &Context, title: &str) -> io::Result<()> {
    let owner = context
        .data(|data| data.get_temp::<isize>(owner_key()))
        .ok_or_else(|| io::Error::other("Main window handle is unavailable"))?;
    let mut search = WindowSearch {
        title: title.encode_utf16().collect(),
        window: std::ptr::null_mut(),
    };
    // SAFETY: the root HWND is supplied by eframe and remains alive for this UI
    // callback. Enumeration is synchronous; the search pointer stays valid.
    unsafe {
        let thread = GetWindowThreadProcessId(owner as Hwnd, std::ptr::null_mut());
        if thread == 0 {
            return Err(io::Error::last_os_error());
        }
        EnumThreadWindows(
            thread,
            find_window,
            &mut search as *mut WindowSearch as isize,
        );
    }
    if search.window.is_null() {
        return Err(io::Error::other("Settings window handle is unavailable"));
    }
    set_owner(search.window, owner as Hwnd)?;
    // SAFETY: the root HWND is live on this thread. Remember only a window we
    // actually disable, so closing cannot enable a previously disabled owner.
    unsafe {
        if IsWindowEnabled(owner as Hwnd) != 0 {
            context.data_mut(|data| data.insert_temp(disabled_owner_key(), owner));
            EnableWindow(owner as Hwnd, 0);
        }
    }
    Ok(())
}

pub fn end_modal(context: &Context) {
    let owner = context.data_mut(|data| data.remove_temp::<isize>(disabled_owner_key()));
    if let Some(owner) = owner {
        // SAFETY: the owner lives for the application's UI lifetime. Re-enable
        // it before closing the dialog so Windows can return activation to it.
        unsafe {
            EnableWindow(owner as Hwnd, 1);
            SetActiveWindow(owner as Hwnd);
        }
    }
}

struct WindowSearch {
    title: Vec<u16>,
    window: Hwnd,
}

unsafe extern "system" fn find_window(window: Hwnd, data: isize) -> i32 {
    // SAFETY: EnumThreadWindows receives the live WindowSearch from begin_modal.
    let search = unsafe { &mut *(data as *mut WindowSearch) };
    let mut title = [0u16; 256];
    // SAFETY: the buffer is writable and the HWND comes from Windows enumeration.
    let length = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
    if length > 0 && title[..length as usize] == search.title {
        search.window = window;
        return 0;
    }
    1
}

fn set_owner(window: Hwnd, owner: Hwnd) -> io::Result<()> {
    // SAFETY: both HWNDs belong to the current UI thread. GWLP_HWNDPARENT
    // assigns a top-level owner, preserving native decorations and positioning.
    unsafe {
        SetLastError(0);
        let previous = SetWindowLongPtrW(window, GWLP_HWNDPARENT, owner as isize);
        let error = io::Error::last_os_error();
        if previous == 0 && error.raw_os_error() != Some(0) {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[link(name = "user32")]
    extern "system" {
        fn CreateWindowExW(
            ex_style: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: Hwnd,
            instance: Hwnd,
            param: *mut c_void,
        ) -> Hwnd;
        fn DestroyWindow(window: Hwnd) -> i32;
        fn GetWindow(window: Hwnd, command: u32) -> Hwnd;
        #[cfg_attr(target_pointer_width = "32", link_name = "GetWindowLongW")]
        fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
    }

    struct TestWindow(Hwnd);

    impl TestWindow {
        fn new(title: &str) -> Self {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
            // SAFETY: STATIC is a system class; strings are terminated and live
            // through the call. The window stays hidden and belongs to this thread.
            let window = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    0x00CF0000,
                    0,
                    0,
                    100,
                    100,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            assert!(!window.is_null(), "{}", io::Error::last_os_error());
            Self(window)
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: each test window is destroyed on its creating thread.
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    #[test]
    fn settings_get_a_native_owner_and_keep_their_window_style() {
        let owner = TestWindow::new("SannyGit test owner");
        let context = Context::default();
        context.data_mut(|data| data.insert_temp(owner_key(), owner.0 as isize));
        for title in ["SannyGit Preferences", "Repository LLM Settings"] {
            let settings = TestWindow::new(title);
            let unrelated = TestWindow::new("Unrelated window");
            // SAFETY: the windows are live and owned by this test thread.
            let style = unsafe { GetWindowLongPtrW(settings.0, -16) };
            begin_modal(&context, title).unwrap();
            // Repeat to cover the nonzero previous-owner return value.
            begin_modal(&context, title).unwrap();
            unsafe {
                assert_eq!(GetWindow(settings.0, 4), owner.0); // GW_OWNER
                assert!(GetWindow(unrelated.0, 4).is_null());
                assert_eq!(GetWindowLongPtrW(settings.0, -16), style);
                assert_eq!(style & 0x40000000, 0); // no WS_CHILD
                assert_eq!(IsWindowEnabled(owner.0), 0);
                assert_ne!(IsWindowEnabled(settings.0), 0);
                assert_ne!(IsWindowEnabled(unrelated.0), 0);
            }
            end_modal(&context);
            end_modal(&context); // closing more than once is harmless
            assert_ne!(unsafe { IsWindowEnabled(owner.0) }, 0);
        }
        assert!(begin_modal(&context, "Missing settings window").is_err());
        assert_ne!(unsafe { IsWindowEnabled(owner.0) }, 0);
    }

    #[test]
    fn closing_modal_does_not_enable_an_already_disabled_owner() {
        let owner = TestWindow::new("SannyGit test owner");
        let _settings = TestWindow::new("SannyGit Preferences");
        let context = Context::default();
        context.data_mut(|data| data.insert_temp(owner_key(), owner.0 as isize));
        unsafe {
            EnableWindow(owner.0, 0);
        }
        begin_modal(&context, "SannyGit Preferences").unwrap();
        end_modal(&context);
        assert_eq!(unsafe { IsWindowEnabled(owner.0) }, 0);
    }

    #[test]
    fn missing_owner_and_invalid_native_handle_return_errors() {
        assert!(begin_modal(&Context::default(), "SannyGit Preferences").is_err());
        assert!(set_owner(std::ptr::null_mut(), std::ptr::null_mut()).is_err());
    }
}
