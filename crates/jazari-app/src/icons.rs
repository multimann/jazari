//! Small icons painted with egui shapes, so they do not depend on which
//! symbols a font happens to have.

use eframe::egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2,
};

/// An icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// A normal layer: a page.
    Layer,
    /// A guide layer.
    Guide,
    /// A mask layer.
    Mask,
    /// A layer folder or a library folder.
    Folder,
    /// An unknown layer kind.
    Unknown,
    /// A movie clip symbol.
    MovieClip,
    /// A graphic symbol.
    Graphic,
    /// A button symbol.
    Button,
    /// A bitmap.
    Bitmap,
    /// A font.
    Font,
    /// A scene.
    Scene,
    /// Hidden (the eye column).
    Hidden,
    /// Locked (the lock column).
    Locked,
    /// Shown as outlines.
    Outline,
    /// An "on" dot in the eye or lock column.
    Dot,
}

/// Paints `icon` centred in `rect` with the foreground colour `fg`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, fg: Color32) {
    let s = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), Vec2::splat(s));
    let thin = Stroke::new((s / 12.0).max(1.0), fg);
    let at = |x: f32, y: f32| pos2(r.left() + x * s, r.top() + y * s);
    match icon {
        Icon::Layer => {
            let page = Rect::from_min_max(at(0.22, 0.12), at(0.78, 0.88));
            painter.rect_stroke(page, CornerRadius::ZERO, thin, StrokeKind::Inside);
            painter.line_segment([at(0.62, 0.12), at(0.78, 0.28)], thin);
            for y in [0.42, 0.56, 0.70] {
                painter.line_segment([at(0.32, y), at(0.68, y)], thin);
            }
        }
        Icon::Guide => {
            painter.line_segment([at(0.2, 0.8), at(0.8, 0.2)], thin);
            painter.circle_stroke(at(0.5, 0.5), s * 0.12, thin);
            painter.line_segment([at(0.2, 0.2), at(0.36, 0.36)], thin);
            painter.line_segment([at(0.64, 0.64), at(0.8, 0.8)], thin);
        }
        Icon::Mask => {
            painter.rect_filled(
                Rect::from_min_max(at(0.15, 0.2), at(0.85, 0.8)),
                CornerRadius::same(2),
                fg,
            );
            painter.circle_filled(at(0.5, 0.5), s * 0.2, Color32::from_gray(128));
        }
        Icon::Folder => {
            let body = Rect::from_min_max(at(0.1, 0.32), at(0.9, 0.82));
            painter.rect_filled(body, CornerRadius::same(1), fg.linear_multiply(0.35));
            painter.rect_stroke(body, CornerRadius::same(1), thin, StrokeKind::Inside);
            painter.add(Shape::convex_polygon(
                vec![at(0.1, 0.32), at(0.14, 0.2), at(0.42, 0.2), at(0.48, 0.32)],
                fg.linear_multiply(0.6),
                Stroke::NONE,
            ));
        }
        Icon::Unknown => {
            painter.circle_stroke(at(0.5, 0.5), s * 0.36, thin);
            painter.circle_filled(at(0.5, 0.5), s * 0.08, fg);
        }
        Icon::MovieClip => {
            let body = Rect::from_min_max(at(0.12, 0.22), at(0.88, 0.78));
            painter.rect_filled(body, CornerRadius::same(2), Color32::from_rgb(70, 130, 210));
            for x in [0.24, 0.42, 0.6, 0.78] {
                painter.rect_filled(
                    Rect::from_center_size(at(x - 0.02, 0.3), Vec2::splat(s * 0.07)),
                    CornerRadius::ZERO,
                    Color32::WHITE,
                );
                painter.rect_filled(
                    Rect::from_center_size(at(x - 0.02, 0.7), Vec2::splat(s * 0.07)),
                    CornerRadius::ZERO,
                    Color32::WHITE,
                );
            }
        }
        Icon::Graphic => {
            painter.add(Shape::convex_polygon(
                vec![at(0.5, 0.15), at(0.88, 0.82), at(0.12, 0.82)],
                Color32::from_rgb(80, 170, 90),
                Stroke::NONE,
            ));
            painter.circle_filled(at(0.68, 0.32), s * 0.14, Color32::from_rgb(230, 180, 60));
        }
        Icon::Button => {
            let body = Rect::from_min_max(at(0.1, 0.3), at(0.9, 0.72));
            painter.rect_filled(body, CornerRadius::same(4), Color32::from_rgb(230, 140, 50));
            painter.line_segment(
                [at(0.3, 0.51), at(0.7, 0.51)],
                Stroke::new(thin.width, Color32::WHITE),
            );
        }
        Icon::Bitmap => {
            let frame = Rect::from_min_max(at(0.12, 0.2), at(0.88, 0.8));
            painter.rect_filled(frame, CornerRadius::ZERO, Color32::from_rgb(150, 200, 240));
            painter.add(Shape::convex_polygon(
                vec![at(0.12, 0.8), at(0.42, 0.42), at(0.7, 0.8)],
                Color32::from_rgb(60, 140, 70),
                Stroke::NONE,
            ));
            painter.circle_filled(at(0.7, 0.36), s * 0.08, Color32::from_rgb(250, 220, 80));
        }
        Icon::Font => {
            painter.text(
                r.center(),
                eframe::egui::Align2::CENTER_CENTER,
                "A",
                eframe::egui::FontId::proportional(s * 0.85),
                fg,
            );
        }
        Icon::Scene => {
            painter.rect_stroke(
                Rect::from_min_max(at(0.12, 0.24), at(0.88, 0.76)),
                CornerRadius::same(1),
                thin,
                StrokeKind::Inside,
            );
            painter.add(Shape::convex_polygon(
                vec![at(0.42, 0.38), at(0.62, 0.5), at(0.42, 0.62)],
                fg,
                Stroke::NONE,
            ));
        }
        Icon::Hidden => {
            let red = Color32::from_rgb(220, 60, 50);
            let st = Stroke::new(thin.width * 1.5, red);
            painter.line_segment([at(0.28, 0.28), at(0.72, 0.72)], st);
            painter.line_segment([at(0.72, 0.28), at(0.28, 0.72)], st);
        }
        Icon::Locked => {
            let body = Rect::from_min_max(at(0.26, 0.46), at(0.74, 0.84));
            painter.rect_filled(body, CornerRadius::same(1), fg);
            let shackle = [
                at(0.34, 0.46),
                at(0.34, 0.3),
                at(0.42, 0.2),
                at(0.58, 0.2),
                at(0.66, 0.3),
                at(0.66, 0.46),
            ];
            painter.add(Shape::line(shackle.to_vec(), thin));
        }
        Icon::Outline => {
            painter.rect_stroke(
                Rect::from_center_size(r.center(), vec2(s * 0.5, s * 0.5)),
                CornerRadius::ZERO,
                thin,
                StrokeKind::Inside,
            );
        }
        Icon::Dot => {
            painter.circle_filled(r.center(), (s * 0.08).max(1.0), fg);
        }
    }
}

/// A small flag for a frame label, with its top-left corner at `top_left`.
pub fn label_flag(painter: &Painter, top_left: Pos2, height: f32, color: Color32) {
    let pole = Stroke::new(1.0, color);
    painter.line_segment([top_left, top_left + vec2(0.0, height)], pole);
    painter.add(Shape::convex_polygon(
        vec![
            top_left,
            top_left + vec2(height * 0.5, height * 0.18),
            top_left + vec2(0.0, height * 0.36),
        ],
        color,
        Stroke::NONE,
    ));
}
