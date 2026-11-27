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
    Undo,
    Redo,
    SelectAll,
}

impl NativeEditAction {
    fn tag(self) -> isize {
        match self {
            Self::Cut => 0,
            Self::Copy => 1,
            Self::Paste => 2,
            Self::Undo => 3,
            Self::Redo => 4,
            Self::SelectAll => 5,
        }
    }

    fn from_tag(tag: isize) -> Option<Self> {
        match tag {
            0 => Some(Self::Cut),
            1 => Some(Self::Copy),
            2 => Some(Self::Paste),
            3 => Some(Self::Undo),
            4 => Some(Self::Redo),
            5 => Some(Self::SelectAll),
            _ => None,
        }
    }

    fn event(self) -> Option<egui::Event> {
        match self {
            Self::Undo | Self::Redo | Self::SelectAll => Some(egui::Event::Key {
                key: if self == Self::SelectAll {
                    egui::Key::A
                } else {
                    egui::Key::Z
                },
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: if self == Self::Redo {
                    egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
                } else {
                    egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND
                },
            }),
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
            let menu = submenu(marker, &root, &definition.title());
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
        let label = definition.label();
        let standard_edit = definition.edit_action.map(standard_edit_item);
        // SAFETY: The selectors below have AppKit's NSMenuItem target/action ABI.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(marker),
                &NSString::from_str(&label),
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
            let child = NSMenu::initWithTitle(NSMenu::alloc(marker), &NSString::from_str(&label));
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

fn standard_edit_item(edit: crate::menu::EditAction) -> StandardEditItem {
    let (selector, key, shift, native_action) = match edit {
        crate::menu::EditAction::Undo => (sel!(undo:), "z", false, Some(NativeEditAction::Undo)),
        crate::menu::EditAction::Redo => (sel!(redo:), "z", true, Some(NativeEditAction::Redo)),
        crate::menu::EditAction::Cut => (sel!(cut:), "x", false, Some(NativeEditAction::Cut)),
        crate::menu::EditAction::Copy => (sel!(copy:), "c", false, Some(NativeEditAction::Copy)),
        crate::menu::EditAction::Paste => (sel!(paste:), "v", false, Some(NativeEditAction::Paste)),
        crate::menu::EditAction::SelectAll => (
            sel!(selectAll:),
            "a",
            false,
            Some(NativeEditAction::SelectAll),
        ),
    };
    StandardEditItem {
        selector,
        key,
        shift,
        native_action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_edit_items_have_native_shortcuts() {
        use crate::menu::EditAction as E;
        for (edit, key, shift) in [
            (E::Undo, "z", false),
            (E::Redo, "z", true),
            (E::Cut, "x", false),
            (E::Copy, "c", false),
            (E::Paste, "v", false),
            (E::SelectAll, "a", false),
        ] {
            let item = standard_edit_item(edit);
            assert_eq!((item.key, item.shift), (key, shift));
        }
        for action in [
            NativeEditAction::Cut,
            NativeEditAction::Copy,
            NativeEditAction::Paste,
            NativeEditAction::Undo,
            NativeEditAction::Redo,
            NativeEditAction::SelectAll,
        ] {
            assert_eq!(NativeEditAction::from_tag(action.tag()), Some(action));
        }
        assert_eq!(
            standard_edit_item(crate::menu::EditAction::Paste).native_action,
            Some(NativeEditAction::Paste)
        );
    }

    #[test]
    fn native_edit_shortcuts_reach_egui() {
        for (action, key, shift) in [
            (NativeEditAction::Undo, egui::Key::Z, false),
            (NativeEditAction::Redo, egui::Key::Z, true),
            (NativeEditAction::SelectAll, egui::Key::A, false),
        ] {
            let Some(egui::Event::Key {
                key: actual,
                modifiers,
                pressed,
                ..
            }) = action.event()
            else {
                panic!("Expected a text editing key event");
            };
            assert_eq!(actual, key);
            assert!(pressed && modifiers.command && modifiers.mac_cmd);
            assert_eq!(modifiers.shift, shift);
        }
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

// AppKit tracks a popup in its native event loop and returns after selection or
// cancellation. Keep its target alive for that entire interval.
#[cfg(not(test))]
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = std::cell::Cell<Option<usize>>]
    struct PopupHandler;
    unsafe impl NSObjectProtocol for PopupHandler {}
    impl PopupHandler {
        #[unsafe(method(selectPopupItem:))]
        fn select_popup_item(&self, sender: &NSMenuItem) {
            self.ivars().set(usize::try_from(sender.tag()).ok());
        }
    }
);

#[cfg(not(test))]
pub(crate) fn popup<T: Clone>(entries: &[crate::ui::PopupMenuEntry<T>]) -> Option<T> {
    if entries.is_empty() {
        return None;
    }
    let marker = MainThreadMarker::new()?;
    let allocated = PopupHandler::alloc(marker).set_ivars(std::cell::Cell::new(None));
    // SAFETY: NSObject initializes the allocated subclass on the main thread.
    let handler: Retained<PopupHandler> = unsafe { msg_send![super(allocated), init] };
    let mut actions = Vec::new();
    let menu = popup_items(marker, entries, &handler, &mut actions);
    let pointer = objc2_app_kit::NSEvent::mouseLocation();
    let screen = objc2_app_kit::NSScreen::screens(marker)
        .iter()
        .find(|screen| {
            let frame = screen.frame();
            pointer.x >= frame.origin.x
                && pointer.x < frame.origin.x + frame.size.width
                && pointer.y >= frame.origin.y
                && pointer.y < frame.origin.y + frame.size.height
        });
    let origin = screen.map_or(pointer, |screen| {
        popup_origin(pointer, menu.size(), screen.visibleFrame())
    });
    menu.popUpMenuPositioningItem_atLocation_inView(None, origin, None);
    handler
        .ivars()
        .get()
        .and_then(|index| actions.get(index).cloned())
}

fn popup_origin(
    pointer: objc2_foundation::NSPoint,
    menu: objc2_foundation::NSSize,
    visible: objc2_foundation::NSRect,
) -> objc2_foundation::NSPoint {
    // Leave room for AppKit's menu padding around the content frame.
    const INSET: f64 = 8.0;
    let left = visible.origin.x + INSET;
    let right = visible.origin.x + visible.size.width - INSET;
    let bottom = visible.origin.y + INSET;
    let top = visible.origin.y + visible.size.height - INSET;
    let y = if pointer.y - menu.height < bottom {
        pointer.y + menu.height
    } else {
        pointer.y
    };
    objc2_foundation::NSPoint::new(
        pointer.x.min(right - menu.width).max(left),
        y.max(bottom + menu.height).min(top),
    )
}

#[cfg(test)]
#[test]
fn popup_position_uses_the_usable_bounds_of_the_clicked_display() {
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let visible = NSRect::new(NSPoint::new(-800.0, -400.0), NSSize::new(800.0, 600.0));
    let menu = NSSize::new(180.0, 240.0);
    for (pointer, expected) in [
        ((-550.0, 100.0), (-550.0, 100.0)),
        ((-550.0, -350.0), (-550.0, -110.0)),
        ((-30.0, -370.0), (-188.0, -130.0)),
        ((-550.0, 199.0), (-550.0, 192.0)),
    ] {
        let origin = popup_origin(NSPoint::new(pointer.0, pointer.1), menu, visible);
        assert_eq!(origin, NSPoint::new(expected.0, expected.1));
    }
}

#[cfg(not(test))]
fn popup_items<T: Clone>(
    marker: MainThreadMarker,
    entries: &[crate::ui::PopupMenuEntry<T>],
    handler: &PopupHandler,
    actions: &mut Vec<T>,
) -> Retained<NSMenu> {
    use crate::ui::PopupMenuEntry as Entry;
    let menu = NSMenu::initWithTitle(NSMenu::alloc(marker), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    for entry in entries {
        let item = match entry {
            Entry::Separator => NSMenuItem::separatorItem(marker),
            Entry::Submenu { label, entries } => {
                let item = NSMenuItem::new(marker);
                item.setTitle(&NSString::from_str(label));
                item.setSubmenu(Some(&popup_items(marker, entries, handler, actions)));
                item
            }
            Entry::Item {
                label,
                enabled,
                action,
                help,
                shortcut,
            } => {
                // SAFETY: PopupHandler implements this selector with NSMenuItem's action ABI.
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(marker),
                        &NSString::from_str(label),
                        Some(sel!(selectPopupItem:)),
                        &NSString::from_str(""),
                    )
                };
                if let Some(shortcut) = shortcut {
                    use eframe::egui::Key;
                    use objc2_app_kit::NSEventModifierFlags as Flags;
                    let key = match shortcut.logical_key {
                        Key::ArrowUp => "\u{f700}".into(),
                        Key::F2 => "\u{f705}".into(),
                        key => key.name().to_ascii_lowercase(),
                    };
                    let mut modifiers = Flags::empty();
                    if shortcut.modifiers.command {
                        modifiers |= Flags::Command;
                    }
                    if shortcut.modifiers.shift {
                        modifiers |= Flags::Shift;
                    }
                    item.setKeyEquivalent(&NSString::from_str(&key));
                    item.setKeyEquivalentModifierMask(modifiers);
                }
                item.setTag(actions.len() as isize);
                actions.push(action.clone());
                item.setEnabled(*enabled);
                if !help.is_empty() {
                    item.setToolTip(Some(&NSString::from_str(help)));
                }
                // SAFETY: popup retains the handler until native tracking finishes.
                unsafe {
                    item.setTarget(Some(handler));
                }
                item
            }
        };
        menu.addItem(&item);
    }
    menu
}
