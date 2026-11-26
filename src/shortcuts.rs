//! SmartGit-aligned keyboard shortcuts and user overrides.
use eframe::egui::{Key, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::menu::MenuAction;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutOverride {
    Custom(StoredShortcut),
    Cleared,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutBindings {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, ShortcutOverride>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredShortcut {
    pub modifiers: u16,
    pub key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchShortcut {
    CheckOut,
    Merge,
    RebaseHeadTo,
    Push,
    PushTo,
    Log,
    Reset,
    ResetAdvanced,
    Rename,
    Copy,
    FastForward,
    SetTracking,
    StopTracking,
    CopyMessage,
    CopyId,
}

pub struct ShortcutCommand {
    pub id: &'static str,
    pub category: &'static str,
    pub label: &'static str,
    pub menu_action: Option<MenuAction>,
    pub branch: Option<BranchShortcut>,
    pub default: Option<KeyboardShortcut>,
}

#[cfg(target_os = "macos")]
const REMOTE_MODIFIERS: Modifiers = Modifiers::COMMAND;
#[cfg(not(target_os = "macos"))]
const REMOTE_MODIFIERS: Modifiers = Modifiers::ALT;

const PRIMARY_MODIFIERS: Modifiers = Modifiers::COMMAND;

pub fn default_shortcut(id: &str) -> Option<KeyboardShortcut> {
    catalog()
        .iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.default)
}

pub fn catalog() -> &'static [ShortcutCommand] {
    use BranchShortcut as B;
    use Key::*;
    use MenuAction::*;
    const CTRL_SHIFT: Modifiers = Modifiers::CTRL.plus(Modifiers::SHIFT);
    const ALT_SHIFT: Modifiers = REMOTE_MODIFIERS.plus(Modifiers::SHIFT);
    static CATALOG: &[ShortcutCommand] = &[
        ShortcutCommand {
            id: "repository.new_window",
            category: "Repository",
            label: "New Window",
            menu_action: Some(NewWindow),
            branch: None,
            default: Some(KeyboardShortcut::new(CTRL_SHIFT, Key::N)),
        },
        ShortcutCommand {
            id: "repository.add_or_create",
            category: "Repository",
            label: "Add or Create…",
            menu_action: Some(Open),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::CTRL, Key::O)),
        },
        ShortcutCommand {
            id: "repository.clone",
            category: "Repository",
            label: "Clone…",
            menu_action: Some(Clone),
            branch: None,
            default: Some(KeyboardShortcut::new(CTRL_SHIFT, Key::O)),
        },
        ShortcutCommand {
            id: "repository.add_group",
            category: "Repository",
            label: "Add Group…",
            menu_action: Some(AddGroup),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "repository.search",
            category: "Repository",
            label: "Search for Repositories…",
            menu_action: Some(SearchRepositories),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "repository.close_repository",
            category: "Repository",
            label: "Close Repository",
            menu_action: Some(CloseRepository),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::CTRL, W)),
        },
        ShortcutCommand {
            id: "repository.add_worktree",
            category: "Repository",
            label: "Add Worktree…",
            menu_action: Some(AddWorktree),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "repository.remove_worktree",
            category: "Repository",
            label: "Remove Worktree…",
            menu_action: Some(RemoveWorktree),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "repository.prune_worktrees",
            category: "Repository",
            label: "Prune Obsolete Worktrees…",
            menu_action: Some(PruneWorktrees),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "repository.edit_config",
            category: "Repository",
            label: "Edit Repository Config…",
            menu_action: Some(EditRepositoryConfig),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "repository.edit_user_config",
            category: "Repository",
            label: "Edit User Config…",
            menu_action: Some(EditUserConfig),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "repository.settings",
            category: "Repository",
            label: "Settings…",
            menu_action: Some(RepositorySettings),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::ALT, Enter)),
        },
        ShortcutCommand {
            id: "repository.close",
            category: "Repository",
            label: "Close",
            menu_action: Some(Close),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::CTRL, F4)),
        },
        ShortcutCommand {
            id: "repository.exit",
            category: "Repository",
            label: "Exit",
            menu_action: Some(Exit),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::ALT, X)),
        },
        ShortcutCommand {
            id: "tools.preferences",
            category: "Tools",
            label: "Preferences…",
            menu_action: Some(Preferences),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::CTRL, Comma)),
        },
        ShortcutCommand {
            id: "tools.light_theme",
            category: "Tools",
            label: "Light Theme",
            menu_action: Some(LightTheme),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "tools.dark_theme",
            category: "Tools",
            label: "Dark Theme",
            menu_action: Some(DarkTheme),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "help.about",
            category: "Help",
            label: "About SannyGit",
            menu_action: Some(About),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "view.refresh",
            category: "View",
            label: "Refresh",
            menu_action: Some(Refresh),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::NONE, F5)),
        },
        ShortcutCommand {
            id: "window.maximize",
            category: "Window",
            label: "Maximize",
            menu_action: Some(MaximizeWindow),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "window.minimize",
            category: "Window",
            label: "Minimize",
            menu_action: Some(MinimizeWindow),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "window.full_screen",
            category: "Window",
            label: "Full Screen",
            menu_action: Some(FullScreen),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::NONE, F11)),
        },
        ShortcutCommand {
            id: "remote.fetch",
            category: "Remote",
            label: "Fetch…",
            menu_action: Some(Fetch),
            branch: None,
            default: Some(KeyboardShortcut::new(ALT_SHIFT, PageDown)),
        },
        ShortcutCommand {
            id: "remote.pull",
            category: "Remote",
            label: "Pull…",
            menu_action: Some(Pull),
            branch: None,
            default: Some(KeyboardShortcut::new(REMOTE_MODIFIERS, PageDown)),
        },
        ShortcutCommand {
            id: "remote.push",
            category: "Remote",
            label: "Push…",
            menu_action: Some(Push),
            branch: None,
            default: Some(KeyboardShortcut::new(REMOTE_MODIFIERS, PageUp)),
        },
        ShortcutCommand {
            id: "remote.push_tag",
            category: "Remote",
            label: "Push Selected Tag…",
            menu_action: Some(PushTag),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "remote.manage",
            category: "Remote",
            label: "Manage Remotes…",
            menu_action: Some(ManageRemotes),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "local.stage",
            category: "Local",
            label: "Stage",
            menu_action: Some(Stage),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::CTRL, T)),
        },
        ShortcutCommand {
            id: "local.unstage",
            category: "Local",
            label: "Unstage",
            menu_action: Some(Unstage),
            branch: None,
            default: Some(KeyboardShortcut::new(CTRL_SHIFT, T)),
        },
        ShortcutCommand {
            id: "local.commit",
            category: "Local",
            label: "Commit",
            menu_action: Some(Commit),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::CTRL, K)),
        },
        ShortcutCommand {
            id: "local.discard",
            category: "Local",
            label: "Discard…",
            menu_action: Some(Discard),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "local.stashes",
            category: "Local",
            label: "Stashes…",
            menu_action: Some(Stashes),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::CTRL, S)),
        },
        ShortcutCommand {
            id: "branch.create",
            category: "Branch",
            label: "Create…",
            menu_action: Some(CreateBranch),
            branch: None,
            default: Some(KeyboardShortcut::new(Modifiers::NONE, F7)),
        },
        ShortcutCommand {
            id: "branch.switch",
            category: "Branch",
            label: "Switch",
            menu_action: Some(SwitchBranch),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "branch.delete",
            category: "Branch",
            label: "Delete…",
            menu_action: Some(DeleteBranch),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "branch.merge_picker",
            category: "Branch",
            label: "Merge…",
            menu_action: Some(Merge),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "branch.rebase_picker",
            category: "Branch",
            label: "Rebase…",
            menu_action: Some(Rebase),
            branch: None,
            default: None,
        },
        ShortcutCommand {
            id: "query.reload_history",
            category: "Query",
            label: "Reload History",
            menu_action: Some(ReloadHistory),
            branch: None,
            default: Some(KeyboardShortcut::new(CTRL_SHIFT, F5)),
        },
        ShortcutCommand {
            id: "branch.check_out",
            category: "Branch",
            label: "Check Out…",
            menu_action: None,
            branch: Some(B::CheckOut),
            default: Some(KeyboardShortcut::new(PRIMARY_MODIFIERS, G)),
        },
        ShortcutCommand {
            id: "branch.merge",
            category: "Branch",
            label: "Merge Branch…",
            menu_action: None,
            branch: Some(B::Merge),
            default: Some(KeyboardShortcut::new(PRIMARY_MODIFIERS, M)),
        },
        ShortcutCommand {
            id: "branch.rebase_head_to",
            category: "Branch",
            label: "Rebase Branch…",
            menu_action: None,
            branch: Some(B::RebaseHeadTo),
            default: Some(KeyboardShortcut::new(PRIMARY_MODIFIERS, D)),
        },
        ShortcutCommand {
            id: "branch.push",
            category: "Branch",
            label: "Push",
            menu_action: None,
            branch: Some(B::Push),
            default: Some(KeyboardShortcut::new(REMOTE_MODIFIERS, PageUp)),
        },
        ShortcutCommand {
            id: "branch.push_to",
            category: "Branch",
            label: "Push To…",
            menu_action: None,
            branch: Some(B::PushTo),
            default: Some(KeyboardShortcut::new(ALT_SHIFT, PageUp)),
        },
        ShortcutCommand {
            id: "branch.log",
            category: "Branch",
            label: "Log",
            menu_action: None,
            branch: Some(B::Log),
            default: Some(KeyboardShortcut::new(PRIMARY_MODIFIERS, L)),
        },
        ShortcutCommand {
            id: "branch.reset",
            category: "Branch",
            label: "Reset…",
            menu_action: None,
            branch: Some(B::Reset),
            default: Some(KeyboardShortcut::new(PRIMARY_MODIFIERS, R)),
        },
        ShortcutCommand {
            id: "branch.reset_advanced",
            category: "Branch",
            label: "Reset Advanced…",
            menu_action: None,
            branch: Some(B::ResetAdvanced),
            default: Some(KeyboardShortcut::new(
                PRIMARY_MODIFIERS.plus(Modifiers::SHIFT),
                Key::R,
            )),
        },
        ShortcutCommand {
            id: "branch.rename",
            category: "Branch",
            label: "Rename…",
            menu_action: Some(RenameBranch),
            branch: Some(B::Rename),
            default: Some(KeyboardShortcut::new(Modifiers::NONE, F2)),
        },
        ShortcutCommand {
            id: "branch.copy",
            category: "Branch",
            label: "Copy",
            menu_action: None,
            branch: Some(B::Copy),
            default: Some(KeyboardShortcut::new(PRIMARY_MODIFIERS, C)),
        },
        ShortcutCommand {
            id: "branch.fast_forward",
            category: "Branch",
            label: "Fast-Forward Merge",
            menu_action: None,
            branch: Some(B::FastForward),
            default: None,
        },
        ShortcutCommand {
            id: "branch.set_tracking",
            category: "Branch",
            label: "Set Tracked Branch…",
            menu_action: None,
            branch: Some(B::SetTracking),
            default: None,
        },
        ShortcutCommand {
            id: "branch.stop_tracking",
            category: "Branch",
            label: "Stop Tracking…",
            menu_action: None,
            branch: Some(B::StopTracking),
            default: None,
        },
        ShortcutCommand {
            id: "branch.copy_message",
            category: "Branch",
            label: "Copy Message",
            menu_action: None,
            branch: Some(B::CopyMessage),
            default: None,
        },
        ShortcutCommand {
            id: "branch.copy_id",
            category: "Branch",
            label: "Copy ID",
            menu_action: None,
            branch: Some(B::CopyId),
            default: None,
        },
    ];
    CATALOG
}

impl ShortcutBindings {
    pub fn effective(&self, id: &str) -> Option<KeyboardShortcut> {
        match self.overrides.get(id) {
            Some(ShortcutOverride::Cleared) => None,
            Some(ShortcutOverride::Custom(stored)) => stored.to_egui(),
            None => default_shortcut(id),
        }
    }

    pub fn menu_action(&self, action: MenuAction) -> Option<KeyboardShortcut> {
        let id = catalog()
            .iter()
            .find(|entry| entry.menu_action == Some(action))?
            .id;
        self.effective(id)
    }

    pub fn branch(&self, kind: BranchShortcut) -> Option<KeyboardShortcut> {
        let id = catalog()
            .iter()
            .find(|entry| entry.branch == Some(kind))?
            .id;
        self.effective(id)
    }

    pub fn assign(&mut self, id: &str, shortcut: Option<KeyboardShortcut>) {
        match shortcut {
            Some(shortcut) if default_shortcut(id) == Some(shortcut) => {
                self.overrides.remove(id);
            }
            Some(shortcut) => {
                self.overrides.insert(
                    id.to_owned(),
                    ShortcutOverride::Custom(StoredShortcut::from_egui(shortcut)),
                );
            }
            None => {
                self.overrides
                    .insert(id.to_owned(), ShortcutOverride::Cleared);
            }
        }
    }

    pub fn reset(&mut self, id: &str) {
        self.overrides.remove(id);
    }

    pub fn is_customized(&self, id: &str) -> bool {
        self.overrides.contains_key(id)
    }

    pub fn conflicts_for(&self, id: &str, shortcut: KeyboardShortcut) -> Vec<&'static str> {
        catalog()
            .iter()
            .filter(|command| command.id != id)
            .filter(|command| {
                self.effective(command.id)
                    .is_some_and(|assigned| chords_equivalent(assigned, shortcut))
            })
            .map(|command| command.id)
            .collect()
    }

    pub fn assign_resolving(&mut self, id: &str, shortcut: KeyboardShortcut, clear_others: bool) {
        if clear_others {
            let others = self.conflicts_for(id, shortcut);
            for other in others {
                self.assign(other, None);
            }
        }
        self.assign(id, Some(shortcut));
    }

    pub fn menu_actions_with_shortcuts() -> impl Iterator<Item = MenuAction> + 'static {
        catalog().iter().filter_map(|entry| entry.menu_action)
    }
}

/// Ctrl and Command name the same physical key on Windows and Linux.
pub fn chords_equivalent(left: KeyboardShortcut, right: KeyboardShortcut) -> bool {
    if left.logical_key != right.logical_key
        || left.modifiers.alt != right.modifiers.alt
        || left.modifiers.shift != right.modifiers.shift
    {
        return false;
    }
    if cfg!(target_os = "macos") {
        return left.modifiers.ctrl == right.modifiers.ctrl
            && left.modifiers.command == right.modifiers.command
            && left.modifiers.mac_cmd == right.modifiers.mac_cmd;
    }
    let left_primary = left.modifiers.ctrl || left.modifiers.command;
    let right_primary = right.modifiers.ctrl || right.modifiers.command;
    left_primary == right_primary
}

impl StoredShortcut {
    pub fn from_egui(shortcut: KeyboardShortcut) -> Self {
        let mut bits = 0u16;
        if shortcut.modifiers.ctrl {
            bits |= 1;
        }
        if shortcut.modifiers.shift {
            bits |= 2;
        }
        if shortcut.modifiers.alt {
            bits |= 4;
        }
        if shortcut.modifiers.mac_cmd {
            bits |= 8;
        }
        if shortcut.modifiers.command {
            bits |= 16;
        }
        Self {
            modifiers: bits,
            key: shortcut.logical_key.name().to_string(),
        }
    }

    pub fn to_egui(&self) -> Option<KeyboardShortcut> {
        let key = key_from_name(&self.key)?;
        let mut modifiers = Modifiers::NONE;
        if self.modifiers & 1 != 0 {
            modifiers = modifiers.plus(Modifiers::CTRL);
        }
        if self.modifiers & 2 != 0 {
            modifiers = modifiers.plus(Modifiers::SHIFT);
        }
        if self.modifiers & 4 != 0 {
            modifiers = modifiers.plus(Modifiers::ALT);
        }
        if self.modifiers & 8 != 0 {
            modifiers = modifiers.plus(Modifiers::MAC_CMD);
        }
        if self.modifiers & 16 != 0 {
            modifiers = modifiers.plus(Modifiers::COMMAND);
        }
        Some(KeyboardShortcut::new(modifiers, key))
    }
}

fn key_from_name(name: &str) -> Option<Key> {
    match name {
        "enter" | "Enter" => Some(Key::Enter),
        "comma" | "Comma" => Some(Key::Comma),
        "pageup" | "PageUp" => Some(Key::PageUp),
        "pagedown" | "PageDown" => Some(Key::PageDown),
        _ => catalog_keys().find(|key| key.name().eq_ignore_ascii_case(name)),
    }
}

fn catalog_keys() -> impl Iterator<Item = Key> {
    [
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::PageUp,
        Key::PageDown,
    ]
    .into_iter()
}

pub fn format_shortcut_text(ctx: &eframe::egui::Context, shortcut: KeyboardShortcut) -> String {
    ctx.format_shortcut(&shortcut).to_string()
}

pub fn shortcut_from_input(key: Key, modifiers: Modifiers) -> Option<KeyboardShortcut> {
    if modifiers == Modifiers::NONE && matches!(key, Key::Escape | Key::Tab) {
        return None;
    }
    let shortcut = KeyboardShortcut::new(modifiers, key);
    if modifiers == Modifiers::NONE && !matches!(key, Key::F2 | Key::F7 | Key::F5 | Key::F11) {
        return None;
    }
    Some(shortcut)
}

#[cfg(test)]
fn parse_shortcut_text(text: &str) -> Option<KeyboardShortcut> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut modifiers = Modifiers::NONE;
    let mut key_part = None;
    for part in text.split('+') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers = modifiers.plus(Modifiers::CTRL),
            "shift" => modifiers = modifiers.plus(Modifiers::SHIFT),
            "alt" => modifiers = modifiers.plus(Modifiers::ALT),
            "cmd" | "command" | "meta" => modifiers = modifiers.plus(Modifiers::COMMAND),
            _ => key_part = Some(part.to_string()),
        }
    }
    let key_name = key_part?;
    let key = key_from_name(&key_name)?;
    Some(KeyboardShortcut::new(modifiers, key))
}

#[cfg(not(target_os = "macos"))]
pub fn menu_action_shortcut(
    bindings: &ShortcutBindings,
    action: MenuAction,
) -> Option<KeyboardShortcut> {
    bindings.menu_action(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use MenuAction::*;

    #[test]
    fn smartgit_defaults_match_key_repository_and_local_actions() {
        let bindings = ShortcutBindings::default();
        assert_eq!(
            bindings.menu_action(Open),
            Some(KeyboardShortcut::new(Modifiers::CTRL, Key::O))
        );
        assert_eq!(
            bindings.menu_action(RepositorySettings),
            Some(KeyboardShortcut::new(Modifiers::ALT, Key::Enter))
        );
        assert_eq!(
            bindings.menu_action(Commit),
            Some(KeyboardShortcut::new(Modifiers::CTRL, Key::K))
        );
        assert_eq!(
            bindings.menu_action(CreateBranch),
            Some(KeyboardShortcut::new(Modifiers::NONE, Key::F7))
        );
        assert_eq!(
            bindings.menu_action(Stashes),
            Some(KeyboardShortcut::new(Modifiers::CTRL, Key::S))
        );
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(
                bindings.menu_action(Pull),
                Some(KeyboardShortcut::new(Modifiers::ALT, Key::PageDown))
            );
            assert_eq!(
                bindings.menu_action(Push),
                Some(KeyboardShortcut::new(Modifiers::ALT, Key::PageUp))
            );
            assert_eq!(
                bindings.menu_action(Fetch),
                Some(KeyboardShortcut::new(
                    Modifiers::ALT.plus(Modifiers::SHIFT),
                    Key::PageDown
                ))
            );
        }
    }

    #[test]
    fn overrides_round_trip_through_preferences() {
        let mut bindings = ShortcutBindings::default();
        let custom = KeyboardShortcut::new(Modifiers::CTRL, Key::J);
        bindings.assign("local.commit", Some(custom));
        assert_eq!(bindings.effective("local.commit"), Some(custom));
        bindings.reset("local.commit");
        assert_eq!(
            bindings.effective("local.commit"),
            Some(KeyboardShortcut::new(Modifiers::CTRL, Key::K))
        );
        bindings.assign("local.commit", None);
        assert_eq!(bindings.effective("local.commit"), None);
    }

    #[test]
    fn ctrl_and_command_name_the_same_chord_off_macos() {
        let ctrl = KeyboardShortcut::new(Modifiers::CTRL, Key::K);
        let command = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
        #[cfg(not(target_os = "macos"))]
        assert!(chords_equivalent(ctrl, command));
        #[cfg(target_os = "macos")]
        assert!(!chords_equivalent(ctrl, command));
        assert!(!chords_equivalent(
            ctrl,
            KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::K)
        ));
    }

    #[test]
    fn branch_push_conflicts_with_the_remote_push_command() {
        let bindings = ShortcutBindings::default();
        let push = bindings.menu_action(Push).unwrap();
        let others = bindings.conflicts_for("remote.push", push);
        assert!(others.contains(&"branch.push"));
    }

    #[test]
    fn catalog_lists_every_menu_action_even_without_a_shortcut() {
        let actions = [
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
        ];
        for action in actions {
            assert!(
                catalog()
                    .iter()
                    .any(|command| command.menu_action == Some(action)),
                "{action:?} is missing from the shortcut list"
            );
        }
        let bindings = ShortcutBindings::default();
        assert!(bindings.effective("repository.add_group").is_none());
        assert!(bindings.effective("local.discard").is_none());
        assert!(bindings.effective("branch.fast_forward").is_none());
        let mut bindings = bindings;
        let shortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::J);
        bindings.assign("local.discard", Some(shortcut));
        assert_eq!(bindings.menu_action(Discard), Some(shortcut));
    }

    #[test]
    fn menu_dispatch_uses_every_catalog_menu_action() {
        let dispatched: Vec<_> = ShortcutBindings::menu_actions_with_shortcuts().collect();
        for command in catalog() {
            if let Some(action) = command.menu_action {
                assert!(dispatched.contains(&action), "{}", command.id);
            }
        }
    }

    #[test]
    fn parse_shortcut_text_accepts_common_labels() {
        assert_eq!(
            parse_shortcut_text("Ctrl+Shift+O"),
            Some(KeyboardShortcut::new(
                Modifiers::CTRL.plus(Modifiers::SHIFT),
                Key::O
            ))
        );
        assert_eq!(
            parse_shortcut_text("Alt+PageDown"),
            Some(KeyboardShortcut::new(Modifiers::ALT, Key::PageDown))
        );
    }
}
