use super::controls::icons::{self, Icon};
use super::controls::popup_menu;
use super::controls::tooltips::TooltipResponse;
use crate::{
    app::{ChangeFilter, RepositoryJob, SannyGitApp},
    models::ChangeKind,
    theme::{semibold_family, Palette},
};
use eframe::egui::{
    Align, Align2, Color32, Direction, FontId, Layout, Rect, RichText, Sense, Ui, Vec2,
};
use egui_extras::{Column, TableBuilder};
use std::path::{Path, PathBuf};

const FILTER_WIDTH: f32 = 366.0;
const MERGE_BANNER_HEIGHT: f32 = 36.0;
const INDEX_COLUMN_GUTTER: f32 = 6.0;

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    let palette = app.palette();
    ui.spacing_mut().item_spacing.y = 0.0;
    let tight = ui.available_width() < FILTER_WIDTH + 150.0;
    let filter_height = if tight { 50.0 } else { 30.0 };
    let mut view_action = None;
    super::header(ui, filter_height, palette, |ui| {
        if tight {
            ui.vertical(|ui| {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    changes_title(ui, app.changes.len());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        view_action = changes_view_button(ui, app);
                    });
                });
                ui.horizontal(|ui| filters(ui, app, palette));
            });
        } else {
            changes_title(ui, app.changes.len());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                view_action = changes_view_button(ui, app);
                ui.add_space(4.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(FILTER_WIDTH, 22.0),
                    Layout::left_to_right(Align::Center),
                    |ui| filters(ui, app, palette),
                );
            });
        }
    });
    if let Some(action) = view_action {
        apply_view_action(app, action);
    }
    if app.merging {
        let banner_palette = Palette {
            header: palette.warning_background,
            panel_radius: 0,
            ..palette
        };
        super::header(ui, MERGE_BANNER_HEIGHT, banner_palette, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        app.abort_merge_allowed(),
                        eframe::egui::Button::new("Abort"),
                    )
                    .help_text(app.sequencing.abort_help())
                    .disabled_help_text(
                        "Wait for the current operation or refresh the working tree",
                    )
                    .clicked()
                {
                    app.open_abort_merge();
                }
                if app.sequencing.supports_skip()
                    && ui
                        .add_enabled(
                            app.sequencer_skip_allowed(),
                            eframe::egui::Button::new("Skip"),
                        )
                        .help_text("Skip the current step after conflicts are resolved")
                        .disabled_help_text("Resolve conflicts and refresh the working tree")
                        .clicked()
                {
                    app.skip_sequence();
                }
                if matches!(
                    app.sequencing,
                    crate::git::Sequencing::CherryPick
                        | crate::git::Sequencing::Revert
                        | crate::git::Sequencing::Rebase
                ) && ui
                    .add_enabled(
                        app.sequencer_continue_allowed(),
                        eframe::egui::Button::new("Continue"),
                    )
                    .help_text("Continue after all conflicts are resolved")
                    .disabled_help_text("Resolve conflicts and refresh the working tree")
                    .clicked()
                {
                    app.continue_sequence();
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    icons::paint(ui, Icon::Warning, palette.warning, 16.0);
                    ui.add(eframe::egui::Label::new(app.sequencing.banner()).truncate());
                });
            });
        });
    }
    let operations_enabled =
        !app.busy() && app.state_current && app.repositories.get(app.selected_repository).is_some();
    let mut staging_request = None;
    let mut file_request = None;
    let sort_id = eframe::egui::Id::unique("files.status_sort");
    let index_sort_id = eframe::egui::Id::unique("files.index_sort");
    let status_sort = ui.data(|data| data.get_temp::<bool>(sort_id));
    let index_sort = ui.data(|data| data.get_temp::<bool>(index_sort_id));
    let display_order = sync_display_order(ui, app);
    let path_rank = |index: usize| {
        display_order
            .iter()
            .position(|path| path == &app.changes[index].path)
            .unwrap_or(usize::MAX)
    };
    let mut visible = app
        .changes
        .iter()
        .enumerate()
        .filter(|(_, change)| row_visible(app, change))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if let Some(ascending) = index_sort {
        visible.sort_by(|&left, &right| {
            let order = app.changes[left].staged.cmp(&app.changes[right].staged);
            let order = if ascending { order } else { order.reverse() };
            order.then_with(|| path_rank(left).cmp(&path_rank(right)))
        });
    } else if let Some(ascending) = status_sort {
        visible.sort_by(|&left, &right| {
            let order = app.changes[left]
                .kind
                .code()
                .cmp(app.changes[right].kind.code());
            let order = if ascending { order } else { order.reverse() };
            order.then_with(|| path_rank(left).cmp(&path_rank(right)))
        });
    } else {
        visible.sort_by_key(|&index| path_rank(index));
    }
    let focus_id = eframe::egui::Id::unique("files.selection.focus");
    // Keep list keyboard ownership separate from the transient row widget IDs.
    if ui.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_none_or(|position| !ui.max_rect().contains(position))
    }) || ui.input(|input| input.key_pressed(eframe::egui::Key::Tab))
    {
        ui.data_mut(|data| data.insert_temp(focus_id, false));
    }
    let focused = ui
        .data(|data| data.get_temp::<bool>(focus_id))
        .unwrap_or(false);
    let reveal = keyboard_selection(ui, app, &visible, focused && !ui.ctx().text_edit_focused());
    let table_rows = change_rows(app, &visible);
    if (focused || ui.rect_contains_pointer(ui.max_rect()))
        && !ui.ctx().text_edit_focused()
        && ui.input_mut(|input| {
            input.consume_key(eframe::egui::Modifiers::COMMAND, eframe::egui::Key::A)
        })
    {
        app.selection.paths = visible
            .iter()
            .map(|&i| app.changes[i].path.clone())
            .collect();
        if !visible.contains(&app.selection.active) {
            app.selection.active = visible.first().copied().unwrap_or(usize::MAX);
        }
    }
    let row_height = 23.0;
    let click_id = focus_id.with("handled_click");
    let frame = ui.ctx().cumulative_frame_nr();
    let mut click_handled = ui.data(|data| data.get_temp::<u64>(click_id)) == Some(frame);
    let table_clip = ui.clip_rect().intersect(ui.available_rect_before_wrap());
    // Table row backgrounds expand into cell spacing; keep them inside the panel.
    ui.shrink_clip_rect(table_clip);
    ui.spacing_mut().item_spacing.x = 4.0;
    let widths_id = eframe::egui::Id::unique("files.header_column_widths");
    let [mut index_width, mut status_width] = ui
        .data_mut(|data| data.get_persisted::<[f32; 2]>(widths_id))
        .unwrap_or([52.0, 72.0]);
    let mut table = TableBuilder::new(ui)
        .id_salt("files.columns.v3")
        .resizable(false)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(index_width))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::exact(status_width))
        .min_scrolled_height(0.0);
    if let Some(position) = reveal {
        if let Some(file) = visible.get(position) {
            if let Some(row) = table_rows
                .iter()
                .position(|row| matches!(row, ChangeRow::File(index) if index == file))
            {
                table = table.scroll_to_row(row, None);
            }
        }
    }
    table
        .header(24.0, |mut header| {
            header.col(|ui| {
                ui.add_space(INDEX_COLUMN_GUTTER);
                let label = match index_sort {
                    None => "Index",
                    Some(true) => "Index ↑",
                    Some(false) => "Index ↓",
                };
                if ui
                    .add(eframe::egui::Button::new(label).frame(false))
                    .help_text("Sort by staged state")
                    .clicked()
                {
                    ui.data_mut(|data| {
                        data.remove::<bool>(sort_id);
                        data.insert_temp(index_sort_id, index_sort != Some(true));
                    });
                    ui.ctx().request_repaint();
                }
                column_separator(ui, "index", &mut index_width, 1.0, 44.0..=80.0, palette);
            });
            header.col(|ui| {
                ui.label("File");
                column_separator(ui, "file", &mut status_width, -1.0, 64.0..=120.0, palette);
            });
            header.col(|ui| {
                let label = match status_sort {
                    None => "Status",
                    Some(true) => "Status ↑",
                    Some(false) => "Status ↓",
                };
                if ui.add(eframe::egui::Button::new(label).frame(false))
                    .help_text("Sort by file status")
                    .clicked()
                {
                    ui.data_mut(|data| {
                        data.remove::<bool>(index_sort_id);
                        data.insert_temp(sort_id, status_sort != Some(true));
                    });
                    ui.ctx().request_repaint();
                }
            });
        })
        .body(|mut body| {
            body.ui_mut().style_mut().interaction.selectable_labels = false;
            body.rows(row_height, table_rows.len(), |mut row| {
                if let ChangeRow::Directory { label, files } = &table_rows[row.index()] {
                    let files = files.clone();
                    let label = label.clone();
                    row.set_selected(false);
                    let mut row_rect = Rect::NOTHING;
                    row.col(|ui| {
                        row_rect = ui.max_rect();
                        ui.add_space(INDEX_COLUMN_GUTTER);
                    });
                    row.col(|ui| {
                        icons::paint(ui, Icon::Folder, palette.muted, 16.0);
                        ui.add_space(4.0);
                        ui.add(
                            eframe::egui::Label::new(
                                RichText::new(label).family(semibold_family()).color(palette.text),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        row_rect = row_rect.union(ui.max_rect());
                    });
                    let response = row.response();
                    ui_line(&response, row_rect, table_clip, palette);
                    if response.clicked() && !click_handled {
                        app.selection.paths = files
                            .iter()
                            .map(|&index| app.changes[index].path.clone())
                            .collect();
                        app.selection.active = files.first().copied().unwrap_or(usize::MAX);
                        app.selection.anchor = app
                            .changes
                            .get(app.selection.active)
                            .map(|change| change.path.clone());
                        app.diff_line_selection.clear();
                        response.ctx.data_mut(|data| data.insert_temp(focus_id, true));
                        response
                            .ctx
                            .data_mut(|data| data.insert_temp(click_id, frame));
                        click_handled = true;
                    }
                    return;
                }
                let ChangeRow::File(index) = table_rows[row.index()] else {
                    return;
                };
                let change = &app.changes[index];
                let conflicted = change.kind == ChangeKind::Conflicted;
                let toggle_staged = !change.staged;
                let can_toggle = app.staging_allowed(index, toggle_staged) && !conflicted;
                row.set_selected(app.change_is_selected(index));
                let mut row_rect = Rect::NOTHING;
                let mut index_toggle = None;
                row.col(|ui| {
                    row_rect = ui.max_rect();
                    ui.spacing_mut().interact_size.y = row_height;
                    let mut checked = change.staged;
                    let tooltip = if conflicted {
                        "Double-click the file to open Conflict Solver"
                    } else if !operations_enabled {
                        "Wait for the current operation or refresh the repository"
                    } else if change.staged && app.merging {
                        "Use Discard to restore the pre-merge version of this file"
                    } else if change.staged && change.unstaged {
                        "Some changes are staged. Click to unstage; use Stage to include the remaining changes"
                    } else if change.staged { "Unstage this file (preserve working-tree contents)" }
                    else { "Stage all changes in this file" };
                    let response = ui
                        .allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), row_height),
                            Layout::centered_and_justified(Direction::LeftToRight),
                            |ui| {
                                ui.add_enabled(
                                    can_toggle,
                                    eframe::egui::Checkbox::without_text(&mut checked),
                                )
                            },
                        )
                        .inner
                        .help_text(tooltip)
                        .disabled_help_text(tooltip);
                    if response.changed() {
                        staging_request = Some((index, !change.staged));
                    }
                    index_toggle = Some(response);
                });
                row.col(|ui| {
                    icons::paint(ui, Icon::for_file(&change.path), status_color(change.kind, palette), 16.0);
                    ui.add_space(4.0);
                    let mut name = RichText::new(&change.path).color(palette.text);
                    if change.kind == ChangeKind::Deleted {
                        name = name.strikethrough();
                    }
                    ui.add(eframe::egui::Label::new(name).truncate().show_tooltip_when_elided(false))
                        .help_text(if let Some(source) = &change.original_path {
                            format!("{source} → {}", change.path)
                        } else {
                            change.path.clone()
                        });
                });
                row.col(|ui| {
                    row_rect = row_rect.union(ui.max_rect());
                    let color = status_color(change.kind, palette);
                    let fill = match change.kind {
                        ChangeKind::Modified | ChangeKind::Renamed => palette.modified_background,
                        ChangeKind::Added | ChangeKind::Untracked => palette.status_added,
                        ChangeKind::Ignored => palette.border,
                        ChangeKind::Deleted | ChangeKind::Conflicted => palette.status_removed,
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(19.0, 18.0), Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        change.kind.code(),
                        FontId::new(crate::theme::ui_font_size(), semibold_family()),
                        color,
                    );
                    response.help_text(change.kind.label());
                });
                let response = row.response();
                ui_line(&response, row_rect, table_clip, palette);
                let index_clicked = index_toggle.as_ref().is_some_and(|toggle| {
                    toggle.clicked() || toggle.changed()
                });
                if response.clicked() && !click_handled && !index_clicked {
                    let modifiers = response.ctx.input(|input| input.modifiers);
                    app.select_change(index, modifiers.command, modifiers.shift, &visible);
                    response.ctx.data_mut(|data| data.insert_temp(focus_id, true));
                    response.ctx.data_mut(|data| data.insert_temp(click_id, frame));
                    click_handled = true;
                } else if response.secondary_clicked() && !click_handled {
                    if app.change_is_selected(index) { app.selection.active = index; }
                    else { app.select_change(index, false, false, &visible); }
                    response.ctx.data_mut(|data| data.insert_temp(focus_id, true));
                    response.ctx.data_mut(|data| data.insert_temp(click_id, frame));
                    click_handled = true;
                }
                if let Some(action) = popup_menu::context(&response, palette, || file_context_menu(app, index)) {
                    file_request = Some((index, action));
                }
                if response.double_clicked() {
                    if conflicted {
                        file_request = Some((index, FileAction::Conflict(crate::conflict::Action::Edit)));
                    } else if can_toggle
                        && app.preferences.file_double_click == crate::preferences::FileDoubleClick::ToggleStage
                    {
                        staging_request = Some((index, toggle_staged));
                    }
                }
            });
        });
    ui.data_mut(|data| data.insert_persisted(widths_id, [index_width, status_width]));
    if let Some((index, staged)) = staging_request {
        if !app.change_is_selected(index) {
            app.select_change(index, false, false, &visible);
        }
        if app.selected_change_indices().len() > 1 {
            app.set_selected_staged(staged);
        } else {
            app.set_staged(index, staged);
        }
    }
    if let Some((index, action)) = file_request {
        app.selection.active = index;
        match action {
            FileAction::ShowChanges => app.select_change(index, false, false, &visible),
            FileAction::LogFile(path) => {
                app.open_file_log(path);
                super::journal::focus_history(ui.ctx());
            }
            FileAction::Commit => ui.ctx().memory_mut(|memory| {
                memory.request_focus(eframe::egui::Id::unique("commit.message"));
            }),
            FileAction::Open(path) => app.run_repository_job(RepositoryJob::OpenFile(path)),
            FileAction::Reveal(path) => app.run_repository_job(RepositoryJob::RevealFile(path)),
            FileAction::Ignore => app.open_ignore_dialog(),
            FileAction::SelectDirectory | FileAction::SelectRepository => {
                let change = &app.changes[index];
                let directory = Path::new(&change.path).parent();
                app.selection.anchor = Some(change.path.clone());
                app.selection.paths = visible
                    .iter()
                    .map(|&index| &app.changes[index].path)
                    .filter(|path| {
                        matches!(action, FileAction::SelectRepository)
                            || directory.is_some_and(|dir| Path::new(path).starts_with(dir))
                    })
                    .cloned()
                    .collect();
            }
            FileAction::Copy(text) => ui.ctx().copy_text(text),
            FileAction::Stage => app.set_selected_staged(true),
            FileAction::Unstage => app.set_selected_staged(false),
            FileAction::Discard => app.open_discard(),
            FileAction::Remove => app.open_remove(),
            FileAction::StashSelection => app.open_stash_selection(),
            FileAction::Rename => app.open_rename_dialog(),
            FileAction::Conflict(action) => app.open_conflict(index, action),
        }
    }
}

fn keyboard_selection(
    ui: &mut Ui,
    app: &mut SannyGitApp,
    visible: &[usize],
    focused: bool,
) -> Option<usize> {
    use eframe::egui::{Event, Key};
    if !focused
        || eframe::egui::Popup::is_any_open(ui.ctx())
        || ui.memory(|memory| memory.top_modal_layer().is_some())
    {
        return None;
    }
    let keys = ui.input_mut(|input| {
        let mut keys = Vec::new();
        input.events.retain(|event| {
            if let Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if !modifiers.alt
                    && !modifiers.command
                    && matches!(key, Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End)
                {
                    keys.push((*key, modifiers.shift));
                    return false;
                }
            }
            true
        });
        keys
    });
    let mut reveal = None;
    for (key, extend) in keys {
        let active = visible
            .iter()
            .position(|&index| index == app.selection.active);
        let last = visible.len().saturating_sub(1);
        let position = match key {
            Key::ArrowUp => active
                .map(|position| position.saturating_sub(1))
                .unwrap_or(last),
            Key::ArrowDown => active.map(|position| (position + 1).min(last)).unwrap_or(0),
            Key::End => last,
            _ => 0,
        };
        if let Some(&index) = visible.get(position) {
            app.select_change(index, false, extend, visible);
            reveal = Some(position);
        }
    }
    reveal
}

#[derive(Clone)]
enum FileAction {
    Open(PathBuf),
    Reveal(PathBuf),
    Ignore,
    SelectDirectory,
    SelectRepository,
    Copy(String),
    ShowChanges,
    LogFile(String),
    Commit,
    Stage,
    Unstage,
    Discard,
    Remove,
    StashSelection,
    Rename,
    Conflict(crate::conflict::Action),
}

fn file_context_menu(app: &SannyGitApp, index: usize) -> Vec<popup_menu::Entry<FileAction>> {
    use popup_menu::Entry as E;
    let Some(change) = app.changes.get(index) else {
        return Vec::new();
    };
    let repository = app.repositories.get(app.selected_repository);
    let mut entries = Vec::new();
    if let Some(repository) = repository {
        let path = Path::new(&repository.path).join(&change.path);
        let reveal = if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else if cfg!(target_os = "windows") {
            "Show in Explorer"
        } else {
            "Open Containing Folder"
        };
        entries.push(E::submenu(
            "Open",
            vec![
                E::item(
                    "Open File",
                    !app.busy() && change.kind != ChangeKind::Deleted,
                    FileAction::Open(path.clone()),
                )
                .help("The file must exist in the working tree and no operation can be running"),
                E::item(reveal, !app.busy(), FileAction::Reveal(path))
                    .help("Wait for the current operation to finish"),
            ],
        ));
    }
    entries.extend([
        E::item("Show Changes", true, FileAction::ShowChanges),
        E::item(
            "Log",
            !app.busy() && repository.is_some(),
            FileAction::LogFile(change.path.clone()),
        )
        .help("Open the history of this file"),
        E::Separator,
        E::item(
            "Commit…",
            !app.busy() && repository.is_some() && app.selected_change_indices().len() <= 1,
            FileAction::Commit,
        )
        .help("To commit selected files, Stage them and use Staged only in the commit panel"),
        E::Separator,
        E::item(
            "Stage",
            app.selected_staging_allowed(true),
            FileAction::Stage,
        )
        .help(
            "Stage selected changes. For a merge conflict, stage after editing the file to resolve it.",
        ),
        E::item(
            "Unstage",
            app.selected_staging_allowed(false),
            FileAction::Unstage,
        )
        .help(
            "Select a staged file after refreshing",
        ),
        E::item(
            "Ignore…",
            app.can_ignore_selected_changes(),
            FileAction::Ignore,
        )
        .help("Select untracked files with supported names, then refresh if needed"),
        E::Separator,
    ]);
    use crate::conflict::Action;
    if change.kind == ChangeKind::Conflicted {
        entries.push(E::item(
            Action::Edit.label(),
            app.can_open_conflict(index, Action::Edit),
            FileAction::Conflict(Action::Edit),
        ));
    }
    if change.kind == ChangeKind::Conflicted || change.staged {
        let actions = if change.kind == ChangeKind::Conflicted {
            &[Action::Ours, Action::Theirs, Action::Recreate][..]
        } else {
            &[Action::Recreate][..]
        };
        entries.push(E::submenu(
            "Resolve",
            actions
                .iter()
                .map(|&action| {
                    E::item(
                        action.label(),
                        app.can_open_conflict(index, action),
                        FileAction::Conflict(action),
                    )
                    .help(action.description())
                })
                .collect(),
        ));
        entries.push(E::Separator);
    }
    entries.extend([
        E::item(
            "Discard…",
            app.discard_blocker().is_none(),
            FileAction::Discard,
        )
        .help(app.discard_blocker().unwrap_or("")),
        E::item(
            "Remove…",
            app.remove_blocker().is_none(),
            FileAction::Remove,
        )
        .help(
            app.remove_blocker()
                .unwrap_or("Delete the selected tracked files and stage their removal"),
        ),
        E::item(
            "Stash Selection…",
            app.stash_selection_allowed(),
            FileAction::StashSelection,
        )
        .help("Select files that are not conflicted, then wait until the working tree is current"),
        E::item("Rename…", app.rename_path_allowed(), FileAction::Rename).help(
            "Select one file that is not deleted or conflicted, and wait until the working tree is current",
        ),
        E::Separator,
    ]);
    let selected = app.selected_change_indices();
    for (label, text) in [
        (
            "Copy Name",
            Some(
                selected
                    .iter()
                    .map(|&index| {
                        let path = &app.changes[index].path;
                        path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ),
        (
            "Copy Path",
            repository.map(|repository| {
                selected
                    .iter()
                    .map(|&index| {
                        std::path::Path::new(&repository.path)
                            .join(&app.changes[index].path)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
        ),
        (
            "Copy Relative Path",
            Some(
                selected
                    .iter()
                    .map(|&index| app.changes[index].path.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ),
    ] {
        entries.push(E::item(
            label,
            text.is_some(),
            FileAction::Copy(text.unwrap_or_default()),
        ));
    }
    entries.extend([
        E::Separator,
        E::item(
            "Select Directory",
            Path::new(&change.path)
                .parent()
                .is_some_and(|folder| !folder.as_os_str().is_empty()),
            FileAction::SelectDirectory,
        )
        .help("Select visible changes in this folder and its subfolders"),
        E::item("Select Repository Root", true, FileAction::SelectRepository)
            .help("Select all visible changes in this repository"),
    ]);
    entries
}

fn column_separator(
    ui: &mut Ui,
    id: &str,
    width: &mut f32,
    direction: f32,
    range: std::ops::RangeInclusive<f32>,
    palette: Palette,
) {
    let rect = ui.max_rect();
    let x = rect.right() - 0.5;
    let hit = Rect::from_min_max(
        eframe::egui::pos2(x - 5.0, rect.top()),
        eframe::egui::pos2(x + 5.0, rect.bottom()),
    );
    let response = ui
        .interact(
            hit,
            ui.make_persistent_id(("files.header.resize", id)),
            Sense::drag(),
        )
        .on_hover_cursor(eframe::egui::CursorIcon::ResizeColumn);
    if response.dragged() {
        *width = (*width + direction * response.drag_delta().x).clamp(*range.start(), *range.end());
    }
    let color = if response.hovered() || response.dragged() {
        palette.text
    } else {
        palette.border
    };
    ui.painter().line_segment(
        [
            eframe::egui::pos2(x, rect.top() + 3.0),
            eframe::egui::pos2(x, rect.bottom() - 3.0),
        ],
        (1.0, color),
    );
}

fn ui_line(response: &eframe::egui::Response, row: Rect, clip: Rect, palette: Palette) {
    response
        .ctx
        .layer_painter(response.layer_id)
        .with_clip_rect(clip)
        .line_segment(
            [
                row.left_bottom() - Vec2::new(0.0, 0.5),
                row.right_bottom() - Vec2::new(0.0, 0.5),
            ],
            (1.0, palette.border.gamma_multiply(0.6)),
        );
}

fn changes_title(ui: &mut Ui, count: usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("Files").family(semibold_family()));
        ui.label(format!("({count} files)"));
    });
}

fn filters(ui: &mut Ui, app: &mut SannyGitApp, palette: Palette) {
    let width_scale = (ui.available_width() / FILTER_WIDTH).min(1.0);
    ui.spacing_mut().item_spacing.x = 0.0;
    for (filter, label, width) in [
        (ChangeFilter::All, "All", 34.0),
        (ChangeFilter::Modified, "Modified", 88.0),
        (ChangeFilter::Added, "Added", 72.0),
        (ChangeFilter::Deleted, "Deleted", 76.0),
        (ChangeFilter::Conflicted, "Conflicted", 96.0),
    ] {
        let count = app
            .changes
            .iter()
            .filter(|c| filter_matches(filter, c.kind))
            .count();
        let selected = app.change_filter == filter;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width * width_scale, 22.0), Sense::click());
        response.widget_info(|| {
            eframe::egui::WidgetInfo::selected(eframe::egui::Role::Button, true, selected, label)
        });
        ui.painter().rect_filled(
            rect,
            4.0,
            if selected {
                palette.accent
            } else if response.hovered() {
                palette.hover
            } else {
                palette.panel
            },
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                4.0,
                ui.visuals().selection.stroke,
                eframe::egui::StrokeKind::Inside,
            );
        }
        let text_color = if selected {
            Color32::WHITE
        } else {
            palette.text
        };
        ui.painter().text(
            rect.left_center() + Vec2::new(6.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(crate::theme::ui_font_size()),
            text_color,
        );
        if filter != ChangeFilter::All {
            let badge = Rect::from_center_size(
                rect.right_center() - Vec2::new(12.0, 0.0),
                Vec2::new(16.0, 17.0),
            );
            ui.painter().rect_filled(
                badge,
                3.0,
                if matches!(filter, ChangeFilter::Deleted | ChangeFilter::Conflicted) {
                    palette.removed_background
                } else {
                    palette.border
                },
            );
            ui.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                count,
                FontId::proportional(crate::theme::ui_font_size()),
                palette.text,
            );
        }
        if response.clicked() {
            app.change_filter = filter;
            sync_visible_selection(app);
        }
    }
}

#[derive(Clone)]
enum ViewAction {
    Group,
    Scope(crate::preferences::ChangesScope),
    Ignored,
}

#[derive(Clone)]
enum ChangeRow {
    Directory { label: String, files: Vec<usize> },
    File(usize),
}

fn changes_view_button(ui: &mut Ui, app: &SannyGitApp) -> Option<ViewAction> {
    popup_menu::button(ui, Icon::List, "Changes view", || view_menu(app))
}

fn view_menu(app: &SannyGitApp) -> Vec<popup_menu::Entry<ViewAction>> {
    use crate::preferences::ChangesScope;
    use popup_menu::Entry as E;
    let mark = |on| if on { "✓ " } else { "" };
    let scope = app.preferences.changes_scope;
    vec![
        E::item(
            format!(
                "{}Group by Directory",
                mark(app.preferences.changes_group_directories)
            ),
            true,
            ViewAction::Group,
        ),
        E::Separator,
        E::item(
            format!("{}Show All", mark(scope == ChangesScope::All)),
            true,
            ViewAction::Scope(ChangesScope::All),
        ),
        E::item(
            format!("{}Only Staged", mark(scope == ChangesScope::Staged)),
            true,
            ViewAction::Scope(ChangesScope::Staged),
        ),
        E::item(
            format!("{}Only Unstaged", mark(scope == ChangesScope::Unstaged)),
            true,
            ViewAction::Scope(ChangesScope::Unstaged),
        ),
        E::Separator,
        E::item(
            format!(
                "{}Show Ignored Files",
                mark(app.preferences.show_ignored_files)
            ),
            true,
            ViewAction::Ignored,
        ),
    ]
}

fn apply_view_action(app: &mut SannyGitApp, action: ViewAction) {
    match action {
        ViewAction::Group => {
            app.preferences.changes_group_directories = !app.preferences.changes_group_directories;
        }
        ViewAction::Scope(scope) => {
            app.preferences.changes_scope = scope;
            sync_visible_selection(app);
        }
        ViewAction::Ignored => app.set_show_ignored_files(!app.preferences.show_ignored_files),
    }
}

fn row_visible(app: &SannyGitApp, change: &crate::models::ChangeFile) -> bool {
    filter_matches(app.change_filter, change.kind)
        && scope_matches(app.preferences.changes_scope, change)
}

fn scope_matches(
    scope: crate::preferences::ChangesScope,
    change: &crate::models::ChangeFile,
) -> bool {
    match scope {
        crate::preferences::ChangesScope::All => true,
        crate::preferences::ChangesScope::Staged => change.staged,
        crate::preferences::ChangesScope::Unstaged => change.unstaged,
    }
}

fn sync_visible_selection(app: &mut SannyGitApp) {
    let previous = app.selection.active;
    let visible: Vec<String> = app
        .changes
        .iter()
        .filter(|change| row_visible(app, change))
        .map(|change| change.path.clone())
        .collect();
    let active_ok = app
        .changes
        .get(app.selection.active)
        .is_some_and(|change| visible.iter().any(|path| path == &change.path));
    app.selection
        .paths
        .retain(|path| visible.iter().any(|item| item == path));
    if !active_ok {
        app.selection.active = app
            .changes
            .iter()
            .position(|change| {
                visible.iter().any(|path| path == &change.path)
                    && (app.selection.paths.is_empty()
                        || app.selection.paths.contains(&change.path))
            })
            .unwrap_or(usize::MAX);
    }
    app.selection.anchor = None;
    if app.selection.active != previous {
        app.diff_line_selection.clear();
    }
}

fn directory_key(path: &str) -> String {
    match path.rfind('/') {
        Some(index) => path[..index].to_owned(),
        None => String::new(),
    }
}

/// Preserve row order while the working tree refreshes after staging or unstaging.
fn sync_display_order(ui: &mut Ui, app: &SannyGitApp) -> Vec<String> {
    let order_id = ui.make_persistent_id((app.selected_repository, "files.display_order"));
    let visible_paths: Vec<String> = app
        .changes
        .iter()
        .filter(|change| row_visible(app, change))
        .map(|change| change.path.clone())
        .collect();
    let mut order: Vec<String> =
        ui.data_mut(|data| data.get_persisted(order_id).unwrap_or_default());
    if order.is_empty() {
        order = visible_paths.clone();
    } else {
        for path in &visible_paths {
            if !order.contains(path) {
                order.push(path.clone());
            }
        }
    }
    order.retain(|path| visible_paths.iter().any(|visible| visible == path));
    ui.data_mut(|data| data.insert_persisted(order_id, order.clone()));
    order
}

fn change_rows(app: &SannyGitApp, visible: &[usize]) -> Vec<ChangeRow> {
    if !app.preferences.changes_group_directories {
        return visible.iter().copied().map(ChangeRow::File).collect();
    }
    let mut groups: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for &index in visible {
        let key = directory_key(&app.changes[index].path);
        groups.entry(key).or_default().push(index);
    }
    let mut rows = Vec::new();
    for (key, files) in groups {
        let label = if key.is_empty() {
            "Repository root".to_owned()
        } else {
            key
        };
        rows.push(ChangeRow::Directory {
            label,
            files: files.clone(),
        });
        rows.extend(files.into_iter().map(ChangeRow::File));
    }
    rows
}

fn filter_matches(filter: ChangeFilter, kind: ChangeKind) -> bool {
    match filter {
        ChangeFilter::All => true,
        ChangeFilter::Modified => kind == ChangeKind::Modified,
        ChangeFilter::Added => kind == ChangeKind::Added,
        ChangeFilter::Deleted => kind == ChangeKind::Deleted,
        ChangeFilter::Conflicted => kind == ChangeKind::Conflicted,
    }
}
fn status_color(kind: ChangeKind, palette: Palette) -> Color32 {
    match kind {
        ChangeKind::Added | ChangeKind::Untracked => palette.added_text,
        ChangeKind::Ignored => palette.muted,
        ChangeKind::Deleted | ChangeKind::Conflicted => palette.removed_text,
        ChangeKind::Modified | ChangeKind::Renamed => palette.modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{install_fonts, ThemeChoice};
    use eframe::egui::{Context, Pos2, RawInput};

    #[test]
    fn index_refresh_keeps_visible_row_positions() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = Context::default();
            install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            app.changes = crate::mock_data::changes();
            let target = app.changes[2].path.clone();
            let label = |output: &eframe::egui::FullOutput, path: &str| -> f32 {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Text(text) if text.galley.job.text == path => {
                            Some(text.pos.y)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("Missing path label: {path}"))
            };
            for _ in 0..2 {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 500.0))),
                        ..Default::default()
                    },
                    |ui| show(ui, &mut app),
                );
                output.textures_delta.clear();
            }
            let mut before = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 500.0))),
                    ..Default::default()
                },
                |ui| show(ui, &mut app),
            );
            before.textures_delta.clear();
            let y_before = label(&before, &target);
            app.changes.swap(0, 2);
            let staged = app
                .changes
                .iter_mut()
                .find(|change| change.path == target)
                .unwrap();
            staged.staged = true;
            staged.kind = ChangeKind::Added;
            let mut after = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 500.0))),
                    ..Default::default()
                },
                |ui| show(ui, &mut app),
            );
            after.textures_delta.clear();
            let y_after = label(&after, &target);
            assert!(
                (y_before - y_after).abs() < 0.5,
                "Row moved after index refresh: before={y_before}, after={y_after}"
            );
        }
    }

    #[test]
    fn filters_conflicts_and_reverses_status_order_without_losing_selection() {
        use eframe::egui::{Event, Modifiers, PointerButton};
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = Context::default();
            install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            app.changes = crate::mock_data::changes();
            app.changes[0].kind = ChangeKind::Conflicted;
            app.selection.active = 0;
            let conflict = app.changes[0].path.clone();
            let added = app.changes[3].path.clone();
            let render = |app: &mut SannyGitApp, events| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(480.0, 360.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| show(ui, app),
                );
                output.textures_delta.clear();
                output
            };
            let find = |output: &eframe::egui::FullOutput, label: &str| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    eframe::egui::Shape::Text(text) if text.galley.job.text == label => {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                })
            };
            let click = |app: &mut SannyGitApp, pos| {
                for pressed in [true, false] {
                    render(
                        app,
                        vec![
                            Event::PointerMoved(pos),
                            Event::PointerButton {
                                pos,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                    );
                }
                render(app, vec![])
            };
            let output = render(&mut app, vec![]);
            let output = click(&mut app, find(&output, "Status").unwrap());
            assert!(find(&output, &added).unwrap().y < find(&output, &conflict).unwrap().y);
            assert_eq!(app.selection.active, 0);
            let output = click(&mut app, find(&output, "Status ↑").unwrap());
            assert!(find(&output, &added).unwrap().y > find(&output, &conflict).unwrap().y);
            assert_eq!(app.selection.active, 0);
            let output = click(&mut app, find(&output, "Conflicted").unwrap());
            assert_eq!(app.change_filter, ChangeFilter::Conflicted);
            assert!(find(&output, &conflict).is_some());
            assert!(find(&output, &added).is_none());
        }
    }

    #[test]
    fn merge_banner_stays_visible_during_a_refresh() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [Vec2::new(1100.0, 700.0), Vec2::new(1536.0, 1024.0)] {
                let ctx = Context::default();
                install_fonts(&ctx);
                egui_extras::install_image_loaders(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                app.merging = true;
                app.sequencing = crate::git::Sequencing::Merge;
                app.interface_style = crate::theme::InterfaceStyle::Modern;
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| show(ui, &mut app),
                );
                output.textures_delta.clear();
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, eframe::egui::Shape::Rect(rect)
                        if rect.fill == app.palette().warning_background
                            && rect.rect.height() == MERGE_BANNER_HEIGHT
                            && rect.corner_radius == eframe::egui::CornerRadius::ZERO)
                }));
                for label in ["Merge in progress", "Abort"] {
                    assert!(output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, eframe::egui::Shape::Text(text) if text.galley.job.text == label)
                    }), "{theme:?} {size:?} {label}");
                }
            }
        }
    }

    #[test]
    fn modifier_clicks_select_ranges_and_right_click_preserves_the_group() {
        use eframe::egui::{Event, Modifiers, PointerButton};
        for (theme, full_window) in [ThemeChoice::Light, ThemeChoice::Dark]
            .into_iter()
            .flat_map(|theme| [false, true].map(|full_window| (theme, full_window)))
        {
            let ctx = Context::default();
            install_fonts(&ctx);
            egui_extras::install_image_loaders(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            app.changes = crate::mock_data::changes();
            let render = |app: &mut SannyGitApp, mut events: Vec<Event>, modifiers| {
                events.insert(0, Event::ModifiersChanged(modifiers));
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1100.0, 700.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if full_window {
                            crate::ui::show(ui, app);
                        } else {
                            show(ui, app);
                        }
                    },
                );
                output.textures_delta.clear();
                output
            };
            for _ in 0..3 {
                render(&mut app, vec![], Modifiers::NONE);
            }
            let output = render(&mut app, vec![], Modifiers::NONE);
            let positions: Vec<_> = app
                .changes
                .iter()
                .map(|change| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| {
                            if let eframe::egui::Shape::Text(text) = &shape.shape {
                                (text.galley.job.text == change.path).then(|| {
                                    Pos2::new(
                                        text.pos.x + 8.0,
                                        text.pos.y + text.galley.size().y / 2.0,
                                    )
                                })
                            } else {
                                None
                            }
                        })
                        .unwrap()
                })
                .collect();
            let click = |app: &mut SannyGitApp, index: usize, button, modifiers| {
                for pressed in [true, false] {
                    render(
                        app,
                        vec![
                            Event::PointerMoved(positions[index]),
                            Event::PointerButton {
                                pos: positions[index],
                                button,
                                pressed,
                                modifiers,
                            },
                        ],
                        modifiers,
                    );
                }
            };
            click(&mut app, 0, PointerButton::Primary, Modifiers::NONE);
            click(&mut app, 2, PointerButton::Primary, Modifiers::COMMAND);
            assert_eq!(app.selected_change_indices(), vec![0, 2]);
            click(&mut app, 4, PointerButton::Primary, Modifiers::SHIFT);
            assert_eq!(app.selected_change_indices(), vec![2, 3, 4]);
            assert!(ctx
                .data(
                    |data| data.get_temp::<bool>(eframe::egui::Id::unique("files.selection.focus"))
                )
                .unwrap_or(false));
            let key = |app: &mut SannyGitApp, key, modifiers| {
                render(
                    app,
                    vec![Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    modifiers,
                );
            };
            key(&mut app, eframe::egui::Key::ArrowUp, Modifiers::SHIFT);
            assert_eq!(app.selected_change_indices(), vec![2, 3]);
            key(&mut app, eframe::egui::Key::ArrowUp, Modifiers::SHIFT);
            assert_eq!(app.selected_change_indices(), vec![2]);
            key(&mut app, eframe::egui::Key::ArrowUp, Modifiers::SHIFT);
            assert_eq!(app.selected_change_indices(), vec![1, 2]);
            key(&mut app, eframe::egui::Key::ArrowDown, Modifiers::NONE);
            assert_eq!(app.selected_change_indices(), vec![2]);
            key(&mut app, eframe::egui::Key::Home, Modifiers::NONE);
            key(&mut app, eframe::egui::Key::End, Modifiers::SHIFT);
            assert_eq!(app.selected_change_indices().len(), app.changes.len());
            click(&mut app, 2, PointerButton::Primary, Modifiers::NONE);
            click(&mut app, 4, PointerButton::Primary, Modifiers::SHIFT);
            click(&mut app, 3, PointerButton::Secondary, Modifiers::NONE);
            assert_eq!(app.selected_change_indices(), vec![2, 3, 4]);
            assert_eq!(app.selection.active, 3);
            assert!(!app.busy());
        }
    }

    #[test]
    fn row_separators_align_with_striped_backgrounds() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            let ctx = Context::default();
            install_fonts(&ctx);
            theme.apply(&ctx);
            let mut app = SannyGitApp::with_theme(theme);
            app.changes = crate::mock_data::changes();
            app.selection.active = 2;
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 500.0))),
                        ..Default::default()
                    },
                    |ui| show(ui, &mut app),
                );
                output.textures_delta.clear();
                let palette = theme.palette();
                let stripes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Rect(rect)
                            if (rect.fill == palette.panel_alt
                                || rect.fill == palette.selection)
                                && rect.rect.width() > 100.0
                                && (rect.rect.height() - 23.0).abs() < 0.1 =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                let separators: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        eframe::egui::Shape::LineSegment { points, stroke }
                            if stroke.color == palette.border.gamma_multiply(0.6)
                                && points[0].y == points[1].y =>
                        {
                            Some(points[0].y)
                        }
                        _ => None,
                    })
                    .collect();
                let checkboxes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        eframe::egui::Shape::Rect(rect)
                            if rect.rect.right() < 52.0
                                && rect.rect.size() == Vec2::splat(14.0) =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(checkboxes.len(), separators.len());
                let index_center_x = 26.0;
                for (checkbox, bottom) in checkboxes.iter().zip(&separators) {
                    assert!(
                        (checkbox.center().y - (bottom - 11.0)).abs() <= 0.5,
                        "Checkbox {checkbox:?} is not centered above separator {bottom}"
                    );
                    assert!(
                        (checkbox.center().x - index_center_x).abs() <= 4.0,
                        "Checkbox {checkbox:?} is not centered in the Index column"
                    );
                }
                assert!(!stripes.is_empty());
                for stripe in stripes {
                    assert!(
                        separators
                            .iter()
                            .any(|y| (*y - stripe.bottom()).abs() <= 0.5),
                        "Stripe boundary {} does not match separators {separators:?}",
                        stripe.bottom()
                    );
                }
            }
        }
    }

    #[test]
    fn file_menu_targets_right_clicked_row_and_preserves_disabled_operations() {
        use eframe::egui::{Event, Modifiers, OutputCommand, PointerButton};

        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for width in [350.0, 900.0] {
                for action in [
                    "Copy Name",
                    "Copy Path",
                    "Copy Relative Path",
                    "Stage",
                    "Unstage",
                ] {
                    let ctx = Context::default();
                    install_fonts(&ctx);
                    egui_extras::install_image_loaders(&ctx);
                    theme.apply(&ctx);
                    let mut app = SannyGitApp::with_theme(theme);
                    app.repositories = crate::mock_data::repositories();
                    app.selected_repository = 0;
                    app.changes = crate::mock_data::changes();
                    app.selection.active = 1;
                    app.state_current = false;
                    let render = |app: &mut SannyGitApp, events| {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    Vec2::new(1100.0, 700.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                ui.set_width(width);
                                show(ui, app);
                            },
                        );
                        output.textures_delta.clear();
                        output
                    };
                    let position = |output: &eframe::egui::FullOutput, label: &str| {
                        output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                eframe::egui::Shape::Text(text)
                                    if text.galley.job.text == label =>
                                {
                                    Some(text.pos + text.galley.size() / 2.0)
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| panic!("Missing menu or file label: {label}"))
                    };
                    for _ in 0..3 {
                        render(&mut app, vec![]);
                    }
                    let output = render(&mut app, vec![]);
                    let y = position(&output, &app.changes[0].path).y;
                    let pointer = Pos2::new(130.0, y);
                    for pressed in [true, false] {
                        render(
                            &mut app,
                            vec![
                                Event::PointerMoved(pointer),
                                Event::PointerButton {
                                    pos: pointer,
                                    button: PointerButton::Secondary,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                },
                            ],
                        );
                    }
                    assert_eq!(app.selection.active, 0);
                    for _ in 0..3 {
                        render(&mut app, vec![]);
                    }
                    let output = render(&mut app, vec![]);
                    for label in [
                        "Show Changes",
                        "Commit…",
                        "Stage",
                        "Unstage",
                        "Ignore…",
                        "Select Directory",
                        "Select Repository Root",
                        "Discard…",
                        "Copy Name",
                        "Copy Path",
                        "Copy Relative Path",
                    ] {
                        position(&output, label);
                    }
                    for label in ["Rename…", "Stash Selection…"] {
                        position(&output, label);
                    }
                    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                        eframe::egui::Shape::Text(text) if matches!(text.galley.job.text.as_str(),
                            "Delete…" | "Blame"))));
                    let pointer = position(&output, action);
                    render(&mut app, vec![Event::PointerMoved(pointer)]);
                    let mut copied = None;
                    for pressed in [true, false] {
                        let output = render(
                            &mut app,
                            vec![Event::PointerButton {
                                pos: pointer,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            }],
                        );
                        for command in output.platform_output.commands {
                            if let OutputCommand::CopyText(text) = command {
                                copied = Some(text);
                            }
                        }
                    }
                    let expected = match action {
                        "Copy Name" => Some("main.rs".to_owned()),
                        "Copy Path" => Some(
                            std::path::Path::new(&app.repositories[0].path)
                                .join(&app.changes[0].path)
                                .to_string_lossy()
                                .into_owned(),
                        ),
                        "Copy Relative Path" => Some(app.changes[0].path.clone()),
                        _ => None,
                    };
                    assert_eq!(copied, expected, "{theme:?} {width} {action}");
                    assert!(!app.busy(), "Disabled operation started a worker");
                    assert!(app.error.is_none());
                    assert!(!app.changes[0].staged);
                }
            }
        }
    }

    #[test]
    fn header_separator_resizes_columns_and_keeps_the_new_width() {
        use eframe::egui::{Event, Modifiers, PointerButton};
        let ctx = Context::default();
        install_fonts(&ctx);
        ThemeChoice::Light.apply(&ctx);
        let mut app = SannyGitApp::with_theme(ThemeChoice::Light);
        let mut render = |events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 300.0))),
                    events,
                    ..Default::default()
                },
                |ui| show(ui, &mut app),
            );
            let line = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    eframe::egui::Shape::LineSegment { points, .. }
                        if (points[0].x - points[1].x).abs() < 0.1 =>
                    {
                        Some((points[0] + points[1].to_vec2()) * 0.5)
                    }
                    _ => None,
                })
                .unwrap();
            let x = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    eframe::egui::Shape::Text(text) if text.galley.job.text == "File" => {
                        Some(text.pos.x)
                    }
                    _ => None,
                })
                .unwrap();
            output.textures_delta.clear();
            (line, x)
        };
        for _ in 0..3 {
            render(vec![]);
        }
        let (pointer, before) = render(vec![]);
        render(vec![Event::PointerMoved(pointer)]);
        render(vec![Event::PointerButton {
            pos: pointer,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        let moved = pointer + Vec2::new(12.0, 0.0);
        for _ in 0..3 {
            render(vec![Event::PointerMoved(moved)]);
        }
        render(vec![Event::PointerButton {
            pos: moved,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        let (_, after) = render(vec![]);
        assert!(
            after > before + 8.0,
            "Header drag did not resize column: {before} -> {after}"
        );
        for _ in 0..5 {
            assert_eq!(render(vec![]).1, after);
        }
    }

    #[test]
    fn file_column_positions_do_not_change_with_rows_or_long_names() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for width in [350.0, 900.0] {
                let ctx = Context::default();
                install_fonts(&ctx);
                theme.apply(&ctx);
                let mut app = SannyGitApp::with_theme(theme);
                let render = |app: &mut SannyGitApp| {
                    let mut positions = Vec::new();
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    Vec2::new(width, 300.0),
                                )),
                                ..Default::default()
                            },
                            |ui| show(ui, app),
                        );
                        for shape in &output.shapes {
                            if let eframe::egui::Shape::LineSegment { points, .. } = &shape.shape {
                                if (points[0].x - points[1].x).abs() < 0.1 {
                                    assert!(
                                        (points[0].y - points[1].y).abs() <= 24.0,
                                        "Column separator extends into body"
                                    );
                                }
                            }
                        }
                        positions = ["Index", "File", "Status"]
                            .into_iter()
                            .map(|label| {
                                output
                                    .shapes
                                    .iter()
                                    .find_map(|shape| {
                                        if let eframe::egui::Shape::Text(text) = &shape.shape {
                                            if text.galley.job.text == label {
                                                return Some(text.pos);
                                            }
                                        }
                                        None
                                    })
                                    .unwrap_or_else(|| {
                                        panic!("Missing column {label} at width {width}")
                                    })
                            })
                            .collect::<Vec<_>>();
                        output.textures_delta.clear();
                    }
                    positions
                };
                let empty = render(&mut app);
                for path in [
                    "file.rs".to_owned(),
                    format!("{}/file.rs", "very-long-directory-name/".repeat(25)),
                ] {
                    app.changes = (0..80)
                        .map(|i| crate::models::ChangeFile {
                            path: format!("{i}-{path}"),
                            original_path: None,
                            kind: ChangeKind::Modified,
                            conflict_status: None,
                            staged: false,
                            unstaged: true,
                            diff: String::new(),
                        })
                        .collect();
                    let populated = render(&mut app);
                    for (before, after) in empty.iter().zip(populated) {
                        assert!(
                            (*before - after).length() < 1.0,
                            "Columns moved in {theme:?} at width {width}: {before:?} -> {after:?}"
                        );
                    }
                    app.changes.clear();
                    assert_eq!(render(&mut app), empty);
                }
            }
        }
    }
}
