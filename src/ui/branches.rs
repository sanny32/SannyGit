use super::controls::dialog_style::DialogResponse;
use super::controls::icons::{self, Icon};
use super::controls::popup_menu;
use super::controls::section_header;
use super::controls::tooltips::TooltipResponse;
use super::controls::tree_indent;
use super::controls::tree_step;
use crate::{
    app::{ref_management::BranchAction, SannyGitApp},
    models::{BranchRef, RefKind},
    theme::{semibold_family, Palette},
};
use eframe::egui::{
    self, text::LayoutJob, Align, FontSelection, Layout, RichText, ScrollArea, TextEdit, Ui, Vec2,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum BranchRequest {
    Switch(String),
    Rename(String),
    Delete(String),
    DeleteTag(String),
    PushTag(String),
    PushTagTo(String),
    MergeTag(String),
    LogRef(String),
    ResetRef(String),
    ResetRefAdvanced(String),
    CopyRefName(String),
    CopyCommitMessage(String),
    CopyCommitId(String),
    CopyTagMessage(String),
    FormatTagPatch(String),
    MergeBranch(String),
    RebaseBranch(String),
    FastForward(String),
    PushLocal(String, bool),
    SetTracking(String),
    StopTracking(String),
    PushRemoteBranch(String),
    DeleteRemoteBranch(String),
    Unavailable,
}

#[derive(Default)]
struct BranchRequests {
    action: Option<BranchRequest>,
    rename_allowed: bool,
    tag_add_allowed: bool,
    tag_actions_allowed: bool,
    tag_merge_allowed: bool,
    tag_log_allowed: bool,
    tag_patch_allowed: bool,
    ref_reset_allowed: bool,
    has_remote_refs: bool,
}

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.background_progress_only {
        create_dialog(ui, app);
        rename_dialog(ui, app);
        tag_dialog(ui, app);
        remote_dialog(ui, app);
    }
    let palette = app.palette();
    let header = ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), super::WORKSPACE_HEADER_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            let heading = ui.heading(RichText::new(crate::i18n::tr_msg("Branches")).color(
                if app.branches.error.is_some() {
                    palette.removed_text
                } else {
                    palette.text
                },
            ));
            if app.branches.error.is_some() {
                heading.help_text(crate::i18n::tr_msg("Reload branches to retry."));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(!app.busy(), |ui| {
                    if icons::button(ui, Icon::Plus, "Create local branch from HEAD").clicked() {
                        app.branches.branch_dialog_open = true;
                    }
                });
                ui.add_enabled_ui(
                    !app.blocks_reads() && app.repositories.get(app.selected_repository).is_some(),
                    |ui| {
                        if icons::button(ui, Icon::Fetch, "Reload branches and tags").clicked() {
                            app.reload_refs();
                        }
                    },
                );
            });
        },
    );
    super::header_separator(
        ui,
        header.response.rect.expand2(Vec2::new(8.0, 0.0)),
        palette,
    );
    let mut requested = branch_requests(app);
    let mut add_tag_requested = false;
    let repository = app
        .repositories
        .get(app.selected_repository)
        .map(|repository| repository.path.clone())
        .unwrap_or_default();
    let height = ui.available_height();
    ScrollArea::vertical()
        .id_salt("branches")
        .max_height(height)
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // An empty, not-yet-loaded snapshot must not look like a tree
            // containing only tags. Keep existing rows on later refreshes.
            if !app.branches.current
                && app.branches.entries.is_empty()
                && app.repositories.get(app.selected_repository).is_some()
            {
                return;
            }
            // The open state is stored on the header id. This scope makes that
            // id include the repository path, so each repository keeps its own
            // expanded and collapsed groups across switches and restarts.
            ui.push_id(&repository, |ui| {
                local_folder_groups(
                    ui,
                    &app.branches.entries,
                    palette,
                    &mut app.branches.selected_branch,
                    &mut app.branches.selected_ref_group,
                    &mut requested,
                    &app.preferences.shortcuts,
                );
                for kind in [RefKind::Local, RefKind::Remote, RefKind::Tag] {
                    add_tag_requested |= branch_group(
                        ui,
                        &app.branches.entries,
                        &mut app.branches.selected_branch,
                        &mut app.branches.selected_ref_group,
                        kind,
                        palette,
                        &mut requested,
                        &app.preferences.shortcuts,
                    );
                }
            });
        });
    if let Some(request) = requested.action {
        dispatch_branch_request(ui.ctx(), app, request);
    }
    if add_tag_requested {
        app.branches.tag_dialog_open = true;
        app.error = None;
    }
}

fn branch_requests(app: &SannyGitApp) -> BranchRequests {
    let refs_current = !app.busy() && app.state_current && app.branches.current;
    let tag_actions_allowed =
        refs_current && app.repositories.get(app.selected_repository).is_some();
    BranchRequests {
        rename_allowed: refs_current,
        tag_add_allowed: !app.busy(),
        tag_actions_allowed,
        tag_merge_allowed: app.merge_allowed(),
        tag_log_allowed: !app.blocks_reads()
            && app.repositories.get(app.selected_repository).is_some(),
        tag_patch_allowed: tag_actions_allowed && !app.tag_patch_pending(),
        ref_reset_allowed: app.ref_reset_allowed(),
        has_remote_refs: app
            .branches
            .entries
            .iter()
            .any(|entry| entry.kind == RefKind::Remote),
        ..Default::default()
    }
}

fn dispatch_branch_request(ctx: &egui::Context, app: &mut SannyGitApp, request: BranchRequest) {
    app.branches.selected_ref_group = None;
    match request {
        BranchRequest::Switch(target) => {
            app.branches.selected_branch = target;
            if let Some(reason) = app.switch_branch_blocker() {
                app.error = Some(reason.into());
            } else {
                app.switch_branch();
            }
        }
        BranchRequest::Delete(target) => {
            app.branches.selected_branch = target;
            app.open_delete_branch();
        }
        BranchRequest::DeleteTag(target) => {
            app.branches.selected_branch = target;
            app.open_delete_tag();
        }
        BranchRequest::PushTag(target) => {
            app.branches.selected_branch = target;
            app.push_selected_tag();
        }
        BranchRequest::PushTagTo(target) => {
            app.branches.selected_branch = target;
            app.open_push_tag();
        }
        BranchRequest::MergeTag(target) => {
            app.branches.selected_branch = target.clone();
            app.open_merge_tag(target);
        }
        BranchRequest::LogRef(target) => {
            app.branches.selected_branch = target.clone();
            app.open_ref_log(target);
            super::journal::focus_history(ctx);
        }
        BranchRequest::ResetRef(target) => {
            app.branches.selected_branch = target.clone();
            app.open_ref_reset(target);
        }
        BranchRequest::ResetRefAdvanced(target) => {
            app.branches.selected_branch = target.clone();
            app.open_advanced_ref_reset(target);
        }
        BranchRequest::MergeBranch(target) => {
            app.branches.selected_branch = target;
            app.open_merge();
        }
        BranchRequest::RebaseBranch(target) => {
            app.branches.selected_branch = target;
            app.open_rebase();
        }
        BranchRequest::FastForward(target) => app.open_fast_forward(target),
        BranchRequest::PushLocal(target, choose_destination) => app.open_branch_action(
            target,
            BranchAction::PushLocal {
                remote: String::new(),
                custom: false,
                destination: String::new(),
                choose_destination,
            },
        ),
        BranchRequest::SetTracking(target) => app.open_branch_action(
            target,
            BranchAction::SetTracking {
                upstream: String::new(),
            },
        ),
        BranchRequest::StopTracking(target) => {
            app.open_branch_action(target, BranchAction::StopTracking)
        }
        BranchRequest::PushRemoteBranch(target) => app.open_branch_action(
            target,
            BranchAction::Push {
                custom: false,
                destination: String::new(),
            },
        ),
        BranchRequest::DeleteRemoteBranch(target) => {
            app.open_branch_action(target, BranchAction::Delete { also_remote: false })
        }
        BranchRequest::Rename(target) => {
            app.branches.selected_branch = target;
            app.open_rename_branch();
        }
        BranchRequest::CopyRefName(name) => ctx.copy_text(name),
        BranchRequest::CopyCommitMessage(hash) => app.copy_commit_message(ctx, hash),
        BranchRequest::CopyCommitId(hash) => ctx.copy_text(hash),
        BranchRequest::CopyTagMessage(oid) => app.copy_tag_message(ctx, oid),
        BranchRequest::FormatTagPatch(oid) => {
            // The click frame still paints the menu. Wait until a later frame has
            // presented it closed, then open the modal directory dialog.
            app.branches.pending_tag_patch = Some(crate::app::ref_management::PendingTagPatch {
                oid,
                menu_painted_closed: false,
            });
            ctx.request_repaint();
        }
        BranchRequest::Unavailable => {}
    }
}

pub(super) fn pick_pending_tag_patch(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(pending) = &mut app.branches.pending_tag_patch else {
        return;
    };
    if !pending.menu_painted_closed {
        pending.menu_painted_closed = true;
        ctx.request_repaint();
        return;
    }
    let Some(pending) = app.branches.pending_tag_patch.take() else {
        return;
    };
    egui::Popup::close_all(ctx);
    let mut picker = rfd::FileDialog::new()
        .set_title(crate::i18n::tr_msg("Select Directory"))
        .set_can_create_directories(true);
    // Own the picker by the window that opened it. rfd then keeps it modal
    // on Windows, macOS, and Linux (X11 and Wayland).
    if let Some(parent) = eframe::viewport_winit_window(ctx, ctx.viewport_id())
        .or_else(|| eframe::viewport_winit_window(ctx, egui::ViewportId::ROOT))
    {
        picker = picker.set_parent(&*parent);
    }
    if let Some(directory) = picker.pick_folder() {
        app.format_tag_patch(ctx, pending.oid, directory);
    }
}

#[cfg(any(not(target_os = "macos"), test))]
pub(crate) fn branch_shortcut_handles(
    app: &SannyGitApp,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> bool {
    if app.branches.selected_ref_group.is_some() {
        return false;
    }
    let Some(branch) = app.branches.entries.iter().find(|branch| {
        branch.kind != RefKind::Tag && branch.full_name == app.branches.selected_branch
    }) else {
        return false;
    };
    branch_menu(branch, &branch_requests(app), &app.preferences.shortcuts)
        .iter()
        .any(|entry| match entry {
            popup_menu::Entry::Item {
                enabled: true,
                shortcut: Some(shortcut),
                ..
            } => shortcut.logical_key == key && modifiers.matches_exact(shortcut.modifiers),
            _ => false,
        })
}

pub(crate) fn handle_branch_shortcuts(ctx: &egui::Context, app: &mut SannyGitApp) {
    if app.branches.selected_ref_group.is_some()
        || app.modal_dialog_open()
        || app.history_window().commit_details_open
        || ctx.text_edit_focused()
        || ctx.memory(|memory| memory.top_modal_layer().is_some())
        || egui::Popup::is_any_open(ctx)
    {
        return;
    }
    let Some(branch) = app.branches.entries.iter().find(|branch| {
        branch.kind != RefKind::Tag && branch.full_name == app.branches.selected_branch
    }) else {
        return;
    };
    let request = ctx.input_mut(|input| {
        if !input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key { pressed: true, .. } | egui::Event::Copy
            )
        }) {
            return None;
        }
        let mut request = None;
        let entries = branch_menu(branch, &branch_requests(app), &app.preferences.shortcuts);
        input.events.retain(|event| {
            let Some((action, enabled)) = entries.iter().find_map(|entry| {
                let popup_menu::Entry::Item {
                    action,
                    enabled,
                    shortcut: Some(shortcut),
                    ..
                } = entry
                else {
                    return None;
                };
                let matches = match event {
                    egui::Event::Key {
                        key,
                        modifiers,
                        pressed: true,
                        ..
                    } => {
                        shortcut.logical_key == *key && modifiers.matches_exact(shortcut.modifiers)
                    }
                    egui::Event::Copy => {
                        matches!(action, BranchRequest::CopyRefName(_))
                            && crate::shortcuts::chords_equivalent(
                                *shortcut,
                                egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C),
                            )
                    }
                    _ => false,
                };
                matches.then_some((action, *enabled))
            }) else {
                return true;
            };
            if enabled
                && request.is_none()
                && !matches!(event, egui::Event::Key { repeat: true, .. })
            {
                request = Some(action.clone());
            }
            false
        });
        request
    });
    if let Some(request) = request {
        dispatch_branch_request(ctx, app, request);
    }
}

#[allow(clippy::too_many_arguments)]
fn branch_group(
    ui: &mut Ui,
    refs: &[BranchRef],
    selected: &mut String,
    selected_group: &mut Option<RefKind>,
    kind: RefKind,
    palette: Palette,
    requested: &mut BranchRequests,
    shortcuts: &crate::shortcuts::ShortcutBindings,
) -> bool {
    let title = match kind {
        RefKind::Local => "Local Branches",
        RefKind::Remote => "Remote Branches",
        RefKind::Tag => "Tags",
    };
    let localized_title = crate::i18n::tr_msg(title);
    let matching: Vec<_> = refs.iter().filter(|r| r.kind == kind).collect();
    if matching.is_empty() && kind != RefKind::Tag {
        return false;
    }
    // Folder groups are top-level sections, so this header keeps only the
    // branches that sit directly under refs/heads.
    let tree = (kind == RefKind::Local).then(|| LocalTree::build(matching.iter().copied()));
    let shown = tree
        .as_ref()
        .map_or(matching.len(), |tree| tree.leaves.len());
    if kind == RefKind::Local && shown == 0 {
        return false;
    }
    let mut add_tag_requested = false;
    let previous_visuals = ui.visuals().clone();
    let fill = if *selected_group == Some(kind) {
        palette.selection
    } else {
        eframe::egui::Color32::TRANSPARENT
    };
    ui.visuals_mut().collapsing_header_frame = true;
    let widgets = &mut ui.visuals_mut().widgets;
    for visuals in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        visuals.weak_bg_fill = fill;
        visuals.bg_stroke = eframe::egui::Stroke::NONE;
        visuals.corner_radius = eframe::egui::CornerRadius::same(0);
    }
    let body_visuals = previous_visuals.clone();
    let group = section_header(
        RichText::new(crate::i18n::tr_args(
            "%{title} (%{count})",
            &[("title", &localized_title), ("count", &shown.to_string())],
        ))
        .text_style(eframe::egui::TextStyle::Body),
    )
    .id_salt(title)
    .default_open(kind == RefKind::Local)
    .show_unindented(ui, |ui| {
        *ui.visuals_mut() = body_visuals;
        let indent = tree_step(ui);
        if let Some(tree) = &tree {
            for branch in &tree.leaves {
                branch_row(
                    ui,
                    branch,
                    &branch.name,
                    RefKind::Local,
                    indent,
                    palette,
                    selected,
                    selected_group,
                    requested,
                    shortcuts,
                );
            }
        } else {
            for branch in matching {
                branch_row(
                    ui,
                    branch,
                    &branch.name,
                    kind,
                    indent,
                    palette,
                    selected,
                    selected_group,
                    requested,
                    shortcuts,
                );
            }
        }
    });
    *ui.visuals_mut() = previous_visuals;
    if group.header_response.clicked() || group.header_response.secondary_clicked() {
        *selected_group = Some(kind);
        selected.clear();
    }
    if kind == RefKind::Tag {
        add_tag_requested = popup_menu::context(&group.header_response, palette, || {
            vec![popup_menu::Entry::item(
                crate::i18n::tr_msg("Add Tag…"),
                requested.tag_add_allowed,
                (),
            )
            .help(crate::i18n::tr_msg(
                "Wait for the current operation to finish",
            ))]
        })
        .is_some();
    }
    add_tag_requested
}

/// Slash-separated local feature branches, nested by path segment.
#[derive(Default)]
struct LocalTree<'a> {
    leaves: Vec<&'a BranchRef>,
    children: Vec<(String, LocalTree<'a>)>,
}

/// Only feature folders form their own sections; SmartGit keeps other
/// slash-prefixed locals such as `release/1.x` under **Local Branches**.
fn is_group_folder(segment: &str) -> bool {
    segment.eq_ignore_ascii_case("feature") || segment.eq_ignore_ascii_case("features")
}

impl<'a> LocalTree<'a> {
    fn build(branches: impl IntoIterator<Item = &'a BranchRef>) -> Self {
        let mut tree = Self::default();
        for branch in branches {
            let segments: Vec<_> = branch.name.split('/').collect();
            // The last segment is the branch name; earlier ones are folders.
            let Some((_, folders)) = segments.split_last() else {
                continue;
            };
            if !folders
                .first()
                .is_some_and(|folder| is_group_folder(folder))
            {
                tree.leaves.push(branch);
                continue;
            }
            tree.insert(folders, branch);
        }
        tree
    }

    fn insert(&mut self, path: &[&str], branch: &'a BranchRef) {
        let Some((segment, rest)) = path.split_first() else {
            self.leaves.push(branch);
            return;
        };
        let index = match self
            .children
            .binary_search_by(|(name, _)| name.as_str().cmp(segment))
        {
            Ok(index) => index,
            Err(index) => {
                self.children
                    .insert(index, ((*segment).to_owned(), Self::default()));
                index
            }
        };
        self.children[index].1.insert(rest, branch);
    }

    fn count(&self) -> usize {
        self.leaves.len()
            + self
                .children
                .iter()
                .map(|(_, child)| child.count())
                .sum::<usize>()
    }
}

/// Turn a path segment into a SmartGit-style group title: `feature` → `Features`.
fn group_title(segment: &str) -> String {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut title: String = first.to_uppercase().chain(chars).collect();
    let lower = segment.to_ascii_lowercase();
    let Some(last) = lower.chars().last() else {
        return title;
    };
    if !last.is_ascii_alphabetic() || lower.ends_with('s') {
        return title;
    }
    if lower.ends_with('x')
        || lower.ends_with('z')
        || lower.ends_with("ch")
        || lower.ends_with("sh")
    {
        title.push_str("es");
    } else {
        title.push('s');
    }
    title
}

/// Top-level folder groups such as `Features`, siblings of **Local Branches**.
#[allow(clippy::too_many_arguments)]
fn local_folder_groups(
    ui: &mut Ui,
    refs: &[BranchRef],
    palette: Palette,
    selected: &mut String,
    selected_group: &mut Option<RefKind>,
    requested: &mut BranchRequests,
    shortcuts: &crate::shortcuts::ShortcutBindings,
) {
    let tree = LocalTree::build(refs.iter().filter(|entry| entry.kind == RefKind::Local));
    for (segment, child) in &tree.children {
        local_group(
            ui,
            segment,
            child,
            segment,
            palette,
            selected,
            selected_group,
            requested,
            shortcuts,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn local_tree(
    ui: &mut Ui,
    tree: &LocalTree<'_>,
    path: &str,
    palette: Palette,
    selected: &mut String,
    selected_group: &mut Option<RefKind>,
    requested: &mut BranchRequests,
    shortcuts: &crate::shortcuts::ShortcutBindings,
) {
    for branch in &tree.leaves {
        let name = branch.name.rsplit('/').next().unwrap_or(&branch.name);
        branch_row(
            ui,
            branch,
            name,
            RefKind::Local,
            tree_step(ui),
            palette,
            selected,
            selected_group,
            requested,
            shortcuts,
        );
    }
    for (segment, child) in &tree.children {
        let path = if path.is_empty() {
            segment.clone()
        } else {
            format!("{path}/{segment}")
        };
        tree_indent(ui, ("local-branch-group", &path), |ui| {
            local_group(
                ui,
                segment,
                child,
                &path,
                palette,
                selected,
                selected_group,
                requested,
                shortcuts,
            );
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn local_group(
    ui: &mut Ui,
    segment: &str,
    child: &LocalTree<'_>,
    path: &str,
    palette: Palette,
    selected: &mut String,
    selected_group: &mut Option<RefKind>,
    requested: &mut BranchRequests,
    shortcuts: &crate::shortcuts::ShortcutBindings,
) {
    section_header(
        RichText::new(format!("{} ({})", group_title(segment), child.count()))
            .text_style(egui::TextStyle::Body),
    )
    .id_salt(("local-branch-group", path))
    .default_open(true)
    .show_unindented(ui, |ui| {
        local_tree(
            ui,
            child,
            path,
            palette,
            selected,
            selected_group,
            requested,
            shortcuts,
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn branch_row(
    ui: &mut Ui,
    branch: &BranchRef,
    display_name: &str,
    kind: RefKind,
    indent: f32,
    palette: Palette,
    selected: &mut String,
    selected_group: &mut Option<RefKind>,
    requested: &mut BranchRequests,
    shortcuts: &crate::shortcuts::ShortcutBindings,
) {
    // Avoid text layout for rows outside the viewport in large ref lists.
    let size = Vec2::new(ui.available_width(), 22.0);
    let rect = eframe::egui::Rect::from_min_size(ui.next_widget_position(), size);
    if !ui.is_rect_visible(rect) {
        ui.allocate_space(size);
        return;
    }
    ui.push_id(&branch.full_name, |ui| {
        let response = super::controls::selectable_row_with_outset(
            ui,
            selected_group.is_none() && *selected == branch.full_name,
            22.0,
            8.0,
            eframe::egui::Sense::click(),
            |ui| branch_row_contents(ui, branch, display_name, kind, indent, palette),
        )
        .help_text(branch_tooltip(branch, kind));
        if response.clicked() || response.secondary_clicked() {
            *selected_group = None;
            *selected = branch.full_name.clone();
        }
        response.widget_info(|| {
            eframe::egui::WidgetInfo::selected(
                eframe::egui::Role::Button,
                true,
                selected_group.is_none() && *selected == branch.full_name,
                display_name,
            )
        });
        if response.double_clicked() {
            requested.action = Some(BranchRequest::Switch(branch.full_name.clone()));
        }
        if let Some(action) = popup_menu::context(&response, palette, || match kind {
            RefKind::Tag => tag_menu(branch, requested),
            RefKind::Local | RefKind::Remote => branch_menu(branch, requested, shortcuts),
        }) {
            requested.action = Some(action);
        }
    });
}

fn branch_row_contents(
    ui: &mut Ui,
    branch: &BranchRef,
    display_name: &str,
    kind: RefKind,
    indent: f32,
    palette: Palette,
) {
    ui.add_space(ui.spacing().indent + indent);
    icons::paint(
        ui,
        if kind == RefKind::Tag {
            Icon::Tag
        } else {
            Icon::Branch
        },
        palette.text,
        14.0,
    );
    let name = RichText::new(display_name).text_style(egui::TextStyle::Body);
    let name = if branch.current {
        name.family(semibold_family())
    } else {
        name
    };
    let mut text = LayoutJob::default();
    name.append_to(&mut text, ui.style(), FontSelection::Default, Align::Center);
    if let Some(subject) = &branch.tag_subject {
        RichText::new(format!(" - {subject}"))
            .text_style(egui::TextStyle::Body)
            .color(palette.muted.lerp_to_gamma(palette.text, 0.25))
            .append_to(&mut text, ui.style(), FontSelection::Default, Align::Center);
    }
    if branch.upstream_gone {
        RichText::new(crate::i18n::tr_msg(" ?"))
            .text_style(egui::TextStyle::Body)
            .color(palette.removed_text)
            .append_to(&mut text, ui.style(), FontSelection::Default, Align::Center);
    }
    if !branch.upstream_gone {
        if let Some(delta) = branch.upstream_delta {
            let sign = match delta {
                (0, 0) => " =",
                (0, _) => " <",
                (_, 0) => " >",
                _ => " ↕",
            };
            RichText::new(sign).color(palette.muted).append_to(
                &mut text,
                ui.style(),
                FontSelection::Default,
                Align::Center,
            );
        }
    }
    if let Some(remote) = branch.upstream_remote.as_ref().or(branch.upstream.as_ref()) {
        let color = if branch.upstream_gone {
            palette.removed_text
        } else {
            palette.muted.lerp_to_gamma(palette.accent, 0.5)
        };
        RichText::new(format!(" {remote}"))
            .text_style(egui::TextStyle::Body)
            .color(color)
            .append_to(&mut text, ui.style(), FontSelection::Default, Align::Center);
    }
    // Reserve the current-branch star before the name claims the row width.
    let star = if branch.current { 22.0 } else { 0.0 };
    let delta = match branch.upstream_delta {
        None | Some((0, 0)) => String::new(),
        Some((ahead, 0)) => format!("+{ahead}"),
        Some((0, behind)) => format!("-{behind}"),
        Some((ahead, behind)) => format!("+{ahead} -{behind}"),
    };
    let badge = ui.painter().layout_no_wrap(
        delta,
        egui::TextStyle::Body.resolve(ui.style()),
        palette.muted,
    );
    let available = ui.available_width() - badge.size().x;
    ui.scope(|ui| {
        ui.set_max_width((available - star).max(0.0));
        ui.add(
            egui::Label::new(egui::WidgetText::from(text))
                .truncate()
                .show_tooltip_when_elided(false),
        );
    });
    if branch.current || !badge.text().is_empty() {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if branch.current {
                icons::paint(ui, Icon::Star, palette.accent, 14.0);
            }
            if !badge.text().is_empty() {
                ui.label(badge);
            }
        });
    }
}

fn branch_tooltip(branch: &BranchRef, kind: RefKind) -> String {
    let upstream = branch
        .upstream
        .as_ref()
        .map(|upstream| {
            let state = if let Some((ahead, behind)) = branch.upstream_delta {
                format!(" (+{ahead} / -{behind})")
            } else if !branch.upstream_gone {
                String::new()
            } else if let Some(remote) = &branch.upstream_remote {
                crate::i18n::tr_args(" (gone on %{remote})", &[("remote", remote)])
            } else {
                crate::i18n::tr_msg(" (gone)")
            };
            crate::i18n::tr_args(
                "\nUpstream: %{upstream}%{state}",
                &[("upstream", upstream), ("state", &state)],
            )
        })
        .unwrap_or_default();
    crate::i18n::tr_args(
        "%{name}\nObject: %{object}%{upstream}%{subject}%{action}",
        &[
            ("name", &branch.full_name),
            ("object", &branch.hash),
            ("upstream", &upstream),
            (
                "subject",
                &branch
                    .tag_subject
                    .as_ref()
                    .map(|subject| crate::i18n::tr_args("\n%{subject}", &[("subject", subject)]))
                    .unwrap_or_default(),
            ),
            (
                "action",
                &crate::i18n::tr_msg(match kind {
                    RefKind::Local => "\nDouble-click to switch",
                    RefKind::Remote => {
                        "\nDouble-click to switch to a local branch or choose checkout options"
                    }
                    RefKind::Tag => "\nDouble-click to check out a detached HEAD",
                }),
            ),
        ],
    )
}

fn branch_menu(
    branch: &BranchRef,
    requested: &BranchRequests,
    shortcuts: &crate::shortcuts::ShortcutBindings,
) -> Vec<popup_menu::Entry<BranchRequest>> {
    use crate::shortcuts::BranchShortcut;
    use popup_menu::Entry as E;

    let target = || branch.full_name.clone();
    let bind = |kind: BranchShortcut| shortcuts.branch(kind);
    let refresh = "Refresh the repository and wait for the current operation";
    let needs_head = "An existing HEAD and fresh repository data are required";
    let local = branch.kind == RefKind::Local;
    let other_branch = !branch.current;
    let mut entries = vec![
        E::item(
            "Check Out…",
            requested.rename_allowed && other_branch,
            BranchRequest::Switch(target()),
        )
        .help("Select a branch that is not checked out and refresh the repository")
        .maybe_shortcut(bind(BranchShortcut::CheckOut)),
        E::Separator,
        E::item(
            "Merge…",
            requested.tag_merge_allowed && other_branch,
            BranchRequest::MergeBranch(target()),
        )
        .help(needs_head)
        .maybe_shortcut(bind(BranchShortcut::Merge)),
        E::item(
            "Rebase…",
            requested.tag_merge_allowed && other_branch,
            BranchRequest::RebaseBranch(target()),
        )
        .help(needs_head)
        .maybe_shortcut(bind(BranchShortcut::RebaseHeadTo)),
    ];
    if local {
        entries.push(
            E::item(
                "Fast-Forward Merge",
                requested.tag_merge_allowed && other_branch,
                BranchRequest::FastForward(target()),
            )
            .help("Select another branch while a current HEAD is available")
            .maybe_shortcut(bind(BranchShortcut::FastForward)),
        );
    }
    entries.push(E::Separator);
    entries.extend([
        E::item(
            "Push",
            local && requested.rename_allowed,
            if local {
                BranchRequest::PushLocal(target(), false)
            } else {
                BranchRequest::Unavailable
            },
        )
        .help(if local {
            refresh
        } else {
            "Direct push is available for local branches"
        })
        .maybe_shortcut(bind(BranchShortcut::Push)),
        E::item(
            "Push To…",
            requested.rename_allowed,
            if local {
                BranchRequest::PushLocal(target(), true)
            } else {
                BranchRequest::PushRemoteBranch(target())
            },
        )
        .help(refresh)
        .maybe_shortcut(bind(BranchShortcut::PushTo)),
        E::Separator,
        E::item(
            "Log",
            requested.tag_log_allowed,
            BranchRequest::LogRef(target()),
        )
        .help("Wait for the current operation to finish")
        .maybe_shortcut(bind(BranchShortcut::Log)),
        E::Separator,
        E::item(
            "Reset…",
            requested.ref_reset_allowed,
            BranchRequest::ResetRef(target()),
        )
        .help(needs_head)
        .maybe_shortcut(bind(BranchShortcut::Reset)),
        E::item(
            "Reset Advanced…",
            requested.ref_reset_allowed,
            BranchRequest::ResetRefAdvanced(target()),
        )
        .help(needs_head)
        .maybe_shortcut(bind(BranchShortcut::ResetAdvanced)),
    ]);
    entries.push(E::Separator);
    if local {
        entries.push(
            E::item(
                "Rename…",
                requested.rename_allowed,
                BranchRequest::Rename(target()),
            )
            .help(refresh)
            .maybe_shortcut(bind(BranchShortcut::Rename)),
        );
    }
    entries.push(
        E::item(
            "Delete…",
            requested.rename_allowed && other_branch,
            if local {
                BranchRequest::Delete(target())
            } else {
                BranchRequest::DeleteRemoteBranch(target())
            },
        )
        .help("Select a branch that is not checked out and refresh the repository"),
    );
    if local {
        entries.extend([
            E::Separator,
            E::item(
                "Set Tracked Branch…",
                requested.rename_allowed && requested.has_remote_refs,
                BranchRequest::SetTracking(target()),
            )
            .help("Fetch remote branches and refresh the repository")
            .maybe_shortcut(bind(BranchShortcut::SetTracking)),
            E::item(
                "Stop Tracking…",
                requested.rename_allowed && branch.upstream.is_some(),
                BranchRequest::StopTracking(target()),
            )
            .help("The selected branch must have a tracked branch")
            .maybe_shortcut(bind(BranchShortcut::StopTracking)),
        ]);
    }
    entries.extend([
        E::Separator,
        E::item(
            "Copy",
            true,
            BranchRequest::CopyRefName(branch.name.clone()),
        )
        .maybe_shortcut(bind(BranchShortcut::Copy)),
    ]);
    if local {
        let id_ready = crate::branches::is_commit_id(&branch.hash);
        entries.extend([
            E::item(
                "Copy Message",
                id_ready,
                BranchRequest::CopyCommitMessage(branch.hash.clone()),
            )
            .help("Refresh branches to load the full commit id")
            .maybe_shortcut(bind(BranchShortcut::CopyMessage)),
            E::item(
                "Copy ID",
                id_ready,
                BranchRequest::CopyCommitId(branch.hash.clone()),
            )
            .help("Refresh branches to load the full commit id")
            .maybe_shortcut(bind(BranchShortcut::CopyId)),
        ]);
    }
    entries
}

fn tag_menu(tag: &BranchRef, requested: &BranchRequests) -> Vec<popup_menu::Entry<BranchRequest>> {
    use popup_menu::Entry as E;

    vec![
        E::item(
            "Check Out…",
            requested.tag_actions_allowed,
            BranchRequest::Switch(tag.full_name.clone()),
        )
        .help("Refresh the repository and tags before continuing"),
        E::Separator,
        E::item(
            "Merge…",
            requested.tag_merge_allowed,
            BranchRequest::MergeTag(tag.full_name.clone()),
        )
        .help(
            "Merge requires an existing HEAD, fresh repository data and no operation in progress",
        ),
        E::Separator,
        E::item(
            "Push",
            requested.tag_actions_allowed,
            BranchRequest::PushTag(tag.full_name.clone()),
        )
        .help("Refresh the repository and wait for the current operation"),
        E::item(
            "Push To…",
            requested.tag_actions_allowed,
            BranchRequest::PushTagTo(tag.full_name.clone()),
        )
        .help("Refresh the repository and wait for the current operation"),
        E::Separator,
        E::item(
            "Log",
            requested.tag_log_allowed,
            BranchRequest::LogRef(tag.full_name.clone()),
        )
        .help("Wait for the current operation to finish"),
        E::Separator,
        E::item(
            "Reset…",
            requested.ref_reset_allowed,
            BranchRequest::ResetRef(tag.full_name.clone()),
        )
        .help(
            "Reset requires an existing HEAD, fresh repository data and no operation in progress",
        ),
        E::item(
            "Reset Advanced…",
            requested.ref_reset_allowed,
            BranchRequest::ResetRefAdvanced(tag.full_name.clone()),
        )
        .help(
            "Reset requires an existing HEAD, fresh repository data and no operation in progress",
        ),
        E::item(
            "Delete…",
            requested.tag_actions_allowed,
            BranchRequest::DeleteTag(tag.full_name.clone()),
        )
        .help("Refresh the repository and wait for the current operation"),
        E::Separator,
        E::item("Copy", true, BranchRequest::CopyRefName(tag.name.clone())),
        if tag.tag_subject.is_some() {
            E::item(
                "Copy Message",
                true,
                BranchRequest::CopyTagMessage(tag.hash.clone()),
            )
        } else {
            E::item("Copy Message", false, BranchRequest::Unavailable)
                .help("This tag has no message")
        },
        E::Separator,
        E::item(
            "Format Patch",
            requested.tag_patch_allowed,
            BranchRequest::FormatTagPatch(tag.hash.clone()),
        )
        .help("Refresh the repository and wait for the current operation"),
        E::item(
            "Fast-Forward Merge",
            requested.tag_merge_allowed,
            BranchRequest::FastForward(tag.full_name.clone()),
        )
        .help("Move HEAD to the tagged commit when it is a descendant of HEAD"),
    ]
}

fn tag_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.branches.tag_dialog_open {
        return;
    }
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let tag_names: Vec<_> = app
        .branches
        .entries
        .iter()
        .filter(|reference| reference.kind == RefKind::Tag)
        .map(|reference| reference.name.clone())
        .collect();
    let mut keep = true;
    let busy = app.busy();
    super::controls::dialog_style::modal(&ctx, palette, "branches.add.tag").show(&ctx, |ui| {
        ui.set_width(500.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header(
            ui,
            palette,
            "Add tag to current branch (HEAD)",
            "Enter the name of the tag to create. If entering a message, an annotated tag is created.",
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::same(18),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                        ui.label(crate::i18n::tr_msg("Tag:"));
                        let combo_width = ui.available_width();
                        super::controls::editable_combobox::show(
                            ui,
                            ui.make_persistent_id("tag.name"),
                            &mut app.branches.new_tag_name,
                            &tag_names,
                            combo_width,
                            palette,
                            "No existing tags",
                        );
                    });
                ui.label(crate::i18n::tr_msg("Annotated Tag Message:"));
                let message = ui.add_sized(
                    [ui.available_width(), 120.0],
                    TextEdit::multiline(&mut app.branches.new_tag_message)
                        .margin(eframe::egui::Margin::symmetric(6, 4)),
                );
                super::controls::dialog_style::preserve_multiline_enter(&message);
                });
            },
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::symmetric(18, 12),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    let blocker = app.create_tag_blocker();
                    if ui.add_enabled(blocker.is_none(), super::controls::dialog_style::button("Add Tag & Push"))
                        .disabled_help_text(crate::i18n::tr_blocker(blocker))
                        .on_hover_text(crate::i18n::tr_msg("Create the tag and push it to the current remote"))
                        .clicked() {
                        app.create_tag(true);
                    }
                    if ui.add_enabled(blocker.is_none(), super::controls::dialog_style::primary("Add Tag", palette))
                        .disabled_help_text(crate::i18n::tr_blocker(blocker)).dialog_accept() {
                        app.create_tag(false);
                    }
                    if ui.add_enabled(!busy, super::controls::dialog_style::button("Cancel")).dialog_cancel() {
                        keep = false;
                    }
                });
            },
        );
    });
    if !keep {
        app.branches.tag_dialog_open = false;
        app.branches.new_tag_name.clear();
        app.branches.new_tag_message.clear();
    }
}

fn remote_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    let Some(mut dialog) = app.branches.remote_checkout.take() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let mut keep = true;
    super::controls::dialog_style::modal(&ctx, palette, "branch.remote.checkout").show(&ctx, |ui| {
        ui.set_width(600.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header(ui, palette, "Check out commit", "Switch to a local branch, create a tracking branch, or check out without creating a branch.");
        super::controls::dialog_style::section(ui, palette.header, eframe::egui::Margin::same(18), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(dialog.remote.strip_prefix("refs/remotes/").unwrap_or(&dialog.remote));
            ui.add_enabled_ui(!app.busy(), |ui| {
                eframe::egui::Grid::new("remote.checkout.options").min_col_width(155.0).spacing([12.0, 10.0]).show(ui, |ui| {
                    ui.radio_value(
                    &mut dialog.mode,
                    0,
                    crate::i18n::tr_msg("Checkout local branch:"),
                );
                    ui.add_enabled_ui(dialog.mode == 0, |ui| {
                        eframe::egui::ComboBox::from_id_salt("remote.local").width(280.0)
                            .selected_text(dialog.local.strip_prefix("refs/heads/").unwrap_or(&dialog.local))
                            .show_ui(ui, |ui| { for branch in app.branches.entries.iter().filter(|r| r.kind == RefKind::Local) { ui.selectable_value(&mut dialog.local, branch.full_name.clone(), &branch.name); } });
                    });
                    ui.end_row();
                    ui.radio_value(
                        &mut dialog.mode,
                        1,
                        crate::i18n::tr_msg("Create local branch:"),
                    );
                    ui.add_enabled(dialog.mode == 1, crate::ui::controls::singleline(&mut dialog.new_name).desired_width(280.0));
                    ui.end_row();
                });
                ui.radio_value(
                    &mut dialog.mode,
                    2,
                    crate::i18n::tr_msg("Don't create local branch (detached HEAD)"),
                );
            });
        });
        super::controls::dialog_style::section(ui, palette.header, eframe::egui::Margin::symmetric(18, 12), |ui| {
            super::controls::dialog_style::button_row(ui, |ui| {
                let valid = match dialog.mode {
                    0 => app.branches.entries.iter().any(|r| r.kind == RefKind::Local && r.full_name == dialog.local),
                    1 => crate::branches::name_hint(&dialog.new_name).is_none() && !app.branches.entries.iter().any(|r| r.full_name == format!("refs/heads/{}", dialog.new_name)),
                    _ => true,
                };
                if ui.add_enabled(valid && !app.busy(), super::controls::dialog_style::primary("Checkout", palette)).dialog_accept() {
                    let choice = match dialog.mode { 0 => crate::branches::RemoteChoice::Local(dialog.local.clone()), 1 => crate::branches::RemoteChoice::Create(dialog.new_name.clone()), _ => crate::branches::RemoteChoice::Detached };
                    app.checkout_remote(dialog.remote.clone(), choice);
                }
                if ui.add_enabled(!app.busy(), super::controls::dialog_style::button("Cancel")).dialog_cancel() { keep = false; }
            });
        });
    });
    if keep {
        app.branches.remote_checkout = Some(dialog);
    }
}

fn rename_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    if app.branches.rename_branch_dialog.is_none() {
        return;
    }
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let mut cancel = false;
    super::controls::dialog_style::modal(&ctx, palette, "branches.rename").show(&ctx, |ui| {
        ui.set_width(440.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header_with_icon(
            ui,
            palette,
            "Rename local branch",
            "Choose a new local name. The remote branch keeps its name.",
            Icon::Branch,
        );
        let busy = app.busy();
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::same(18),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if let Some(dialog) = &mut app.branches.rename_branch_dialog {
                    ui.add(
                        eframe::egui::Label::new(
                            dialog
                                .source
                                .strip_prefix("refs/heads/")
                                .unwrap_or(&dialog.source),
                        )
                        .truncate()
                        .show_tooltip_when_elided(false),
                    )
                    .help_text(&dialog.source);
                    ui.add_enabled(
                        !busy,
                        crate::ui::controls::singleline(&mut dialog.name)
                            .desired_width(f32::INFINITY),
                    );
                }
                let blocker = app.rename_branch_blocker();
                let status = crate::i18n::tr_or(blocker, "Ready to rename");
                ui.add(
                    eframe::egui::Label::new(RichText::new(&status).small().color(palette.muted))
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .help_text(status);
            },
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::symmetric(18, 12),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    let blocker = app.rename_branch_blocker();
                    if ui
                        .add_enabled(
                            blocker.is_none(),
                            super::controls::dialog_style::primary("Rename", palette),
                        )
                        .disabled_help_text(crate::i18n::tr_blocker(blocker))
                        .dialog_accept()
                    {
                        app.rename_branch();
                    }
                    if ui
                        .add_enabled(!busy, super::controls::dialog_style::button("Cancel"))
                        .dialog_cancel()
                    {
                        cancel = true;
                    }
                });
            },
        );
    });
    if cancel {
        app.branches.rename_branch_dialog = None;
    }
}

fn create_dialog(ui: &mut Ui, app: &mut SannyGitApp) {
    if !app.branches.branch_dialog_open {
        return;
    }
    let ctx = ui.ctx().clone();
    let palette = app.palette();
    let busy = app.busy();
    let mut cancel = false;
    super::controls::dialog_style::modal(&ctx, palette, "branches.create").show(&ctx, |ui| {
        ui.set_width(440.0);
        super::controls::dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        super::controls::dialog_style::header_with_icon(
            ui,
            palette,
            "Create branch",
            "Create a local branch from the current HEAD.",
            Icon::Branch,
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::same(18),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let source = crate::i18n::tr_args(
                    "Create from HEAD (%{branch})",
                    &[("branch", &app.branches.current_branch)],
                );
                ui.add(
                    eframe::egui::Label::new(&source)
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .help_text(&source);
                ui.add_enabled_ui(!busy, |ui| {
                    ui.add(
                        crate::ui::controls::singleline(&mut app.branches.new_branch_name)
                            .hint_text("feature/my-change")
                            .desired_width(f32::INFINITY),
                    );
                    ui.checkbox(
                        &mut app.branches.switch_to_new_branch,
                        crate::i18n::tr_msg("Switch to the new branch"),
                    );
                });
                let blocker = app.create_branch_blocker();
                let status = crate::i18n::tr_or(blocker, "Ready to create");
                ui.add(
                    eframe::egui::Label::new(RichText::new(&status).small().color(palette.muted))
                        .truncate()
                        .show_tooltip_when_elided(false),
                )
                .help_text(status);
            },
        );
        super::controls::dialog_style::section(
            ui,
            palette.header,
            eframe::egui::Margin::symmetric(18, 12),
            |ui| {
                super::controls::dialog_style::button_row(ui, |ui| {
                    let blocker = app.create_branch_blocker();
                    if ui
                        .add_enabled(
                            blocker.is_none(),
                            super::controls::dialog_style::primary("Create", palette),
                        )
                        .disabled_help_text(crate::i18n::tr_blocker(blocker))
                        .dialog_accept()
                    {
                        app.create_branch();
                    }
                    if ui
                        .add_enabled(!busy, super::controls::dialog_style::button("Cancel"))
                        .dialog_cancel()
                    {
                        cancel = true;
                    }
                });
            },
        );
    });
    if cancel {
        app.branches.branch_dialog_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

    #[test]
    fn tag_dialog_keeps_geometry_during_validation_progress_and_errors() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 0;
                app.branches.entries = crate::mock_data::refs();
                app.state_current = true;
                app.branches.current = true;
                app.branches.branch_head_exists = true;
                app.branches.tag_dialog_open = true;
                let mut geometry = Vec::new();
                for stage in 0..4 {
                    app.branches.new_tag_name = if stage == 0 { "" } else { "feature/new" }.into();
                    app.branches.current = stage != 2;
                    app.error = (stage == 3).then(|| "Long error message. ".repeat(100));
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| tag_dialog(ui, &mut app),
                        );
                        output.textures_delta.clear();
                        if frame == 2 {
                            let rect = ctx
                                .memory(|memory| {
                                    memory.area_rect(eframe::egui::Id::unique("branches.add.tag"))
                                })
                                .unwrap();
                            assert!(
                                (rect.center() - (Pos2::ZERO + size * 0.5)).length() < 1.0,
                                "Tag dialog is off center: {rect:?}"
                            );
                            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
                            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, eframe::egui::Shape::Text(text) if text.galley.text() == "Add tag to current branch (HEAD)")));
                            geometry.push(rect);
                        }
                    }
                }
                for pair in geometry.windows(2) {
                    assert_eq!(pair[0], pair[1]);
                }
            }
        }
    }

    #[test]
    fn create_dialog_stays_centered_with_header_and_stable_status_line() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 0;
                app.branches.entries = crate::mock_data::refs();
                app.state_current = true;
                app.branches.current = true;
                app.branches.branch_head_exists = true;
                app.branches.branch_dialog_open = true;
                let mut geometry = Vec::new();
                for stage in 0..4 {
                    app.branches.new_branch_name =
                        if stage == 0 { "" } else { "feature/new" }.into();
                    app.branches.current = stage != 2;
                    app.error = (stage == 3).then(|| "Long error message. ".repeat(100));
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| create_dialog(ui, &mut app),
                        );
                        output.textures_delta.clear();
                        if frame == 2 {
                            let rect = ctx
                                .memory(|memory| {
                                    memory.area_rect(eframe::egui::Id::unique("branches.create"))
                                })
                                .unwrap();
                            assert!(
                                (rect.center() - (Pos2::ZERO + size * 0.5)).length() < 1.0,
                                "Create dialog is off center: {rect:?}"
                            );
                            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
                            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, eframe::egui::Shape::Text(text) if text.galley.text() == "Create branch")));
                            geometry.push(rect);
                        }
                    }
                }
                for pair in geometry.windows(2) {
                    assert_eq!(pair[0], pair[1]);
                }
            }
        }
    }

    #[test]
    fn rename_dialog_keeps_geometry_with_validation_progress_and_long_errors() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 0;
                app.state_current = true;
                app.branches.current = true;
                app.branches.entries = crate::mock_data::refs();
                app.branches.selected_branch = "refs/heads/develop".into();
                app.open_rename_branch();
                let mut geometry = Vec::new();
                for stage in 0..4 {
                    app.branches.rename_branch_dialog.as_mut().unwrap().name =
                        if stage == 0 { "develop" } else { "new-name" }.into();
                    app.branches.current = stage != 2;
                    app.error = (stage == 3).then(|| "A very long Git error. ".repeat(80));
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| rename_dialog(ui, &mut app),
                        );
                        output.textures_delta.clear();
                        if frame == 2 {
                            let rect = ctx
                                .memory(|memory| {
                                    memory.area_rect(eframe::egui::Id::unique("branches.rename"))
                                })
                                .unwrap();
                            assert!(
                                (rect.center() - (Pos2::ZERO + size * 0.5)).length() < 1.0,
                                "Rename dialog is off center: {rect:?}"
                            );
                            let buttons: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| {
                                    if let eframe::egui::Shape::Text(text) = &shape.shape {
                                        if matches!(text.galley.text(), "Rename" | "Cancel") {
                                            return Some((text.pos, text.galley.size()));
                                        }
                                    }
                                    None
                                })
                                .collect();
                            assert_eq!(buttons.len(), 2);
                            for (pos, extent) in &buttons {
                                assert!(Rect::from_min_size(Pos2::ZERO, size)
                                    .contains_rect(Rect::from_min_size(*pos, *extent)));
                            }
                            geometry.push(buttons);
                        }
                    }
                }
                for pair in geometry.windows(2) {
                    assert_eq!(pair[0], pair[1]);
                }
            }
        }
    }

    #[test]
    fn tag_dialog_content_stays_within_header_width() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.branches.tag_dialog_open = true;
                app.branches.new_tag_name = "v1.0.0-b1".into();
                for frame in 0..3 {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| tag_dialog(ui, &mut app),
                    );
                    output.textures_delta.clear();
                    if frame == 2 {
                        let header = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                eframe::egui::Shape::Rect(rect)
                                    if (rect.rect.width() - 500.0).abs() < 1.0
                                        && (60.0..150.0).contains(&rect.rect.height()) =>
                                {
                                    Some(rect.rect)
                                }
                                _ => None,
                            })
                            .expect("Dialog header missing");
                        for shape in &output.shapes {
                            if let eframe::egui::Shape::Rect(rect) = &shape.shape {
                                if rect.rect.width() > 300.0 && rect.rect.width() < size.x {
                                    assert!(rect.rect.right() <= header.right() + 2.0,
                                        "Dialog content expanded beyond header: header={header:?}, rect={:?}", rect.rect);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn initial_ref_load_never_shows_a_tags_only_tree() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.repositories = crate::mock_data::repositories();
                app.selected_repository = 1;
                for stage in 0..4 {
                    app.branches.loading = stage == 0;
                    app.branches.current = stage >= 2;
                    app.branches.entries = if stage == 2 {
                        crate::mock_data::refs()
                    } else {
                        Vec::new()
                    };
                    let mut labels = Vec::new();
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| {
                                ui.set_width(260.0);
                                show(ui, &mut app);
                            },
                        );
                        labels = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                                _ => None,
                            })
                            .collect();
                        output.textures_delta.clear();
                    }
                    let has = |label: &str| labels.iter().any(|text| text.starts_with(label));
                    assert!(!has(&crate::i18n::tr_msg("Loading…")));
                    assert_eq!(has(&crate::i18n::tr_msg("Tags")), stage >= 2);
                    assert_eq!(has(&crate::i18n::tr_msg("Local Branches")), stage == 2);
                    assert_eq!(has(&crate::i18n::tr_msg("Remote Branches")), stage == 2);
                }
            }
        }
    }

    #[test]
    fn background_ref_updates_do_not_move_branch_rows() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1400.0, 900.0)] {
                let ctx = Context::default();
                crate::theme::install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.branches.entries = vec![BranchRef {
                    name: "feature".into(),
                    full_name: "refs/heads/feature".into(),
                    hash: "a".repeat(40),
                    tag_subject: None,
                    upstream: None,
                    upstream_remote: None,
                    upstream_delta: None,
                    upstream_gone: false,
                    current: false,
                    kind: RefKind::Local,
                }];
                let mut positions = Vec::new();
                for stage in 0..3 {
                    app.branches.loading = stage == 1;
                    app.branches.error = (stage == 2).then(|| "Cannot load refs".into());
                    for frame in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| {
                                ui.set_width(260.0);
                                show(ui, &mut app);
                            },
                        );
                        if frame == 2 {
                            let position = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    eframe::egui::Shape::Text(text)
                                        if text.galley.job.text == "feature" =>
                                    {
                                        Some(text.pos)
                                    }
                                    _ => None,
                                })
                                .expect("Branch row missing");
                            positions.push(position);
                            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                                eframe::egui::Shape::Text(text) if text.galley.job.text == "Loading branches…")));
                        }
                        output.textures_delta.clear();
                    }
                }
                assert!(
                    positions
                        .windows(2)
                        .all(|pair| pair[0].distance(pair[1]) < 0.5),
                    "Branch rows moved during a background update: {positions:?}"
                );
            }
        }
    }

    #[test]
    fn branch_name_requires_double_click_to_switch() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            theme.apply(&ctx);
            let refs = vec![BranchRef {
                name: "feature".into(),
                full_name: "refs/heads/feature".into(),
                hash: "a".repeat(40),
                tag_subject: None,
                upstream: None,
                upstream_remote: None,
                upstream_delta: None,
                upstream_gone: false,
                current: false,
                kind: RefKind::Local,
            }];
            let mut selected = String::new();
            let mut selected_group = Some(RefKind::Remote);
            let mut requested = BranchRequests::default();
            let mut pointer = Pos2::ZERO;
            for (frame, pressed) in [None, None, Some(true), Some(false), Some(true), Some(false)]
                .into_iter()
                .enumerate()
            {
                let mut events = vec![Event::PointerMoved(pointer)];
                if let Some(pressed) = pressed {
                    events.push(Event::PointerButton {
                        pos: pointer,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
                        time: Some(frame as f64 * 0.05),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        pointer = ui.next_widget_position() + Vec2::new(70.0, 34.0);
                        let _ = branch_group(
                            ui,
                            &refs,
                            &mut selected,
                            &mut selected_group,
                            RefKind::Local,
                            theme.palette(),
                            &mut requested,
                            &crate::shortcuts::ShortcutBindings::default(),
                        );
                    },
                );
                output.textures_delta.clear();
                if frame == 3 {
                    assert_eq!(selected, "refs/heads/feature");
                    assert_eq!(selected_group, None);
                    assert!(requested.action.is_none());
                }
            }
            assert_eq!(
                requested.action,
                Some(BranchRequest::Switch("refs/heads/feature".into()))
            );
        }
    }

    #[test]
    fn local_branch_push_claims_configured_shortcut_and_a_remote_branch_does_not() {
        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.state_current = true;
        app.branches.current = true;
        app.branches.entries = vec![BranchRef {
            name: "main".into(),
            full_name: "refs/heads/main".into(),
            hash: "b".repeat(40),
            tag_subject: None,
            upstream: None,
            upstream_remote: None,
            upstream_delta: None,
            upstream_gone: false,
            current: true,
            kind: RefKind::Local,
        }];
        app.branches.selected_branch = "refs/heads/main".into();
        let shortcut = app
            .preferences
            .shortcuts
            .branch(crate::shortcuts::BranchShortcut::Push)
            .unwrap();
        assert!(super::branch_shortcut_handles(
            &app,
            shortcut.logical_key,
            shortcut.modifiers
        ));
        app.branches.entries[0].kind = RefKind::Remote;
        app.branches.entries[0].full_name = "refs/remotes/origin/main".into();
        app.branches.selected_branch = app.branches.entries[0].full_name.clone();
        assert!(!super::branch_shortcut_handles(
            &app,
            shortcut.logical_key,
            shortcut.modifiers
        ));
    }

    #[test]
    fn remote_branch_copy_shortcut_targets_only_the_selected_ref() {
        let ctx = Context::default();
        let mut app = SannyGitApp::with_theme(crate::theme::ThemeChoice::Light);
        app.branches.entries = vec![BranchRef {
            name: "origin/team/topic".into(),
            full_name: "refs/remotes/origin/team/topic".into(),
            hash: "a".repeat(40),
            tag_subject: None,
            upstream: None,
            upstream_remote: None,
            upstream_delta: None,
            upstream_gone: false,
            current: false,
            kind: RefKind::Remote,
        }];
        app.branches.selected_branch = app.branches.entries[0].full_name.clone();
        for (modifiers, repeat, selected_group, copies) in [
            (Modifiers::COMMAND, false, None, true),
            (Modifiers::COMMAND, true, None, false),
            (
                Modifiers::COMMAND.plus(Modifiers::SHIFT),
                false,
                None,
                false,
            ),
            (Modifiers::COMMAND, false, Some(RefKind::Remote), false),
        ] {
            app.branches.selected_ref_group = selected_group;
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                ui.input_mut(|input| {
                    input.events.push(Event::Key {
                        key: egui::Key::C,
                        physical_key: None,
                        pressed: true,
                        repeat,
                        modifiers,
                    });
                });
                handle_branch_shortcuts(ui.ctx(), &mut app);
            });
            output.textures_delta.clear();
            assert_eq!(
                output
                    .platform_output
                    .commands
                    .iter()
                    .any(|command| matches!(
                        command, egui::OutputCommand::CopyText(text) if text == "origin/team/topic"
                    )),
                copies,
            );
        }
    }

    #[test]
    fn secondary_click_selects_ref_before_dispatching_a_menu_action() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            for kind in [RefKind::Local, RefKind::Remote, RefKind::Tag] {
                let ctx = Context::default();
                theme.apply(&ctx);
                let prefix = match kind {
                    RefKind::Local => "refs/heads",
                    RefKind::Remote => "refs/remotes/origin",
                    RefKind::Tag => "refs/tags",
                };
                let reference = BranchRef {
                    name: "target".into(),
                    full_name: format!("{prefix}/target"),
                    hash: "a".repeat(40),
                    tag_subject: None,
                    upstream: None,
                    upstream_remote: None,
                    upstream_delta: None,
                    upstream_gone: false,
                    current: false,
                    kind,
                };
                let refs = [reference];
                let mut selected = String::new();
                let mut selected_group = Some(RefKind::Local);
                let mut requested = BranchRequests::default();
                let mut header = Pos2::ZERO;
                let mut row = Pos2::ZERO;
                for frame in 0..7 {
                    let pointer = if frame < 4 { header } else { row };
                    let mut events = vec![Event::PointerMoved(pointer)];
                    let button = match frame {
                        1 | 2 if kind != RefKind::Local => Some(PointerButton::Primary),
                        4 | 5 => Some(PointerButton::Secondary),
                        _ => None,
                    };
                    if let Some(button) = button {
                        events.push(Event::PointerButton {
                            pos: pointer,
                            button,
                            pressed: frame == 1 || frame == 4,
                            modifiers: Modifiers::NONE,
                        });
                    }
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(1100.0, 700.0),
                            )),
                            time: Some(frame as f64 * 0.25),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            header = ui.next_widget_position() + Vec2::new(70.0, 11.0);
                            row = header + Vec2::new(0.0, 23.0);
                            branch_group(
                                ui,
                                &refs,
                                &mut selected,
                                &mut selected_group,
                                kind,
                                theme.palette(),
                                &mut requested,
                                &crate::shortcuts::ShortcutBindings::default(),
                            );
                        },
                    );
                    output.textures_delta.clear();
                    if frame == 6 {
                        let items: &[&str] = if kind == RefKind::Tag {
                            &["Check Out…", "Push To…", "Copy", "Fast-Forward Merge"]
                        } else if kind == RefKind::Local {
                            &[
                                "Check Out…",
                                "Merge…",
                                "Rebase…",
                                "Fast-Forward Merge",
                                "Push",
                                "Push To…",
                                "Log",
                                "Reset…",
                                "Reset Advanced…",
                                "Rename…",
                                "Delete…",
                                "Set Tracked Branch…",
                                "Stop Tracking…",
                                "Copy",
                                "Copy Message",
                                "Copy ID",
                            ]
                        } else {
                            &[
                                "Check Out…",
                                "Merge…",
                                "Rebase…",
                                "Push",
                                "Push To…",
                                "Log",
                                "Reset…",
                                "Reset Advanced…",
                                "Delete…",
                                "Copy",
                            ]
                        };
                        for &item in items {
                            assert!(
                                output.shapes.iter().any(|shape| matches!(
                                    &shape.shape,
                                    eframe::egui::Shape::Text(text) if text.galley.text() == item
                                )),
                                "Missing context menu item {item}"
                            );
                        }
                        if kind == RefKind::Remote {
                            for item in ["Copy Message", "Copy ID"] {
                                assert!(
                                    !output.shapes.iter().any(|shape| matches!(
                                        &shape.shape,
                                        eframe::egui::Shape::Text(text) if text.galley.text() == item
                                    )),
                                    "Remote branches do not copy a commit {item}"
                                );
                            }
                        }
                    }
                }
                assert_eq!(selected, refs[0].full_name);
                assert_eq!(selected_group, None);
                assert!(requested.action.is_none());
            }
        }
    }

    #[test]
    fn group_mouse_click_selects_group_instead_of_branch() {
        for button in [PointerButton::Primary, PointerButton::Secondary] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            let refs = vec![BranchRef {
                name: "feature".into(),
                full_name: "refs/heads/feature".into(),
                hash: "a".repeat(40),
                tag_subject: None,
                upstream: None,
                upstream_remote: None,
                upstream_delta: None,
                upstream_gone: false,
                current: false,
                kind: RefKind::Local,
            }];
            let mut selected = "refs/heads/feature".to_string();
            let mut selected_group = None;
            let mut requested = BranchRequests::default();
            let mut pointer = Pos2::ZERO;
            for (frame, pressed) in [None, None, Some(true), Some(false)]
                .into_iter()
                .enumerate()
            {
                let mut events = vec![Event::PointerMoved(pointer)];
                if let Some(pressed) = pressed {
                    events.push(Event::PointerButton {
                        pos: pointer,
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
                        time: Some(frame as f64 * 0.05),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        pointer = ui.next_widget_position() + Vec2::new(70.0, 11.0);
                        let _ = branch_group(
                            ui,
                            &refs,
                            &mut selected,
                            &mut selected_group,
                            RefKind::Local,
                            crate::theme::ThemeChoice::Light.palette(),
                            &mut requested,
                            &crate::shortcuts::ShortcutBindings::default(),
                        );
                    },
                );
                output.textures_delta.clear();
            }
            assert_eq!(selected_group, Some(RefKind::Local));
            assert!(selected.is_empty());
            assert!(requested.action.is_none());
        }
    }

    #[test]
    fn selected_group_keeps_standard_header_geometry() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            let refs = vec![BranchRef {
                name: "feature".into(),
                full_name: "refs/heads/feature".into(),
                hash: "a".repeat(40),
                tag_subject: None,
                upstream: None,
                upstream_remote: None,
                upstream_delta: None,
                upstream_gone: false,
                current: false,
                kind: RefKind::Local,
            }];
            let render = |selected_header: bool| {
                let mut selected = String::new();
                let mut selected_group = selected_header.then_some(RefKind::Local);
                let mut requested = BranchRequests::default();
                let mut result = None;
                for frame in 0..3 {
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(400.0, 200.0),
                            )),
                            time: Some(frame as f64 * 0.05),
                            ..Default::default()
                        },
                        |ui| {
                            if selected_header {
                                let _ = branch_group(
                                    ui,
                                    &refs,
                                    &mut selected,
                                    &mut selected_group,
                                    RefKind::Local,
                                    theme.palette(),
                                    &mut requested,
                                    &crate::shortcuts::ShortcutBindings::default(),
                                );
                            } else {
                                let _ = section_header(
                                    RichText::new(crate::i18n::tr_msg("Local Branches (1)"))
                                        .text_style(eframe::egui::TextStyle::Body),
                                )
                                .id_salt("Local Branches")
                                .default_open(true)
                                .show_unindented(ui, |ui| {
                                    let _ = crate::ui::controls::selectable_row(
                                        ui,
                                        false,
                                        22.0,
                                        |ui| {
                                            ui.label(crate::i18n::tr_msg("feature"));
                                        },
                                    );
                                });
                            }
                        },
                    );
                    output.textures_delta.clear();
                    result = Some(output);
                }
                result.unwrap()
            };
            let geometry = |output: &eframe::egui::FullOutput| {
                let mut title = None;
                let mut branch_y = None;
                let mut arrow = None;
                for shape in &output.shapes {
                    match &shape.shape {
                        eframe::egui::Shape::Text(text)
                            if text.galley.job.text == "Local Branches (1)" =>
                        {
                            title = Some(text.galley.rect.translate(text.pos.to_vec2()));
                        }
                        eframe::egui::Shape::Text(text) if text.galley.job.text == "feature" => {
                            branch_y = Some(text.pos.y);
                        }
                        eframe::egui::Shape::Mesh(mesh) => {
                            arrow = Some(mesh.calc_bounds());
                        }
                        _ => {}
                    }
                }
                (
                    title.expect("Group title missing"),
                    branch_y.expect("Branch row missing"),
                    arrow.expect("Group arrow missing"),
                )
            };
            assert_eq!(geometry(&render(false)), geometry(&render(true)));
        }
    }

    fn branch_ref(name: &str, kind: RefKind) -> BranchRef {
        let full_name = match kind {
            RefKind::Local => format!("refs/heads/{name}"),
            RefKind::Remote => format!("refs/remotes/{name}"),
            RefKind::Tag => format!("refs/tags/{name}"),
        };
        BranchRef {
            name: name.into(),
            full_name,
            hash: "a".repeat(40),
            tag_subject: None,
            upstream: None,
            upstream_remote: None,
            upstream_delta: None,
            upstream_gone: false,
            current: false,
            kind,
        }
    }

    #[test]
    fn local_tree_groups_only_feature_folders_and_counts_each_branch() {
        let refs = [
            branch_ref("main", RefKind::Local),
            branch_ref("release/1.x", RefKind::Local),
            branch_ref("feature/a", RefKind::Local),
            branch_ref("feature/ui/b", RefKind::Local),
        ];
        let tree = LocalTree::build(refs.iter());
        assert_eq!(tree.count(), 4);
        // Branches outside feature folders stay flat, including slashed names.
        assert_eq!(tree.leaves.len(), 2);
        assert_eq!(tree.leaves[1].name, "release/1.x");
        assert_eq!(tree.children.len(), 1);
        let (segment, feature) = &tree.children[0];
        assert_eq!(segment, "feature");
        assert_eq!(feature.count(), 2);
        assert_eq!(feature.leaves.len(), 1);
        assert_eq!(feature.children.len(), 1);
        assert_eq!(feature.children[0].0, "ui");
        assert_eq!(feature.children[0].1.count(), 1);
        assert!(is_group_folder("feature") && is_group_folder("Features"));
        assert!(!is_group_folder("release"));
        assert_eq!(group_title("feature"), "Features");
        assert_eq!(group_title("fix"), "Fixes");
        assert_eq!(group_title("1.0"), "1.0");
    }

    fn painted(output: &eframe::egui::FullOutput, label: &str) -> bool {
        output.shapes.iter().any(|shape| {
            let eframe::egui::Shape::Text(text) = &shape.shape else {
                return false;
            };
            let painted = text.galley.text();
            painted == label
                || painted
                    .strip_prefix(label)
                    .is_some_and(|rest| rest.starts_with(" ("))
        })
    }

    fn label_center(output: &eframe::egui::FullOutput, label: &str) -> Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| {
                let eframe::egui::Shape::Text(text) = &shape.shape else {
                    return None;
                };
                let painted = text.galley.text();
                let matches = painted == label
                    || painted
                        .strip_prefix(label)
                        .is_some_and(|rest| rest.starts_with(" ("));
                matches.then(|| text.galley.rect.translate(text.pos.to_vec2()).center())
            })
            .unwrap_or_else(|| panic!("missing {label}"))
    }

    struct BranchPanel {
        ctx: Context,
        app: SannyGitApp,
        time: f64,
    }

    impl BranchPanel {
        fn new(theme: crate::theme::ThemeChoice) -> Self {
            Self::with_refs(
                theme,
                vec![
                    branch_ref("main", RefKind::Local),
                    branch_ref("origin/main", RefKind::Remote),
                    branch_ref("v1", RefKind::Tag),
                ],
            )
        }

        fn with_refs(theme: crate::theme::ThemeChoice, entries: Vec<BranchRef>) -> Self {
            let ctx = Context::default();
            crate::theme::install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            app.repositories = vec![
                crate::models::Repository {
                    name: "alpha".into(),
                    path: "/repos/alpha".into(),
                },
                crate::models::Repository {
                    name: "beta".into(),
                    path: "/repos/beta".into(),
                },
            ];
            app.selected_repository = 0;
            app.branches.entries = entries;
            Self {
                ctx,
                app,
                time: 0.0,
            }
        }

        fn from_memory(theme: crate::theme::ThemeChoice, memory: eframe::egui::Memory) -> Self {
            let panel = Self::new(theme);
            panel.ctx.memory_mut(|stored| *stored = memory);
            crate::theme::install_fonts(&panel.ctx);
            egui_extras::install_image_loaders(&panel.ctx);
            theme.apply(&panel.ctx);
            panel
        }

        fn render(&mut self, events: Vec<Event>) -> eframe::egui::FullOutput {
            self.time += 0.05;
            let time = self.time;
            let mut output = self.ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(260.0);
                    show(ui, &mut self.app);
                },
            );
            output.textures_delta.clear();
            output
        }

        fn settle(&mut self) -> eframe::egui::FullOutput {
            self.time += 1.0;
            self.render(Vec::new());
            self.time += 1.0;
            self.render(Vec::new())
        }

        fn click(&mut self, label: &str) {
            let output = self.render(Vec::new());
            let pos = label_center(&output, label);
            self.render(vec![Event::PointerMoved(pos)]);
            self.render(vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }]);
            self.render(vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }]);
            self.settle();
        }
    }

    fn assert_groups(output: &eframe::egui::FullOutput, local: bool, remote: bool, tags: bool) {
        assert_eq!(painted(output, "main"), local, "local group: {local}");
        assert_eq!(
            painted(output, "origin/main"),
            remote,
            "remote group: {remote}"
        );
        assert_eq!(painted(output, "v1"), tags, "tag group: {tags}");
    }

    fn text_rect(output: &eframe::egui::FullOutput, label: &str) -> Option<Rect> {
        output.shapes.iter().find_map(|shape| {
            let eframe::egui::Shape::Text(text) = &shape.shape else {
                return None;
            };
            (text.galley.job.text == label).then(|| text.galley.rect.translate(text.pos.to_vec2()))
        })
    }

    #[test]
    fn local_branches_group_by_path_and_show_tracked_remote_with_missing_upstream() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let mut refs = vec![
                branch_ref("main", RefKind::Local),
                branch_ref("release/1.x", RefKind::Local),
                branch_ref("feature/login", RefKind::Local),
                branch_ref("feature/signup", RefKind::Local),
                branch_ref("origin/main", RefKind::Remote),
                branch_ref("v1", RefKind::Tag),
            ];
            refs[0].upstream_delta = Some((0, 46));
            refs[0].upstream = Some("origin/main".into());
            refs[0].upstream_remote = Some("origin".into());
            refs[1].upstream = Some("origin/release/1.x".into());
            refs[1].upstream_remote = Some("origin".into());
            refs[2].upstream = Some("origin/login".into());
            refs[2].upstream_remote = Some("origin".into());
            refs[2].upstream_gone = true;
            refs[3].upstream = Some("origin/signup".into());
            refs[3].upstream_remote = Some("origin".into());
            let mut panel = BranchPanel::with_refs(theme, refs);
            let output = panel.settle();
            let painted_texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    eframe::egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            for label in [
                "Features (2)",
                "Local Branches (2)",
                "Remote Branches (1)",
                "Tags (1)",
                "-46",
                "main < origin",
                "release/1.x origin",
                "login ? origin",
                "signup origin",
            ] {
                assert!(
                    painted(&output, label),
                    "missing {label}: {painted_texts:?}"
                );
            }
            assert!(
                !painted(&output, "Releases"),
                "Only feature folders become groups"
            );
            assert!(
                !output.shapes.iter().any(|shape| matches!(
                    &shape.shape,
                    eframe::egui::Shape::Text(text)
                        if text.galley.job.text == "main ? origin"
                )),
                "Only the missing upstream carries a question mark"
            );
            let login_x = text_rect(&output, "login ? origin").unwrap().left();
            let flat_x = text_rect(&output, "release/1.x origin").unwrap().left();
            let main_x = text_rect(&output, "main < origin").unwrap().left();
            let features_x = text_rect(&output, "Features (2)").unwrap().left();
            let locals_x = text_rect(&output, "Local Branches (2)").unwrap().left();
            assert_eq!(flat_x, main_x, "Slash-prefixed locals stay flat rows");
            assert!(
                (flat_x
                    - 18.0
                    - locals_x
                    - panel
                        .ctx
                        .style_of(if theme == crate::theme::ThemeChoice::Dark {
                            egui::Theme::Dark
                        } else {
                            egui::Theme::Light
                        })
                        .spacing
                        .indent
                    + 6.0)
                    .abs()
                    < 0.5,
                "Branch rows should use the shared tree step: header={locals_x}, row={flat_x}"
            );
            assert_eq!(
                features_x, locals_x,
                "Folder groups are siblings of Local Branches"
            );
            assert_eq!(
                login_x, flat_x,
                "Section children and folder children share one indent level"
            );
            assert!(
                login_x > features_x,
                "Branches inside a group indent past its header: header={features_x}, login={login_x}"
            );

            panel.click("Remote Branches (1)");
            panel.click("Tags (1)");
            let expanded = panel.settle();
            let remote_x = text_rect(&expanded, "origin/main").unwrap().left();
            let tag_x = text_rect(&expanded, "v1").unwrap().left();
            assert_eq!(remote_x, flat_x, "Remote rows share the section indent");
            assert_eq!(tag_x, flat_x, "Tag rows share the section indent");
            let remote_color = |label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Text(text) if text.galley.job.text == label => text
                            .galley
                            .job
                            .sections
                            .last()
                            .map(|section| section.format.color),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing {label}"))
            };
            assert_eq!(remote_color("login ? origin"), theme.palette().removed_text);
            assert_ne!(remote_color("signup origin"), theme.palette().removed_text);

            panel.click("Features (2)");
            let collapsed = panel.settle();
            assert!(!painted(&collapsed, "login ? origin"));
            assert!(painted(&collapsed, "Features (2)"));
            assert!(painted(&collapsed, "main < origin"));

            panel.click("Features (2)");
            let expanded = panel.settle();
            assert!(painted(&expanded, "login ? origin"));
            panel.click("login ? origin");
            panel.settle();
            assert_eq!(
                panel.app.branches.selected_branch,
                "refs/heads/feature/login"
            );
            assert_eq!(panel.app.branches.selected_ref_group, None);
        }
    }

    #[test]
    fn nested_branch_groups_use_one_tree_step_per_level() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let mut panel = BranchPanel::with_refs(
                theme,
                vec![
                    branch_ref("main", RefKind::Local),
                    branch_ref("feature/login", RefKind::Local),
                    branch_ref("feature/mobile/payments", RefKind::Local),
                ],
            );
            let output = panel.settle();
            let x = |label| text_rect(&output, label).unwrap().left();
            let step = panel
                .ctx
                .style_of(if theme == crate::theme::ThemeChoice::Dark {
                    egui::Theme::Dark
                } else {
                    egui::Theme::Light
                })
                .spacing
                .indent
                - 6.0;
            assert!((x("Mobiles (1)") - x("Features (2)") - step).abs() < 0.5);
            assert!((x("payments") - 18.0 - x("Mobiles (1)") - step).abs() < 0.5);
            assert!((x("payments") - x("login") - step).abs() < 0.5);
        }
    }
    #[test]
    fn branch_group_expansion_is_remembered_per_repository() {
        for theme in [
            crate::theme::ThemeChoice::Light,
            crate::theme::ThemeChoice::Dark,
        ] {
            let mut panel = BranchPanel::new(theme);
            assert_groups(&panel.settle(), true, false, false);

            panel.click("Local Branches");
            panel.click("Remote Branches");
            panel.click("Tags");
            assert_groups(&panel.settle(), false, true, true);

            panel.app.selected_repository = 1;
            assert_groups(&panel.settle(), true, false, false);
            panel.click("Local Branches");
            assert_groups(&panel.settle(), false, false, false);

            panel.app.selected_repository = 0;
            assert_groups(&panel.settle(), false, true, true);

            let memory = crate::test_support::restart_memory(&panel.ctx);
            let mut restored = BranchPanel::from_memory(theme, memory);
            assert_groups(&restored.settle(), false, true, true);
            restored.app.selected_repository = 1;
            assert_groups(&restored.settle(), false, false, false);
        }
    }
}
