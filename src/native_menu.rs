//! macOS menu bar. AppKit callbacks enqueue actions instead of borrowing the egui app.
use eframe::egui;
use objc2::{
    define_class, msg_send, rc::Retained, runtime::Sel, sel, DefinedClass, MainThreadOnly,
};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem, NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::menu::{MenuAction, MENUS};

struct HandlerState {
    sender: Sender<MenuAction>,
    edit_sender: Sender<egui::Event>,
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

        #[unsafe(method(performEditAction:))]
        fn perform_edit_action(&self, sender: &NSMenuItem) {
            if let Some(action) = NativeEditAction::from_tag(sender.tag()) {
                if let Some(event) = action.event() {
                    let _ = self.ivars().edit_sender.send(event);
                    self.ivars().context.request_repaint();
                }
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
    edit_items: Vec<Retained<NSMenuItem>>,
    receiver: Receiver<MenuAction>,
    edit_receiver: Receiver<egui::Event>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEditAction {
    Cut,
    Copy,
    Paste,
}

impl NativeEditAction {
    fn tag(self) -> isize {
        match self {
            Self::Cut => 0,
            Self::Copy => 1,
            Self::Paste => 2,
        }
    }

    fn from_tag(tag: isize) -> Option<Self> {
        match tag {
            0 => Some(Self::Cut),
            1 => Some(Self::Copy),
            2 => Some(Self::Paste),
            _ => None,
        }
    }

    fn event(self) -> Option<egui::Event> {
        match self {
            Self::Cut => Some(egui::Event::Cut),
            Self::Copy => Some(egui::Event::Copy),
            Self::Paste => {
                let pasteboard = NSPasteboard::generalPasteboard();
                // SAFETY: NSPasteboardTypeString is an immutable AppKit constant.
                let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString })?;
                let text = text.to_string().replace("\r\n", "\n");
                (!text.is_empty()).then_some(egui::Event::Paste(text))
            }
        }
    }
}

impl NativeMenu {
    pub fn new(context: &egui::Context) -> Option<Self> {
        let marker = MainThreadMarker::new()?;
        let application = NSApplication::sharedApplication(marker);
        // Preserve winit's application menu (About, Hide, Services and Quit).
        let root = application.mainMenu()?;
        let (sender, receiver) = mpsc::channel();
        let (edit_sender, edit_receiver) = mpsc::channel();
        let handler = MenuHandler::new(
            marker,
            HandlerState {
                sender,
                edit_sender,
                context: context.clone(),
                actions: crate::menu::actions(),
            },
        );
        let mut items = Vec::new();
        let mut edit_items = Vec::new();
        for definition in MENUS {
            let menu = submenu(marker, &root, definition.title);
            append_items(
                marker,
                &menu,
                definition.items,
                &handler,
                &mut items,
                &mut edit_items,
            );
        }
        Some(Self {
            _handler: handler,
            items,
            edit_items,
            receiver,
            edit_receiver,
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
    pub fn take_edit_events(&self) -> Vec<egui::Event> {
        self.edit_receiver.try_iter().collect()
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
        for item in &self.edit_items {
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
    edit_items: &mut Vec<Retained<NSMenuItem>>,
) {
    for definition in definitions {
        if definition.separator {
            menu.addItem(&NSMenuItem::separatorItem(marker));
            continue;
        }
        let standard_edit = standard_edit_item(definition.label);
        // SAFETY: The selectors below have AppKit's NSMenuItem target/action ABI.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(marker),
                &NSString::from_str(definition.label),
                definition
                    .action
                    .map(|_| sel!(performMenuAction:))
                    .or_else(|| {
                        standard_edit.map(|item| {
                            if item.native_action.is_some() {
                                sel!(performEditAction:)
                            } else {
                                item.selector
                            }
                        })
                    }),
                &NSString::from_str(standard_edit.map_or_else(
                    || match definition.action {
                        Some(MenuAction::Open | MenuAction::Clone) => "o",
                        Some(MenuAction::NewWindow) => "n",
                        Some(MenuAction::Refresh) => "\u{f708}",
                        Some(MenuAction::Close) => "\u{f707}",
                        Some(MenuAction::Exit) => "x",
                        _ => "",
                    },
                    |item| item.key,
                )),
            )
        };
        if !definition.children.is_empty() {
            let child =
                NSMenu::initWithTitle(NSMenu::alloc(marker), &NSString::from_str(definition.label));
            child.setAutoenablesItems(false);
            append_items(
                marker,
                &child,
                definition.children,
                handler,
                items,
                edit_items,
            );
            item.setSubmenu(Some(&child));
        } else if let Some(action) = definition.action {
            item.setTag(items.len() as isize);
            if action == MenuAction::Close {
                item.setKeyEquivalentModifierMask(objc2_app_kit::NSEventModifierFlags::Control);
            }
            if action == MenuAction::Exit {
                item.setKeyEquivalentModifierMask(objc2_app_kit::NSEventModifierFlags::Option);
            }
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
                    | MenuAction::Preferences
                    | MenuAction::LightTheme
                    | MenuAction::DarkTheme
                    | MenuAction::Close
                    | MenuAction::Exit
                    | MenuAction::About
            ));
            items.push((action, item.clone()));
        } else if let Some(edit) = standard_edit {
            let mut modifiers = objc2_app_kit::NSEventModifierFlags::Command;
            if edit.shift {
                modifiers |= objc2_app_kit::NSEventModifierFlags::Shift;
            }
            item.setKeyEquivalentModifierMask(modifiers);
            if let Some(action) = edit.native_action {
                item.setTag(action.tag());
                // SAFETY: The handler implements performEditAction: with AppKit's ABI.
                unsafe {
                    item.setTarget(Some(handler));
                }
                edit_items.push(item.clone());
            }
            item.setEnabled(true);
        } else {
            item.setEnabled(false);
            item.setToolTip(Some(&NSString::from_str("Not implemented yet")));
        }
        menu.addItem(&item);
    }
}

#[derive(Clone, Copy)]
struct StandardEditItem {
    selector: Sel,
    key: &'static str,
    shift: bool,
    native_action: Option<NativeEditAction>,
}

fn standard_edit_item(label: &str) -> Option<StandardEditItem> {
    let (selector, key, shift, native_action) = match label {
        "Undo" => (sel!(undo:), "z", false, None),
        "Redo" => (sel!(redo:), "z", true, None),
        "Cut" => (sel!(cut:), "x", false, Some(NativeEditAction::Cut)),
        "Copy" => (sel!(copy:), "c", false, Some(NativeEditAction::Copy)),
        "Paste" => (sel!(paste:), "v", false, Some(NativeEditAction::Paste)),
        "Select All" => (sel!(selectAll:), "a", false, None),
        _ => return None,
    };
    Some(StandardEditItem {
        selector,
        key,
        shift,
        native_action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_edit_items_have_native_shortcuts() {
        for (label, key, shift) in [
            ("Undo", "z", false),
            ("Redo", "z", true),
            ("Cut", "x", false),
            ("Copy", "c", false),
            ("Paste", "v", false),
            ("Select All", "a", false),
        ] {
            let item = standard_edit_item(label).unwrap();
            assert_eq!((item.key, item.shift), (key, shift));
        }
        for action in [
            NativeEditAction::Cut,
            NativeEditAction::Copy,
            NativeEditAction::Paste,
        ] {
            assert_eq!(NativeEditAction::from_tag(action.tag()), Some(action));
        }
        assert_eq!(
            standard_edit_item("Paste").unwrap().native_action,
            Some(NativeEditAction::Paste)
        );
        assert!(standard_edit_item("Preferences").is_none());
    }

    #[test]
    fn paste_event_is_delivered_to_the_focused_text_edit() {
        let context = egui::Context::default();
        let id = egui::Id::unique("paste.target");
        let mut value = String::new();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            ui.memory_mut(|memory| memory.request_focus(id));
            ui.input_mut(|input| input.events.push(egui::Event::Paste("pasted".into())));
            ui.add(egui::TextEdit::singleline(&mut value).id(id));
        });
        output.textures_delta.clear();
        assert_eq!(value, "pasted");
    }
}
