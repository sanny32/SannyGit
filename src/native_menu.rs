//! macOS menu bar. AppKit callbacks enqueue actions instead of borrowing the egui app.
use eframe::egui;
use objc2::{define_class, msg_send, rc::Retained, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::menu::{MenuAction, MENUS};

struct HandlerState {
    sender: Sender<MenuAction>,
    context: egui::Context,
    actions: Vec<MenuAction>,
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
        // SAFETY: AppKit passes the NSMenuItem that invokes this action.
        #[unsafe(method(performMenuAction:))]
        fn perform_menu_action(&self, sender: &NSMenuItem) {
            if let Ok(index) = usize::try_from(sender.tag()) {
                if let Some(action) = self.ivars().actions.get(index).copied() { self.enqueue(action); }
            }
        }
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
    items: Vec<(MenuAction, Retained<NSMenuItem>)>,
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
                actions: crate::menu::actions(),
            },
        );
        let mut items = Vec::new();
        for definition in MENUS {
            let menu = submenu(marker, &root, definition.title);
            append_items(marker, &menu, definition.items, &handler, &mut items);
        }
        Some(Self {
            _handler: handler,
            items,
            receiver,
        })
    }

    pub fn update_enabled(&self, enabled: impl Fn(MenuAction) -> bool) {
        for (action, item) in &self.items {
            item.setEnabled(enabled(*action));
        }
    }
    pub fn take_actions(&self) -> Vec<MenuAction> {
        self.receiver.try_iter().collect()
    }
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        // Detach the weak target before releasing the retained handler.
        for (_, item) in &self.items {
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

fn append_items(
    marker: MainThreadMarker,
    menu: &NSMenu,
    definitions: &[crate::menu::MenuItem],
    handler: &MenuHandler,
    items: &mut Vec<(MenuAction, Retained<NSMenuItem>)>,
) {
    for definition in definitions {
        if definition.separator {
            menu.addItem(&NSMenuItem::separatorItem(marker));
            continue;
        }
        // SAFETY: The selector below has AppKit's NSMenuItem target/action ABI.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(marker),
                &NSString::from_str(definition.label),
                definition.action.map(|_| sel!(performMenuAction:)),
                &NSString::from_str(match definition.action {
                    Some(MenuAction::Open | MenuAction::Clone) => "o",
                    Some(MenuAction::NewWindow) => "n",
                    Some(MenuAction::Refresh) => "\u{f708}",
                    Some(MenuAction::Close) => "w",
                    _ => "",
                }),
            )
        };
        if !definition.children.is_empty() {
            let child =
                NSMenu::initWithTitle(NSMenu::alloc(marker), &NSString::from_str(definition.label));
            child.setAutoenablesItems(false);
            append_items(marker, &child, definition.children, handler, items);
            item.setSubmenu(Some(&child));
        } else if let Some(action) = definition.action {
            item.setTag(items.len() as isize);
            if action == MenuAction::Refresh {
                item.setKeyEquivalentModifierMask(objc2_app_kit::NSEventModifierFlags::empty());
            }
            if action == MenuAction::RepositorySettings {
                item.setKeyEquivalent(&NSString::from_str("\r"));
                item.setKeyEquivalentModifierMask(objc2_app_kit::NSEventModifierFlags::Option);
            }
            if action == MenuAction::Clone {
                item.setKeyEquivalentModifierMask(
                    objc2_app_kit::NSEventModifierFlags::Command
                        | objc2_app_kit::NSEventModifierFlags::Shift,
                );
            }
            // SAFETY: NativeMenu retains the handler until its targets are detached.
            unsafe {
                item.setTarget(Some(handler));
            }
            item.setEnabled(matches!(
                action,
                MenuAction::NewWindow
                    | MenuAction::Open
                    | MenuAction::AddGroup
                    | MenuAction::Clone
                    | MenuAction::SearchRepositories
                    | MenuAction::EditUserConfig
                    | MenuAction::LightTheme
                    | MenuAction::DarkTheme
                    | MenuAction::Close
                    | MenuAction::About
            ));
            items.push((action, item.clone()));
        } else {
            item.setEnabled(false);
            item.setToolTip(Some(&NSString::from_str("Not implemented yet")));
        }
        menu.addItem(&item);
    }
}
