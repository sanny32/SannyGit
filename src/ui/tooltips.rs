use crate::theme::ThemeChoice;
use eframe::egui::{CornerRadius, Frame, Label, Margin, Response, Stroke, Tooltip};

pub trait TooltipResponse {
    fn help_text(self, text: impl Into<String>) -> Self;
    fn disabled_help_text(self, text: impl Into<String>) -> Self;
}

fn show(response: &Response, text: String, disabled: bool) {
    let palette = if response.ctx.global_style().visuals.dark_mode {
        ThemeChoice::Dark
    } else {
        ThemeChoice::Light
    }
    .palette();
    let frame = Frame::popup(&response.ctx.global_style())
        .fill(palette.header)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(7, 4));
    let mut tooltip = if disabled {
        Tooltip::for_disabled(response)
    } else {
        Tooltip::for_enabled(response)
    };
    tooltip.popup = tooltip.popup.frame(frame);
    tooltip.width(320.0).show(|ui| {
        ui.add(Label::new(text).wrap());
    });
}

impl TooltipResponse for Response {
    fn help_text(self, text: impl Into<String>) -> Self {
        show(&self, text.into(), false);
        self
    }

    fn disabled_help_text(self, text: impl Into<String>) -> Self {
        show(&self, text.into(), true);
        self
    }
}
