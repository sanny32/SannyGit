use eframe::egui::{CentralPanel, Frame, Margin, Panel, Rect, Ui};

use crate::app::SannyGitApp;

pub(super) fn show(ui: &mut Ui, app: &mut SannyGitApp) -> [Rect; 4] {
    let palette = app.palette();
    let mut rects = [Rect::NOTHING; 4];
    // Nested splits fill the workspace: dragging a divider gives space to one
    // neighbour and takes the same amount from the other.
    Panel::bottom("working_tree.modern.linked.rows")
        .resizable(true)
        .default_size(230.0)
        .size_range(170.0..=(ui.available_height() - 180.0).max(170.0))
        .frame(Frame::new().fill(palette.background).inner_margin(Margin {
            top: 6,
            ..Margin::ZERO
        }))
        .show(ui, |ui| {
            ui.take_available_space();
            let commit = Panel::left("working_tree.modern.linked.commit")
                .resizable(true)
                .default_size(ui.available_width() * 0.42)
                .size_range(310.0..=(ui.available_width() - 420.0).max(310.0))
                .frame(super::panel_frame(palette).outer_margin(Margin {
                    right: 6,
                    ..Margin::ZERO
                }))
                .show(ui, |ui| {
                    ui.take_available_space();
                    super::commit_panel::show(ui, app);
                });
            rects[2] = commit.response.rect;
            rects[3] = CentralPanel::default()
                .frame(super::panel_frame(palette))
                .show(ui, |ui| super::journal::show(ui, app))
                .response
                .rect;
        });
    rects[0] = Panel::left("working_tree.modern.linked.files")
        .resizable(true)
        .default_size(ui.available_width() * 0.48)
        .size_range(310.0..=(ui.available_width() - 320.0).max(310.0))
        .frame(super::panel_frame(palette).outer_margin(Margin {
            right: 6,
            ..Margin::ZERO
        }))
        .show(ui, |ui| {
            ui.take_available_space();
            super::header(ui, 34.0, palette, |ui| {
                let name = app
                    .repositories
                    .get(app.selected_repository)
                    .map_or("Working Tree", |repository| repository.name.as_str());
                ui.heading(name);
                ui.label("›");
                ui.label(&app.current_branch);
            });
            super::changes::show(ui, app);
        })
        .response
        .rect;
    rects[1] = CentralPanel::default()
        .frame(super::panel_frame(palette))
        .show(ui, |ui| super::diff_view::show(ui, app))
        .response
        .rect;
    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{InterfaceStyle, ThemeChoice};
    use eframe::egui::{pos2, vec2, Context, Event, Modifiers, PointerButton, RawInput};

    fn assert_filled(rects: [Rect; 4], bounds: Rect) {
        let [files, diff, commit, journal] = rects;
        for rect in rects {
            assert!(
                bounds.expand(1.0).contains_rect(rect),
                "Panel overflow: {rect:?}"
            );
            assert!(rect.width() > 300.0 && rect.height() >= 160.0);
        }
        // Only the fixed divider gutters (six points plus the frame stroke) may remain between panels.
        assert!((files.left() - bounds.left()).abs() < 1.0);
        assert!((commit.left() - bounds.left()).abs() < 1.0);
        assert!((diff.right() - bounds.right()).abs() < 1.0);
        assert!((journal.right() - bounds.right()).abs() < 1.0);
        assert!((files.right() - diff.left()).abs() < 1.0);
        assert!((commit.right() - journal.left()).abs() < 1.0);
        assert!((files.top() - bounds.top()).abs() < 1.0);
        assert!((diff.top() - bounds.top()).abs() < 1.0);
        assert!((files.bottom() - diff.bottom()).abs() < 1.0);
        assert!((commit.top() - journal.top()).abs() < 1.0);
        assert!(
            (commit.top() - files.bottom() - 7.0).abs() < 1.0,
            "{rects:?} bounds={bounds:?}"
        );
        assert!((commit.bottom() - bounds.bottom()).abs() < 1.0);
        assert!((journal.bottom() - bounds.bottom()).abs() < 1.0);
    }

    #[test]
    fn dividers_resize_neighbours_without_gaps_and_retain_sizes() {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            for size in [vec2(820.0, 580.0), vec2(1250.0, 900.0)] {
                for divider in [0, 2, 3] {
                    for direction in [-1.0, 1.0] {
                        let ctx = Context::default();
                        crate::theme::install_fonts(&ctx);
                        theme.apply_style(&ctx, InterfaceStyle::Modern);
                        let mut app = SannyGitApp::with_theme(theme);
                        app.interface_style = InterfaceStyle::Modern;
                        let bounds = Rect::from_min_size(pos2(0.0, 0.0), size);
                        let render = |app: &mut SannyGitApp, events| {
                            let mut rects = [Rect::NOTHING; 4];
                            let mut output = ctx.run_ui(
                                RawInput {
                                    screen_rect: Some(bounds),
                                    events,
                                    ..Default::default()
                                },
                                |ui| {
                                    CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
                                        rects = show(ui, app);
                                    });
                                },
                            );
                            output.textures_delta.clear();
                            assert_filled(rects, bounds);
                            rects
                        };
                        for _ in 0..3 {
                            render(&mut app, vec![]);
                        }
                        let before = render(&mut app, vec![]);
                        let horizontal = divider != 3;
                        let pointer = if horizontal {
                            pos2(before[divider].right(), before[divider].center().y)
                        } else {
                            pos2(bounds.center().x, before[0].bottom())
                        };
                        render(&mut app, vec![Event::PointerMoved(pointer)]);
                        render(
                            &mut app,
                            vec![Event::PointerButton {
                                pos: pointer,
                                button: PointerButton::Primary,
                                pressed: true,
                                modifiers: Modifiers::NONE,
                            }],
                        );
                        let moved = pointer
                            + if horizontal {
                                vec2(direction * 35.0, 0.0)
                            } else {
                                vec2(0.0, direction * 35.0)
                            };
                        for _ in 0..3 {
                            render(&mut app, vec![Event::PointerMoved(moved)]);
                        }
                        render(
                            &mut app,
                            vec![Event::PointerButton {
                                pos: moved,
                                button: PointerButton::Primary,
                                pressed: false,
                                modifiers: Modifiers::NONE,
                            }],
                        );
                        let after = render(&mut app, vec![]);
                        if horizontal {
                            let growth = after[divider].width() - before[divider].width();
                            assert!(
                                growth * direction > 25.0,
                                "Divider did not move: {divider} {theme:?} {size:?}"
                            );
                            assert!(
                                (after[divider + 1].width() - before[divider + 1].width() + growth)
                                    .abs()
                                    < 1.0
                            );
                            let other = if divider == 0 { 2 } else { 0 };
                            assert_eq!(before[other], after[other]);
                            assert_eq!(before[other + 1], after[other + 1]);
                        } else {
                            let growth = after[0].height() - before[0].height();
                            assert!(growth * direction > 25.0, "Row divider did not move");
                            assert!((after[2].height() - before[2].height() + growth).abs() < 1.0);
                            assert!((after[1].height() - before[1].height() - growth).abs() < 1.0);
                            assert!((after[3].height() - before[3].height() + growth).abs() < 1.0);
                        }
                        app.history_loading = true;
                        app.refs_loading = true;
                        for _ in 0..5 {
                            assert_eq!(render(&mut app, vec![]), after);
                        }
                    }
                }
            }
        }
    }
}
