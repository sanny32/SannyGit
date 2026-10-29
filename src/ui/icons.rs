use eframe::egui::{Color32, Painter, Rect, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy)]
pub enum Icon {
    Pull,
    Push,
    Fetch,
    Branch,
    Merge,
    Rebase,
    Stash,
    Discard,
    Repository,
    Tag,
}

pub fn paint(ui: &mut Ui, icon: Icon, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter_at(rect);
    let stroke = Stroke::new((size / 12.0).max(1.0), color);

    match icon {
        Icon::Pull => {
            arrow(&painter, rect, (0.5, 0.1), (0.5, 0.65), stroke);
            tray(&painter, rect, stroke);
        }
        Icon::Push => {
            arrow(&painter, rect, (0.5, 0.65), (0.5, 0.1), stroke);
            tray(&painter, rect, stroke);
        }
        Icon::Fetch => {
            arrow(&painter, rect, (0.3, 0.1), (0.3, 0.75), stroke);
            arrow(&painter, rect, (0.7, 0.9), (0.7, 0.25), stroke);
        }
        Icon::Branch => {
            polyline(&painter, rect, &[(0.3, 0.85), (0.3, 0.45), (0.55, 0.45), (0.7, 0.3), (0.7, 0.15)], stroke);
            line(&painter, rect, (0.3, 0.45), (0.3, 0.15), stroke);
            for center in [(0.3, 0.15), (0.3, 0.85), (0.7, 0.15)] {
                circle(&painter, rect, center, 0.08, stroke);
            }
        }
        Icon::Merge => {
            line(&painter, rect, (0.3, 0.15), (0.3, 0.85), stroke);
            polyline(&painter, rect, &[(0.7, 0.85), (0.7, 0.65), (0.3, 0.4)], stroke);
            for center in [(0.3, 0.15), (0.3, 0.85), (0.7, 0.85)] {
                circle(&painter, rect, center, 0.08, stroke);
            }
        }
        Icon::Rebase => {
            line(&painter, rect, (0.15, 0.8), (0.85, 0.8), stroke);
            arrow(&painter, rect, (0.5, 0.8), (0.5, 0.15), stroke);
            circle(&painter, rect, (0.2, 0.8), 0.06, stroke);
            circle(&painter, rect, (0.8, 0.8), 0.06, stroke);
        }
        Icon::Stash => {
            polyline(&painter, rect, &[(0.15, 0.35), (0.85, 0.35), (0.85, 0.85), (0.15, 0.85), (0.15, 0.35)], stroke);
            line(&painter, rect, (0.15, 0.5), (0.85, 0.5), stroke);
            arrow(&painter, rect, (0.5, 0.05), (0.5, 0.32), stroke);
        }
        Icon::Discard => {
            polyline(&painter, rect, &[(0.3, 0.3), (0.35, 0.85), (0.65, 0.85), (0.7, 0.3)], stroke);
            line(&painter, rect, (0.2, 0.3), (0.8, 0.3), stroke);
            line(&painter, rect, (0.4, 0.15), (0.6, 0.15), stroke);
            line(&painter, rect, (0.45, 0.45), (0.45, 0.7), stroke);
            line(&painter, rect, (0.55, 0.45), (0.55, 0.7), stroke);
        }
        Icon::Repository => {
            polyline(&painter, rect, &[(0.1, 0.25), (0.4, 0.25), (0.48, 0.38), (0.9, 0.38), (0.85, 0.85), (0.1, 0.85), (0.1, 0.25)], stroke);
        }
        Icon::Tag => {
            polyline(&painter, rect, &[(0.15, 0.15), (0.55, 0.15), (0.9, 0.5), (0.55, 0.85), (0.15, 0.85), (0.15, 0.15)], stroke);
            circle(&painter, rect, (0.3, 0.32), 0.055, stroke);
        }
    }
}

fn point(rect: Rect, x: f32, y: f32) -> eframe::egui::Pos2 {
    rect.min + Vec2::new(x * rect.width(), y * rect.height())
}

fn line(painter: &Painter, rect: Rect, from: (f32, f32), to: (f32, f32), stroke: Stroke) {
    painter.line_segment([point(rect, from.0, from.1), point(rect, to.0, to.1)], stroke);
}

fn polyline(painter: &Painter, rect: Rect, points: &[(f32, f32)], stroke: Stroke) {
    for segment in points.windows(2) {
        line(painter, rect, segment[0], segment[1], stroke);
    }
}

fn circle(painter: &Painter, rect: Rect, center: (f32, f32), radius: f32, stroke: Stroke) {
    painter.circle(point(rect, center.0, center.1), radius * rect.width(), Color32::TRANSPARENT, stroke);
}

fn arrow(painter: &Painter, rect: Rect, from: (f32, f32), to: (f32, f32), stroke: Stroke) {
    line(painter, rect, from, to, stroke);
    let direction = if to.1 > from.1 { -1.0 } else { 1.0 };
    line(painter, rect, to, (to.0 - 0.2, to.1 + 0.22 * direction), stroke);
    line(painter, rect, to, (to.0 + 0.2, to.1 + 0.22 * direction), stroke);
}

fn tray(painter: &Painter, rect: Rect, stroke: Stroke) {
    polyline(painter, rect, &[(0.15, 0.72), (0.15, 0.9), (0.85, 0.9), (0.85, 0.72)], stroke);
}
