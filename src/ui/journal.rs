use eframe::egui::{Align, Color32, Layout, RichText, Sense, TextEdit, Ui, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::app::SannyGitApp;

pub fn show(ui: &mut Ui, app: &mut SannyGitApp) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Journal");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                TextEdit::singleline(&mut app.journal_filter)
                    .hint_text("Search commits...")
                    .desired_width(180.0),
            );
        });
    });
    ui.add_space(6.0);

    let filter = app.journal_filter.to_lowercase();
    let visible = app
        .commits
        .iter()
        .filter(|commit| {
            filter.is_empty()
                || commit.message.to_lowercase().contains(&filter)
                || commit.author.to_lowercase().contains(&filter)
                || commit.hash.to_lowercase().contains(&filter)
        })
        .collect::<Vec<_>>();

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::initial(42.0))
        .column(Column::remainder().at_least(140.0))
        .column(Column::initial(72.0))
        .column(Column::initial(88.0))
        .column(Column::initial(74.0))
        .min_scrolled_height(0.0)
        .header(24.0, |mut header| {
            for title in ["Graph", "Message", "Author", "Date", "Hash"] {
                header.col(|ui| {
                    ui.label(RichText::new(title).strong());
                });
            }
        })
        .body(|body| {
            body.rows(24.0, visible.len(), |mut row| {
                let commit = visible[row.index()];
                let last = row.index() + 1 == visible.len();
                row.col(|ui| graph(ui, ui.visuals().hyperlink_color, last));
                row.col(|ui| {
                    ui.label(commit.message);
                });
                row.col(|ui| {
                    ui.label(commit.author);
                });
                row.col(|ui| {
                    ui.label(commit.date);
                });
                row.col(|ui| {
                    ui.label(RichText::new(commit.hash).monospace());
                });
            });
        });
}

fn graph(ui: &mut Ui, color: Color32, last: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(30.0, 24.0), Sense::hover());
    let x = rect.center().x;
    let bottom = if last { rect.center().y } else { rect.bottom() };
    ui.painter()
        .line_segment([(x, rect.top()).into(), (x, bottom).into()], (1.5, color));
    ui.painter().circle_filled(rect.center(), 3.5, color);
}
