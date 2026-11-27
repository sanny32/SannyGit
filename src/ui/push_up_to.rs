use super::controls::{
    dialog_style::{self, DialogResponse},
    icons::Icon,
    tooltips::TooltipResponse,
};
use crate::{app::SannyGitApp, theme::Palette};
use eframe::egui::{self, RichText};

pub fn show(ctx: &egui::Context, app: &mut SannyGitApp) {
    let Some(dialog) = app.push_up_to.take() else {
        return;
    };
    let palette = app.palette();
    let busy = app.busy();
    let ready = !busy && app.state_current && dialog.error.is_none();
    let (status, push_ready) = match (&dialog.error, &dialog.preview) {
        (Some(error), _) => (error.clone(), false),
        (None, Some(preview)) if preview.fast_forward => (
            crate::i18n::tr_msg("Do you want to push changes up to this commit?"),
            true,
        ),
        (None, Some(preview)) if preview.already_pushed => (
            crate::i18n::tr_msg("The remote branch already contains this commit."),
            false,
        ),
        (None, Some(preview)) if !preview.on_branch => (
            crate::i18n::tr_msg("This commit is not on the current branch."),
            false,
        ),
        (None, Some(_)) => (
            crate::i18n::tr_msg(
                "This commit cannot be pushed without rewriting the remote branch.",
            ),
            false,
        ),
        (None, None) if busy => (
            crate::i18n::tr_msg("Checking whether this commit can be pushed…"),
            false,
        ),
        (None, None) => (
            crate::i18n::tr_msg("Close and reopen Push to retry."),
            false,
        ),
    };
    let (close, push) = confirmation(
        ctx,
        Prompt {
            commit: &dialog.commit,
            subject: &dialog.subject,
            branch: &dialog.branch_name,
            upstream: &dialog.upstream,
            status: &status,
            ready: ready && push_ready,
            busy,
        },
        palette,
    );
    if push {
        app.push_up_to = Some(dialog);
        app.submit_push_up_to();
    } else if !close {
        app.push_up_to = Some(dialog);
    }
}

struct Prompt<'a> {
    commit: &'a str,
    subject: &'a str,
    branch: &'a str,
    upstream: &'a str,
    status: &'a str,
    ready: bool,
    busy: bool,
}

fn confirmation(ctx: &egui::Context, prompt: Prompt<'_>, palette: Palette) -> (bool, bool) {
    let mut close = false;
    let mut push = false;
    dialog_style::modal(ctx, palette, "push.up_to").show(ctx, |ui| {
        ui.set_width(560.0);
        dialog_style::setup(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        dialog_style::header_with_icon(
            ui,
            palette,
            "Push",
            "All commits up to the selected commit will be pushed to the remote repository.",
            Icon::Push,
        );
        dialog_style::section(ui, palette.panel, egui::Margin::symmetric(12, 10), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let commit_line =
                crate::i18n::tr_args("Commit: %{subject}", &[("subject", prompt.subject)]);
            dialog_style::text_line(ui, &commit_line, 20.0).help_text(prompt.commit);
            let branch_line =
                crate::i18n::tr_args("Branch: %{branch}", &[("branch", prompt.branch)]);
            dialog_style::text_line(ui, &branch_line, 20.0).help_text(&branch_line);
            let remote_line =
                crate::i18n::tr_args("Remote: %{remote}", &[("remote", prompt.upstream)]);
            dialog_style::text_line(ui, &remote_line, 20.0).help_text(&remote_line);
            dialog_style::text_line(ui, RichText::new(prompt.status).color(palette.muted), 20.0)
                .help_text(prompt.status);
            dialog_style::button_row(ui, |ui| {
                close = ui
                    .add_enabled(!prompt.busy, dialog_style::button("Cancel"))
                    .dialog_cancel();
                push = ui
                    .add_enabled(prompt.ready, dialog_style::primary("Push", palette))
                    .help_text(prompt.status)
                    .dialog_accept();
            });
        });
    });
    (close, push)
}

#[cfg(test)]
#[test]
fn push_confirmation_keeps_its_size_while_the_check_loads() {
    for theme in [
        crate::theme::ThemeChoice::Light,
        crate::theme::ThemeChoice::Dark,
    ] {
        for size in [egui::vec2(1100.0, 700.0), egui::vec2(1400.0, 900.0)] {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            theme.apply(&ctx);
            let hash = "ab".repeat(20);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut previous = None;
            for stage in 0..3 {
                let checking = stage != 1;
                let status = if checking {
                    "Checking whether this commit can be pushed…"
                } else {
                    "Do you want to push changes up to this commit?"
                };
                for _ in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| {
                            let (close, push) = confirmation(
                                ui.ctx(),
                                Prompt {
                                    commit: &hash,
                                    subject: "Middle commit",
                                    branch: "main",
                                    upstream: "origin/main",
                                    status,
                                    ready: !checking,
                                    busy: checking,
                                },
                                theme.palette(),
                            );
                            assert!(!close && !push);
                        },
                    );
                    output.textures_delta.clear();
                }
                let rect = ctx
                    .memory(|memory| memory.area_rect(egui::Id::unique("push.up_to")))
                    .unwrap();
                assert!(screen.contains_rect(rect));
                if let Some(previous) = previous {
                    assert_eq!(rect, previous);
                }
                previous = Some(rect);
            }
        }
    }
}
