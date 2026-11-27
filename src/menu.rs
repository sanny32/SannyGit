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
    Rebase,
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
impl MenuAction {
    pub fn shortcut(
        self,
        bindings: &crate::shortcuts::ShortcutBindings,
    ) -> Option<eframe::egui::KeyboardShortcut> {
        crate::shortcuts::menu_action_shortcut(bindings, self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl EditAction {
    #[cfg(test)]
    pub fn from_label_key(key: &str) -> Option<Self> {
        Some(match key {
            "edit.undo" => Self::Undo,
            "edit.redo" => Self::Redo,
            "edit.cut" => Self::Cut,
            "edit.copy" => Self::Copy,
            "edit.paste" => Self::Paste,
            "edit.select_all" => Self::SelectAll,
            _ => return None,
        })
    }
}

#[cfg(not(target_os = "macos"))]
impl EditAction {
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
    pub label_key: &'static str,
    pub action: Option<MenuAction>,
    pub edit_action: Option<EditAction>,
    pub children: &'static [MenuItem],
    pub separator: bool,
}

impl MenuItem {
    pub fn label(&self) -> String {
        crate::i18n::tr_key(self.label_key)
    }
}

pub struct Menu {
    pub title_key: &'static str,
    pub items: &'static [MenuItem],
}

impl Menu {
    pub fn title(&self) -> String {
        crate::i18n::tr_key(self.title_key)
    }
}

const fn action(label_key: &'static str, action: MenuAction) -> MenuItem {
    MenuItem {
        label_key,
        action: Some(action),
        edit_action: None,
        children: &[],
        separator: false,
    }
}

const fn pending(label_key: &'static str) -> MenuItem {
    MenuItem {
        label_key,
        action: None,
        edit_action: None,
        children: &[],
        separator: false,
    }
}

const fn edit(edit: EditAction) -> MenuItem {
    let label_key = match edit {
        EditAction::Undo => "edit.undo",
        EditAction::Redo => "edit.redo",
        EditAction::Cut => "edit.cut",
        EditAction::Copy => "edit.copy",
        EditAction::Paste => "edit.paste",
        EditAction::SelectAll => "edit.select_all",
    };
    MenuItem {
        label_key,
        action: None,
        edit_action: Some(edit),
        children: &[],
        separator: false,
    }
}

const fn separator() -> MenuItem {
    MenuItem {
        label_key: "",
        action: None,
        edit_action: None,
        children: &[],
        separator: true,
    }
}

const fn submenu(label_key: &'static str, children: &'static [MenuItem]) -> MenuItem {
    MenuItem {
        label_key,
        action: None,
        edit_action: None,
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
        title_key: "menu_title.repository",
        items: &[
            action("repository.new_window", MenuAction::NewWindow),
            action("repository.add_or_create", MenuAction::Open),
            action("repository.add_group", MenuAction::AddGroup),
            action("repository.clone", MenuAction::Clone),
            action("repository.search", MenuAction::SearchRepositories),
            action("repository.close_repository", MenuAction::CloseRepository),
            separator(),
            action("repository.add_worktree", MenuAction::AddWorktree),
            action("repository.remove_worktree", MenuAction::RemoveWorktree),
            action("repository.prune_worktrees", MenuAction::PruneWorktrees),
            separator(),
            action("repository.settings", MenuAction::RepositorySettings),
            submenu(
                "menu.edit_git_config",
                &[
                    action("menu.repository_config", MenuAction::EditRepositoryConfig),
                    action("menu.user_config", MenuAction::EditUserConfig),
                ],
            ),
            separator(),
            action("repository.close", MenuAction::Close),
            action("repository.exit", MenuAction::Exit),
        ],
    },
    Menu {
        title_key: "menu_title.edit",
        items: &[
            edit(EditAction::Undo),
            edit(EditAction::Redo),
            edit(EditAction::Cut),
            edit(EditAction::Copy),
            edit(EditAction::Paste),
            edit(EditAction::SelectAll),
        ],
    },
    Menu {
        title_key: "menu_title.view",
        items: &[action("view.refresh", MenuAction::Refresh)],
    },
    Menu {
        title_key: "menu_title.remote",
        items: &[
            action("remote.fetch", MenuAction::Fetch),
            action("remote.pull", MenuAction::Pull),
            action("remote.push", MenuAction::Push),
            action("remote.push_tag", MenuAction::PushTag),
            separator(),
            action("remote.manage", MenuAction::ManageRemotes),
        ],
    },
    Menu {
        title_key: "menu_title.local",
        items: &[
            action("local.stage", MenuAction::Stage),
            action("local.unstage", MenuAction::Unstage),
            action("local.commit", MenuAction::Commit),
            action("local.discard", MenuAction::Discard),
            action("local.stashes", MenuAction::Stashes),
        ],
    },
    Menu {
        title_key: "menu_title.branch",
        items: &[
            action("branch.create", MenuAction::CreateBranch),
            action("branch.switch", MenuAction::SwitchBranch),
            action("branch.rename", MenuAction::RenameBranch),
            action("branch.delete", MenuAction::DeleteBranch),
            action("branch.merge_picker", MenuAction::Merge),
            action("branch.rebase_picker", MenuAction::Rebase),
        ],
    },
    Menu {
        title_key: "menu_title.query",
        items: &[
            action("query.reload_history", MenuAction::ReloadHistory),
            pending("menu.blame"),
        ],
    },
    Menu {
        title_key: "menu_title.tools",
        items: &[
            action("tools.preferences", MenuAction::Preferences),
            separator(),
            action("tools.light_theme", MenuAction::LightTheme),
            action("tools.dark_theme", MenuAction::DarkTheme),
        ],
    },
    Menu {
        title_key: "menu_title.review",
        items: &[pending("menu.review")],
    },
    Menu {
        title_key: "menu_title.window",
        items: &[
            action("repository.new_window", MenuAction::NewWindow),
            separator(),
            action("window.maximize", MenuAction::MaximizeWindow),
            action("window.minimize", MenuAction::MinimizeWindow),
            action("window.full_screen", MenuAction::FullScreen),
        ],
    },
    Menu {
        title_key: "menu_title.help",
        items: &[action("help.about", MenuAction::About)],
    },
];
