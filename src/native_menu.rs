//! macOS menu bar. AppKit callbacks enqueue actions instead of borrowing the egui app.
use eframe::egui;
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, Sel},
    sel, DefinedClass, MainThreadOnly,
};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use std::sync::mpsc::{self, Receiver, Sender};

pub enum MenuAction {
    Open,
    Refresh,
}

struct HandlerState {
    sender: Sender<MenuAction>,
    context: egui::Context,
}

define_class!(
    // SAFETY: NSObject has no additional subclassing requirements. The handler
    // lives and receives AppKit callbacks exclusively on the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = HandlerState]
    struct MenuHandler;

    unsafe impl NSObjectProtocol for MenuHandler {}

    impl MenuHandler {
        // SAFETY: These selectors use the standard target/action signature.
        #[unsafe(method(openRepository:))]
        fn open_repository(&self, _sender: &AnyObject) { self.enqueue(MenuAction::Open); }

        #[unsafe(method(refreshRepository:))]
        fn refresh_repository(&self, _sender: &AnyObject) { self.enqueue(MenuAction::Refresh); }
    }
);

impl MenuHandler {
    fn new(marker: MainThreadMarker, state: HandlerState) -> Retained<Self> {
        let object = Self::alloc(marker).set_ivars(state);
        // SAFETY: NSObject's init initializes this allocated subclass.
        unsafe { msg_send![super(object), init] }
    }

    fn enqueue(&self, action: MenuAction) {
        let _ = self.ivars().sender.send(action);
        self.ivars().context.request_repaint();
    }
}

pub struct NativeMenu {
    _handler: Retained<MenuHandler>,
    open: Retained<NSMenuItem>,
    refresh: Retained<NSMenuItem>,
    receiver: Receiver<MenuAction>,
}

impl NativeMenu {
    pub fn new(context: &egui::Context) -> Option<Self> {
        let marker = MainThreadMarker::new()?;
        let application = NSApplication::sharedApplication(marker);
        // Preserve winit's application menu (About, Hide, Services and Quit).
        let root = application.mainMenu()?;
        let (sender, receiver) = mpsc::channel();
        let handler = MenuHandler::new(
            marker,
            HandlerState {
                sender,
                context: context.clone(),
            },
        );
        let repository = submenu(marker, &root, "Repository");
        let view = submenu(marker, &root, "View");
        let open = action(marker, "Open…", "o", sel!(openRepository:), &handler);
        let refresh = action(
            marker,
            "Refresh",
            "\u{f708}",
            sel!(refreshRepository:),
            &handler,
        ); // F5
           // Function keys should not require the default Command modifier.
        refresh.setKeyEquivalentModifierMask(objc2_app_kit::NSEventModifierFlags::empty());
        refresh.setEnabled(false);
        repository.addItem(&open);
        view.addItem(&refresh);
        Some(Self {
            _handler: handler,
            open,
            refresh,
            receiver,
        })
    }

    pub fn set_refresh_enabled(&self, enabled: bool) {
        self.refresh.setEnabled(enabled);
    }
    pub fn take_actions(&self) -> Vec<MenuAction> {
        self.receiver.try_iter().collect()
    }
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        // Detach the weak target before releasing the retained handler.
        for item in [&self.open, &self.refresh] {
            item.setEnabled(false);
            // SAFETY: nil removes the callback target and selector.
            unsafe {
                item.setTarget(None);
                item.setAction(None);
            }
        }
    }
}

fn submenu(marker: MainThreadMarker, root: &NSMenu, title: &str) -> Retained<NSMenu> {
    let title = NSString::from_str(title);
    let menu = NSMenu::initWithTitle(NSMenu::alloc(marker), &title);
    menu.setAutoenablesItems(false);
    let item = NSMenuItem::new(marker);
    item.setTitle(&title);
    item.setSubmenu(Some(&menu));
    root.addItem(&item);
    menu
}

fn action(
    marker: MainThreadMarker,
    title: &str,
    key: &str,
    selector: Sel,
    handler: &MenuHandler,
) -> Retained<NSMenuItem> {
    // SAFETY: Selectors are implemented above with the target/action ABI. The
    // NativeMenu retains the handler for as long as these items can be invoked.
    unsafe {
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(marker),
            &NSString::from_str(title),
            Some(selector),
            &NSString::from_str(key),
        );
        item.setTarget(Some(handler));
        item
    }
}
