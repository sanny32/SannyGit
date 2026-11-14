//! Shared menu order and actions for native macOS and in-window desktop menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    NewWindow,
    MaximizeWindow,
    MinimizeWindow,
    FullScreen,
    AddGroup,
    Clone,
    SearchRepositories,
    CloseRepository,
    AddWorktree,
    RemoveWorktree,
    PruneWorktrees,
    RepositorySettings,
    EditRepositoryConfig,
    EditUserConfig,
    Open,
    Refresh,
    Fetch,
    Pull,
    Push,
    Stage,
    Unstage,
    Commit,
    Stashes,
    Merge,
    Discard,
    DeleteBranch,
    PushTag,
    ManageRemotes,
    CreateBranch,
    SwitchBranch,
    RenameBranch,
    ReloadHistory,
    Preferences,
    LightTheme,
    DarkTheme,
    Close,
    Exit,
    About,
}

#[cfg(not(target_os = "macos"))]
pub const SHORTCUTS: &[(MenuAction, eframe::egui::KeyboardShortcut)] = {
    use eframe::egui::{Key, KeyboardShortcut as Shortcut, Modifiers};
    use MenuAction::*;
    const CTRL_SHIFT: Modifiers = Modifiers::CTRL.plus(Modifiers::SHIFT);
    &[
        (NewWindow, Shortcut::new(CTRL_SHIFT, Key::N)),
        (FullScreen, Shortcut::new(Modifiers::NONE, Key::F11)),
        (Open, Shortcut::new(Modifiers::CTRL, Key::O)),
        (Clone, Shortcut::new(CTRL_SHIFT, Key::O)),
        (CloseRepository, Shortcut::new(Modifiers::CTRL, Key::W)),
        (Close, Shortcut::new(Modifiers::CTRL, Key::F4)),
        (Exit, Shortcut::new(Modifiers::ALT, Key::X)),
        (Refresh, Shortcut::new(Modifiers::NONE, Key::F5)),
        (ReloadHistory, Shortcut::new(CTRL_SHIFT, Key::F5)),
        (Preferences, Shortcut::new(Modifiers::CTRL, Key::Comma)),
        (Fetch, Shortcut::new(CTRL_SHIFT, Key::F)),
        (Pull, Shortcut::new(CTRL_SHIFT, Key::L)),
        (Push, Shortcut::new(CTRL_SHIFT, Key::P)),
        (Stage, Shortcut::new(Modifiers::CTRL, Key::T)),
        (Unstage, Shortcut::new(CTRL_SHIFT, Key::T)),
        (Commit, Shortcut::new(Modifiers::CTRL, Key::Enter)),
        (Stashes, Shortcut::new(CTRL_SHIFT, Key::S)),
        (CreateBranch, Shortcut::new(Modifiers::CTRL, Key::B)),
    ]
};

#[cfg(not(target_os = "macos"))]
impl MenuAction {
    pub fn shortcut(self) -> Option<eframe::egui::KeyboardShortcut> {
        SHORTCUTS
            .iter()
            .find_map(|(action, shortcut)| (*action == self).then_some(*shortcut))
    }
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

#[cfg(not(target_os = "macos"))]
impl EditAction {
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "Undo" => Self::Undo,
            "Redo" => Self::Redo,
            "Cut" => Self::Cut,
            "Copy" => Self::Copy,
            "Paste" => Self::Paste,
            "Select All" => Self::SelectAll,
            _ => return None,
        })
    }

    pub fn shortcut(self) -> eframe::egui::KeyboardShortcut {
        use eframe::egui::{Key, KeyboardShortcut, Modifiers};
        let (modifiers, key) = match self {
            Self::Undo => (Modifiers::CTRL, Key::Z),
            Self::Redo if cfg!(target_os = "windows") => (Modifiers::CTRL, Key::Y),
            Self::Redo => (Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Z),
            Self::Cut => (Modifiers::CTRL, Key::X),
            Self::Copy => (Modifiers::CTRL, Key::C),
            Self::Paste => (Modifiers::CTRL, Key::V),
            Self::SelectAll => (Modifiers::CTRL, Key::A),
        };
        KeyboardShortcut::new(modifiers, key)
    }

    pub fn event(self) -> Option<eframe::egui::Event> {
        use eframe::egui::Event;
        match self {
            Self::Cut => Some(Event::Cut),
            Self::Copy => Some(Event::Copy),
            Self::Paste => None, // The window integration reads the system clipboard.
            _ => {
                let shortcut = self.shortcut();
                Some(Event::Key {
                    key: shortcut.logical_key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    // egui uses the platform-independent command flag for text editing.
                    modifiers: shortcut.modifiers.plus(eframe::egui::Modifiers::COMMAND),
                })
            }
        }
    }
}

pub struct MenuItem {
    pub label: &'static str,
    pub action: Option<MenuAction>,
    pub children: &'static [MenuItem],
    pub separator: bool,
}

pub struct Menu {
    pub title: &'static str,
    pub items: &'static [MenuItem],
}

const fn action(label: &'static str, action: MenuAction) -> MenuItem {
    MenuItem {
        label,
        action: Some(action),
        children: &[],
        separator: false,
    }
}
const fn pending(label: &'static str) -> MenuItem {
    MenuItem {
        label,
        action: None,
        children: &[],
        separator: false,
    }
}

const fn separator() -> MenuItem {
    MenuItem {
        label: "",
        action: None,
        children: &[],
        separator: true,
    }
}
const fn submenu(label: &'static str, children: &'static [MenuItem]) -> MenuItem {
    MenuItem {
        label,
        action: None,
        children,
        separator: false,
    }
}

#[cfg(target_os = "macos")]
pub fn actions() -> Vec<MenuAction> {
    fn collect(items: &[MenuItem], actions: &mut Vec<MenuAction>) {
        for item in items {
            if let Some(action) = item.action {
                actions.push(action);
            }
            collect(item.children, actions);
        }
    }
    let mut result = Vec::new();
    for menu in MENUS {
        collect(menu.items, &mut result);
    }
    result
}

pub const MENUS: &[Menu] = &[
    Menu {
        title: "Repository",
        items: &[
            action("New Window", MenuAction::NewWindow),
            action("Add or Create…", MenuAction::Open),
            action("Add Group…", MenuAction::AddGroup),
            action("Clone…", MenuAction::Clone),
            action("Search for Repositories…", MenuAction::SearchRepositories),
            action("Close Repository", MenuAction::CloseRepository),
            separator(),
            action("Add Worktree…", MenuAction::AddWorktree),
            action("Remove Worktree…", MenuAction::RemoveWorktree),
            action("Prune Obsolete Worktrees…", MenuAction::PruneWorktrees),
            separator(),
            action("Settings…", MenuAction::RepositorySettings),
            submenu(
                "Edit Git Config",
                &[
                    action("Repository…", MenuAction::EditRepositoryConfig),
                    action("User…", MenuAction::EditUserConfig),
                ],
            ),
            separator(),
            action("Close", MenuAction::Close),
            action("Exit", MenuAction::Exit),
        ],
    },
    Menu {
        title: "Edit",
        items: &[
            pending("Undo"),
            pending("Redo"),
            pending("Cut"),
            pending("Copy"),
            pending("Paste"),
            pending("Select All"),
        ],
    },
    Menu {
        title: "View",
        items: &[action("Refresh", MenuAction::Refresh)],
    },
    Menu {
        title: "Remote",
        items: &[
            action("Fetch…", MenuAction::Fetch),
            action("Pull…", MenuAction::Pull),
            action("Push…", MenuAction::Push),
            action("Push Selected Tag…", MenuAction::PushTag),
            separator(),
            action("Manage Remotes…", MenuAction::ManageRemotes),
        ],
    },
    Menu {
        title: "Local",
        items: &[
            action("Stage", MenuAction::Stage),
            action("Unstage", MenuAction::Unstage),
            action("Commit", MenuAction::Commit),
            action("Discard…", MenuAction::Discard),
            action("Stashes…", MenuAction::Stashes),
        ],
    },
    Menu {
        title: "Branch",
        items: &[
            action("Create…", MenuAction::CreateBranch),
            action("Switch", MenuAction::SwitchBranch),
            action("Rename…", MenuAction::RenameBranch),
            action("Delete…", MenuAction::DeleteBranch),
            action("Merge…", MenuAction::Merge),
            pending("Rebase…"),
        ],
    },
    Menu {
        title: "Query",
        items: &[
            action("Reload History", MenuAction::ReloadHistory),
            pending("Blame…"),
        ],
    },
    Menu {
        title: "Tools",
        items: &[
            action("Preferences…", MenuAction::Preferences),
            separator(),
            action("Light Theme", MenuAction::LightTheme),
            action("Dark Theme", MenuAction::DarkTheme),
        ],
    },
    Menu {
        title: "Review",
        items: &[pending("Review…")],
    },
    Menu {
        title: "Window",
        items: &[
            action("New Window", MenuAction::NewWindow),
            separator(),
            action("Maximize", MenuAction::MaximizeWindow),
            action("Minimize", MenuAction::MinimizeWindow),
            action("Full Screen", MenuAction::FullScreen),
        ],
    },
    Menu {
        title: "Help",
        items: &[action("About SannyGit", MenuAction::About)],
    },
];
