//! Shared menu order and actions for native macOS and in-window desktop menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Open,
    Refresh,
    Stage,
    Unstage,
    Commit,
    CreateBranch,
    SwitchBranch,
    ReloadHistory,
    LightTheme,
    DarkTheme,
    Close,
    About,
}

pub struct MenuItem {
    pub label: &'static str,
    pub action: Option<MenuAction>,
}

pub struct Menu {
    pub title: &'static str,
    pub items: &'static [MenuItem],
}

const fn action(label: &'static str, action: MenuAction) -> MenuItem {
    MenuItem {
        label,
        action: Some(action),
    }
}
const fn pending(label: &'static str) -> MenuItem {
    MenuItem {
        label,
        action: None,
    }
}

pub const MENUS: &[Menu] = &[
    Menu {
        title: "Repository",
        items: &[
            action("Open…", MenuAction::Open),
            pending("Clone…"),
            pending("Close Repository"),
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
        items: &[pending("Fetch"), pending("Pull"), pending("Push")],
    },
    Menu {
        title: "Local",
        items: &[
            action("Stage", MenuAction::Stage),
            action("Unstage", MenuAction::Unstage),
            action("Commit", MenuAction::Commit),
            pending("Discard…"),
            pending("Save Stash…"),
        ],
    },
    Menu {
        title: "Branch",
        items: &[
            action("Create…", MenuAction::CreateBranch),
            action("Switch", MenuAction::SwitchBranch),
            pending("Rename…"),
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
