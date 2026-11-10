//! Shared menu order and actions for native macOS and in-window desktop menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    NewWindow,
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
    CreateBranch,
    SwitchBranch,
    RenameBranch,
    ReloadHistory,
    Preferences,
    LightTheme,
    DarkTheme,
    Close,
    About,
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
        ],
    },
    Menu {
        title: "Local",
        items: &[
            action("Stage", MenuAction::Stage),
            action("Unstage", MenuAction::Unstage),
            action("Commit", MenuAction::Commit),
            pending("Discard…"),
            action("Stashes…", MenuAction::Stashes),
        ],
    },
    Menu {
        title: "Branch",
        items: &[
            action("Create…", MenuAction::CreateBranch),
            action("Switch", MenuAction::SwitchBranch),
            action("Rename…", MenuAction::RenameBranch),
            pending("Delete…"),
            pending("Merge…"),
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
        items: &[action("Close", MenuAction::Close)],
    },
    Menu {
        title: "Help",
        items: &[action("About SannyGit", MenuAction::About)],
    },
];
