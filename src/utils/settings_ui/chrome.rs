use winisland_render::FontWeight;
use winisland_render::text::FontManager;
use winisland_render::{
    BlurSpec, FontStyle, Image, ImageOptions, Painter, Path, Point, Radius, Rect, Rgba, Sampling,
    StrokeCap, StrokeJoin, Vec2,
};

use crate::utils::color::SettingsTheme;

use super::ellipsize_text;
use super::text_baseline;

pub(crate) const SIDEBAR_INSET: f32 = 8.0;
pub(crate) const WINDOW_CORNER: f32 = 22.0;
pub(crate) const SIDEBAR_PANEL_RADIUS: f32 = WINDOW_CORNER - SIDEBAR_INSET;
pub(crate) const SIDEBAR_ICON: f32 = 20.0;
pub(crate) const SEARCH_TOP: f32 = 42.0;
pub(crate) const SEARCH_H: f32 = 28.0;
pub(crate) const TRAFFIC_LIGHT_Y: f32 = 23.0;
pub(crate) const TRAFFIC_LIGHT_X: [f32; 3] = [24.0, 44.0, 64.0];
pub(crate) const TRAFFIC_LIGHT_RADIUS: f32 = 6.0;

pub(crate) fn sidebar_panel(sidebar_w: f32, win_h: f32) -> Rect {
    Rect::from_xywh(
        SIDEBAR_INSET,
        SIDEBAR_INSET,
        sidebar_w - SIDEBAR_INSET * 2.0,
        win_h - SIDEBAR_INSET * 2.0,
    )
}

pub(crate) fn search_rect(sidebar_w: f32, pad: f32) -> Rect {
    Rect::from_xywh(pad, SEARCH_TOP, sidebar_w - pad * 2.0, SEARCH_H)
}

pub(crate) fn traffic_light_centers() -> [(f32, f32); 3] {
    TRAFFIC_LIGHT_X.map(|x| (x, TRAFFIC_LIGHT_Y))
}

/// Fills the opaque window base around the floating sidebar and draws the sidebar panel, leaving
/// the panel translucent when `glass` is true so the composition backdrop shows through.
pub(crate) fn draw_window_base(
    painter: Painter<'_>,
    theme: &SettingsTheme,
    window: Rect,
    panel: Rect,
    glass: bool,
) {
    let panel_path = Path::continuous_rounded_rect(panel, SIDEBAR_PANEL_RADIUS);
    let saved = painter.save();
    painter.clip_path_difference(&panel_path);
    painter.fill_rect(window, theme.win_bg);
    painter.fill_path_blurred(
        &Path::continuous_rounded_rect(panel.offset(Vec2::new(0.0, 1.5)), SIDEBAR_PANEL_RADIUS),
        theme.sidebar_shadow,
        BlurSpec::uniform(5.0),
    );
    painter.restore_to(saved);
    painter.fill_path(
        &panel_path,
        if glass {
            theme.sidebar_glass
        } else {
            theme.sidebar_bg
        },
    );
    painter.stroke_path(
        &Path::continuous_rounded_rect(panel.inset(0.5), SIDEBAR_PANEL_RADIUS - 0.5),
        1.0,
        theme.sidebar_border,
        StrokeCap::Butt,
        StrokeJoin::Round,
    );
}

/// Clips drawing to the window's continuous corner shape.
pub(crate) fn clip_window(painter: Painter<'_>, window: Rect) {
    painter.clip_path(&Path::continuous_rounded_rect(window, WINDOW_CORNER));
}

pub(crate) fn stroke_window(painter: Painter<'_>, window: Rect, color: Rgba) {
    painter.stroke_path(
        &Path::continuous_rounded_rect(window.inset(0.5), WINDOW_CORNER - 0.5),
        1.0,
        color,
        StrokeCap::Butt,
        StrokeJoin::Round,
    );
}

/// Corner radius for the circular composition backdrop so it stays inside the continuous
/// panel corner while the opaque base covers anything beyond it.
pub(crate) const BACKDROP_CORNER: f32 = SIDEBAR_PANEL_RADIUS * 0.6;

pub(crate) fn draw_traffic_lights(
    painter: Painter<'_>,
    is_light: bool,
    focused: bool,
    hovered: bool,
    text_sec: Rgba,
) {
    let inactive = if is_light {
        Rgba::from_rgb(200, 200, 204)
    } else {
        Rgba::from_rgb(78, 78, 82)
    };
    let fills = if focused {
        [
            Rgba::from_rgb(255, 95, 87),
            Rgba::from_rgb(254, 188, 46),
            inactive,
        ]
    } else {
        [inactive; 3]
    };
    let border = Rgba::from_argb(if is_light { 30 } else { 50 }, 0, 0, 0);
    for ((x, y), fill) in traffic_light_centers().into_iter().zip(fills) {
        let center = Point::new(x, y);
        painter.fill_circle(center, TRAFFIC_LIGHT_RADIUS, fill);
        painter.stroke_circle(center, TRAFFIC_LIGHT_RADIUS - 0.25, 0.5, border);
    }
    if !hovered {
        return;
    }
    let marks = [
        if focused {
            Rgba::from_rgb(120, 10, 6)
        } else {
            text_sec
        },
        if focused {
            Rgba::from_rgb(140, 90, 0)
        } else {
            text_sec
        },
    ];
    let (close_x, y) = (TRAFFIC_LIGHT_X[0], TRAFFIC_LIGHT_Y);
    for direction in [-1.0, 1.0] {
        painter.stroke_line(
            Point::new(close_x - 2.5, y - 2.5 * direction),
            Point::new(close_x + 2.5, y + 2.5 * direction),
            1.1,
            marks[0],
            StrokeCap::Round,
        );
    }
    let minimize_x = TRAFFIC_LIGHT_X[1];
    painter.stroke_line(
        Point::new(minimize_x - 3.5, y),
        Point::new(minimize_x + 3.5, y),
        1.1,
        marks[1],
        StrokeCap::Round,
    );
}

pub(crate) fn traffic_light_at(x: f32, y: f32) -> Option<usize> {
    traffic_light_centers()
        .iter()
        .position(|&(cx, cy)| (x - cx).powi(2) + (y - cy).powi(2) <= 64.0)
}

pub(crate) fn traffic_lights_hovered(x: f32, y: f32) -> bool {
    (TRAFFIC_LIGHT_X[0] - 10.0..=TRAFFIC_LIGHT_X[2] + 10.0).contains(&x)
        && (TRAFFIC_LIGHT_Y - 10.0..=TRAFFIC_LIGHT_Y + 10.0).contains(&y)
}

pub(crate) struct SidebarRow<'a> {
    pub(crate) rect: Rect,
    pub(crate) icon: Option<&'a Image>,
    pub(crate) label: &'a str,
    pub(crate) selected: bool,
    pub(crate) focused: bool,
    pub(crate) hover: f32,
}

pub(crate) fn draw_sidebar_row(painter: Painter<'_>, theme: &SettingsTheme, row: SidebarRow<'_>) {
    let radius = Radius::uniform(8.0);
    let text_color = if row.selected {
        if row.focused {
            painter.fill_round_rect(row.rect, radius, theme.selection_bg);
            theme.selection_text
        } else {
            painter.fill_round_rect(row.rect, radius, theme.card_highlight);
            theme.text_pri
        }
    } else {
        if row.hover > 0.005 {
            let base = theme.sidebar_hover;
            painter.fill_round_rect(
                row.rect,
                radius,
                base.with_alpha_f(f32::from(base.a()) / 255.0 * row.hover),
            );
        }
        theme.text_pri
    };
    let icon_rect = Rect::from_xywh(
        row.rect.left + 8.0,
        row.rect.center_y() - SIDEBAR_ICON / 2.0,
        SIDEBAR_ICON,
        SIDEBAR_ICON,
    );
    if let Some(icon) = row.icon {
        draw_icon_shadow(painter, icon_rect, 1.0);
        painter.draw_image(
            icon,
            icon_rect,
            &ImageOptions::default()
                .with_sampling(Sampling::LinearLinear)
                .with_anti_alias(false),
        );
    }
    let text_left = icon_rect.right + 9.0;
    let label = ellipsize_text(
        FontManager::global(),
        row.label,
        13.0,
        FontStyle::normal(),
        row.rect.right - text_left - 8.0,
    );
    FontManager::global().draw_str(
        painter,
        &label,
        Point::new(text_left, text_baseline(row.rect.center_y(), 13.0)),
        13.0,
        FontWeight::REGULAR,
        text_color,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_search_field(
    painter: Painter<'_>,
    theme: &SettingsTheme,
    rect: Rect,
    glyph: Option<&Image>,
    query: &str,
    composition: &str,
    focused: bool,
    caret: bool,
) {
    let radius = Radius::uniform(rect.height() / 2.0);
    painter.fill_round_rect(rect, radius, theme.field_bg);
    if focused {
        painter.stroke_round_rect(
            rect.inset(-1.0),
            Radius::uniform(rect.height() / 2.0 + 1.0),
            2.0,
            theme.accent.with_alpha_f(0.45),
        );
    }
    let icon_center = Point::new(rect.left + 15.0, rect.center_y());
    if let Some(glyph) = glyph {
        let size = 13.0;
        painter.draw_image(
            glyph,
            Rect::from_xywh(
                icon_center.x - size / 2.0,
                icon_center.y - size / 2.0,
                size,
                size,
            ),
            &ImageOptions::default()
                .with_sampling(Sampling::LinearLinear)
                .with_alpha_f(0.8),
        );
    }
    let text_left = rect.left + 28.0;
    let baseline = text_baseline(rect.center_y(), 13.0);
    let fonts = FontManager::global();
    let typed = format!("{query}{composition}");
    if typed.is_empty() {
        fonts.draw_str(
            painter,
            &winisland_core::i18n::tr("search_placeholder"),
            Point::new(text_left, baseline),
            13.0,
            FontWeight::REGULAR,
            theme.text_sec,
        );
    } else {
        let text = ellipsize_text(
            fonts,
            &typed,
            13.0,
            FontStyle::normal(),
            rect.right - text_left - 26.0,
        );
        fonts.draw_str(
            painter,
            &text,
            Point::new(text_left, baseline),
            13.0,
            FontWeight::REGULAR,
            theme.text_pri,
        );
        if !composition.is_empty() {
            let start = text_left + fonts.measure_text_cached(query, 13.0, FontStyle::normal());
            let end = (start + fonts.measure_text_cached(composition, 13.0, FontStyle::normal()))
                .min(rect.right - 26.0);
            painter.stroke_line(
                Point::new(start, baseline + 2.5),
                Point::new(end, baseline + 2.5),
                1.0,
                theme.text_pri,
                StrokeCap::Butt,
            );
        }
    }
    if caret {
        let width = fonts.measure_text_cached(&typed, 13.0, FontStyle::normal());
        let x = (text_left + width + 1.0).min(rect.right - 26.0);
        painter.stroke_line(
            Point::new(x, rect.center_y() - 7.0),
            Point::new(x, rect.center_y() + 7.0),
            1.2,
            theme.accent,
            StrokeCap::Butt,
        );
    }
    if !query.is_empty() {
        let center = search_clear_center(rect);
        painter.fill_circle(center, 7.0, theme.text_sec.with_alpha_f(0.55));
        for direction in [-1.0, 1.0] {
            painter.stroke_line(
                Point::new(center.x - 2.5, center.y - 2.5 * direction),
                Point::new(center.x + 2.5, center.y + 2.5 * direction),
                1.3,
                theme.win_bg,
                StrokeCap::Round,
            );
        }
    }
}

pub(crate) fn search_clear_center(rect: Rect) -> Point {
    Point::new(rect.right - 14.0, rect.center_y())
}

/// Draws a chevron pointing left or right, used by the navigation capsule.
pub(crate) fn draw_chevron(painter: Painter<'_>, center: Point, left: bool, color: Rgba) {
    let dx = if left { 2.5 } else { -2.5 };
    if let Some(path) = Path::from_svg(&format!(
        "M {} {} L {} {} L {} {}",
        center.x + dx,
        center.y - 5.5,
        center.x - dx,
        center.y,
        center.x + dx,
        center.y + 5.5,
    )) {
        painter.stroke_path(&path, 1.8, color, StrokeCap::Round, StrokeJoin::Round);
    }
}

/// Draws a soft drop shadow under an app-style icon tile occupying `rect`.
pub(crate) fn draw_icon_shadow(painter: Painter<'_>, rect: Rect, alpha: f32) {
    let size = rect.width().min(rect.height());
    painter.fill_path_blurred(
        &Path::continuous_rounded_rect(
            rect.offset(Vec2::new(0.0, size * 0.05)).inset(size * 0.03),
            size * 0.2,
        ),
        Rgba::from_argb((72.0 * alpha.clamp(0.0, 1.0)) as u8, 0, 0, 0),
        BlurSpec::uniform(size * 0.05 + 0.5),
    );
}
