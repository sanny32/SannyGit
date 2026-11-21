//! AppKit ownership and input routing for native modal viewports.
use block2::RcBlock;
use eframe::egui::{Context, Id};
use objc2::{rc::Retained, runtime::AnyObject, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::NSString;
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle};
use std::{cell::RefCell, io, ptr::NonNull};

thread_local! {
    static MODAL: RefCell<Option<Modal>> = const { RefCell::new(None) };
}

struct Modal {
    owner: Retained<NSWindow>,
    window: Retained<NSWindow>,
    monitor: Retained<AnyObject>,
}

fn owner_key() -> Id {
    Id::unique("sannygit.native_owner")
}

pub fn remember_owner(context: &eframe::CreationContext<'_>) -> Result<(), HandleError> {
    let RawWindowHandle::AppKit(handle) = context.window_handle()?.as_raw() else {
        return Err(HandleError::NotSupported);
    };
    context.egui_ctx.data_mut(|data| {
        data.insert_temp(owner_key(), handle.ns_view.as_ptr() as usize);
    });
    Ok(())
}

pub fn begin_modal(context: &Context, title: &str) -> io::Result<()> {
    let mtm = MainThreadMarker::new().ok_or_else(|| io::Error::other("Expected the UI thread"))?;
    let view = context
        .data(|data| data.get_temp::<usize>(owner_key()))
        .ok_or_else(|| io::Error::other("Main window handle is unavailable"))?;
    // SAFETY: eframe owns this NSView for the lifetime of the root window.
    // The immediate viewport is rendered on the same main thread.
    let owner = unsafe { &*(view as *const NSView) }
        .window()
        .ok_or_else(|| io::Error::other("Main window is unavailable"))?;
    let title = NSString::from_str(title);
    let window = NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .find(|window| window.title() == title)
        .ok_or_else(|| io::Error::other("Native dialog window is unavailable"))?;

    // This can be called more than once during the first layout frame.
    if MODAL.with_borrow(|modal| {
        modal
            .as_ref()
            .is_some_and(|modal| Retained::as_ptr(&modal.window) == Retained::as_ptr(&window))
    }) {
        return Ok(());
    }
    end_modal(context);

    let modal_window = window.clone();
    let main_window = owner.clone();
    let handler = RcBlock::new(move |event: NonNull<NSEvent>| {
        // SAFETY: AppKit provides a live event for the duration of the callback.
        let mouse_event = unsafe { event.as_ref() };
        if mouse_event
            .window(mtm)
            .is_some_and(|target| Retained::as_ptr(&target) == Retained::as_ptr(&main_window))
        {
            modal_window.makeKeyAndOrderFront(None);
            std::ptr::null_mut()
        } else {
            event.as_ptr()
        }
    });
    // SAFETY: the monitor either passes through the original event or returns
    // null. The two windows belong to the same AppKit main thread.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown,
            &handler,
        )
    }
    .ok_or_else(|| io::Error::other("Cannot monitor clicks on the main window"))?;
    // SAFETY: the dialog is a separate, live top-level window with no parent.
    unsafe { owner.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
    MODAL.with_borrow_mut(|modal| {
        *modal = Some(Modal {
            owner,
            window,
            monitor,
        })
    });
    Ok(())
}

pub fn end_modal(_context: &Context) {
    let Some(Modal {
        owner,
        window,
        monitor,
    }) = MODAL.with_borrow_mut(Option::take)
    else {
        return;
    };
    // SAFETY: this is the token returned by the local monitor registration.
    unsafe { NSEvent::removeMonitor(&monitor) };
    owner.removeChildWindow(&window);
    if window.isKeyWindow() {
        owner.makeKeyWindow();
    }
}
