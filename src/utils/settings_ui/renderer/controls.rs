use crate::utils::color::SettingsTheme;
use crate::utils::color::settings_color;
use winisland_render::FontStyle;
use winisland_render::FontWeight;
use winisland_render::Painter;
use winisland_render::Point;
use winisland_render::Radius;
use winisland_render::Rect;
use winisland_render::Rgba;
use winisland_render::StrokeCap;
use winisland_render::text::{DrawTextCachedParams, FontManager};

use super::super::items::{
    CONTENT_PADDING, GROUP_INNER_PAD, POPUP_BTN_R, TOGGLE_H, TOGGLE_INSET, TOGGLE_KNOB_W, TOGGLE_W,
};

pub(super) struct PillBtnParams<'a> {
    pub(super) painter: Painter<'a>,
    pub(super) theme: &'a SettingsTheme,
    pub(super) rect: Rect,
    pub(super) label: &'a str,
    pub(super) text_color: Rgba,
    pub(super) hovered: bool,
    pub(super) enabled: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct SettingsPainter<'a> {
    painter: Painter<'a>,
}

impl<'a> SettingsPainter<'a> {
    pub(crate) fn new(painter: Painter<'a>) -> Self {
        Self { painter }
    }

    pub(crate) fn text(
        self,
        text: &str,
        position: (f32, f32),
        size: f32,
        weight: FontWeight,
        color: impl Into<Rgba>,
    ) {
        FontManager::global().draw_text_cached(DrawTextCachedParams {
            painter: self.painter,
            text,
            x: position.0,
            y: position.1,
            size,
            weight,
            color: color.into(),
            blur: None,
        });
    }

    pub(crate) fn centered_text(
        self,
        text: &str,
        position: (f32, f32),
        size: f32,
        weight: FontWeight,
        color: impl Into<Rgba>,
    ) {
        let width = FontManager::global().measure_text_cached(text, size, weight);
        self.text(
            text,
            (position.0 - width / 2.0, position.1),
            size,
            weight,
            color,
        );
    }
}

pub(super) fn draw_row_separator(
    painter: Painter<'_>,
    theme: &SettingsTheme,
    content_w: f32,
    sep_y: f32,
) {
    let row_x = CONTENT_PADDING + GROUP_INNER_PAD;
    painter.stroke_line(
        Point::new(row_x, sep_y),
        Point::new(CONTENT_PADDING + content_w - GROUP_INNER_PAD, sep_y),
        0.5,
        settings_color(theme.separator),
        StrokeCap::Butt,
    );
}

pub(crate) fn draw_switch(
    painter: Painter<'_>,
    x: f32,
    y: f32,
    pos: f32,
    enabled: bool,
    theme: &SettingsTheme,
) {
    let (off_color, on_color) = if enabled {
        (theme.toggle_off, theme.toggle_on)
    } else {
        (theme.toggle_off, theme.toggle_off)
    };
    let r = off_color.r() as f32 + (on_color.r() as f32 - off_color.r() as f32) * pos;
    let g = off_color.g() as f32 + (on_color.g() as f32 - off_color.g() as f32) * pos;
    let b = off_color.b() as f32 + (on_color.b() as f32 - off_color.b() as f32) * pos;
    let track = Rect::from_xywh(x, y, TOGGLE_W, TOGGLE_H);
    painter.fill_round_rect(
        track,
        Radius::uniform(TOGGLE_H / 2.0),
        Rgba::from_rgb(r as u8, g as u8, b as u8),
    );
    painter.stroke_round_rect(
        track.inset(0.25),
        Radius::uniform(TOGGLE_H / 2.0 - 0.25),
        0.5,
        settings_color(theme.control_border),
    );

    let knob_h = TOGGLE_H - TOGGLE_INSET * 2.0;
    let knob = Rect::from_xywh(
        x + TOGGLE_INSET + pos * (TOGGLE_W - TOGGLE_KNOB_W - TOGGLE_INSET * 2.0),
        y + TOGGLE_INSET,
        TOGGLE_KNOB_W,
        knob_h,
    );
    painter.fill_path_blurred(
        &winisland_render::Path::rounded_rect(
            knob.offset(winisland_render::Vec2::new(0.0, 0.5)),
            knob_h / 2.0,
        ),
        Rgba::from_argb(46, 0, 0, 0),
        winisland_render::BlurSpec::uniform(0.8),
    );
    painter.fill_round_rect(knob, Radius::uniform(knob_h / 2.0), Rgba::WHITE);
}

pub(super) fn draw_push_button(
    painter: Painter<'_>,
    theme: &SettingsTheme,
    rect: Rect,
    hovered: bool,
    enabled: bool,
) {
    let radius = Radius::uniform(POPUP_BTN_R);
    if enabled {
        painter.fill_path_blurred(
            &winisland_render::Path::rounded_rect(
                rect.offset(winisland_render::Vec2::new(0.0, 0.5)),
                POPUP_BTN_R,
            ),
            theme.button_shadow,
            winisland_render::BlurSpec::uniform(1.0),
        );
    }
    painter.fill_round_rect(
        rect,
        radius,
        if !enabled {
            theme.control_disabled
        } else if hovered {
            theme.control_hover
        } else {
            theme.button_bg
        },
    );
    painter.stroke_round_rect(rect.inset(0.25), radius, 0.5, theme.button_border);
}

pub(super) fn draw_button_label(painter: Painter<'_>, rect: Rect, label: &str, color: Rgba) {
    let fm = FontManager::global();
    let width = fm.measure_text_cached(label, 13.0, FontStyle::normal());
    let label = if width > rect.width() - 12.0 {
        ellipsize_text(fm, label, 13.0, FontStyle::normal(), rect.width() - 12.0)
    } else {
        label.to_string()
    };
    let width = fm.measure_text_cached(&label, 13.0, FontStyle::normal());
    fm.draw_str(
        painter,
        &label,
        Point::new(
            rect.center_x() - width / 2.0,
            text_baseline(rect.center_y(), 13.0),
        ),
        13.0,
        FontWeight::REGULAR,
        color,
    );
}

pub(super) fn draw_pill_btn(params: PillBtnParams<'_>) {
    draw_push_button(
        params.painter,
        params.theme,
        params.rect,
        params.hovered,
        params.enabled,
    );
    draw_button_label(params.painter, params.rect, params.label, params.text_color);
}

fn draw_small_chevron(painter: Painter<'_>, center: Point, up: bool, color: Rgba) {
    let dy = if up { 1.5 } else { -1.5 };
    if let Some(path) = winisland_render::Path::from_svg(&format!(
        "M {} {} L {} {} L {} {}",
        center.x - 3.0,
        center.y + dy,
        center.x,
        center.y - dy,
        center.x + 3.0,
        center.y + dy,
    )) {
        painter.stroke_path(
            &path,
            1.4,
            color,
            StrokeCap::Round,
            winisland_render::StrokeJoin::Round,
        );
    }
}

/// Draws the macOS style double chevron indicator used by pop-up buttons.
pub(super) fn draw_updown_indicator(
    painter: Painter<'_>,
    theme: &SettingsTheme,
    rect: Rect,
    active: bool,
    hovered: bool,
    enabled: bool,
) {
    let fill = if active {
        theme.accent
    } else if hovered && enabled {
        theme.control_hover
    } else {
        theme.control_bg
    };
    painter.fill_round_rect(rect, Radius::uniform(5.0), fill);
    let color = if active {
        Rgba::WHITE
    } else if enabled {
        theme.text_pri
    } else {
        theme.disabled
    };
    draw_small_chevron(
        painter,
        Point::new(rect.center_x(), rect.center_y() - 3.0),
        true,
        color,
    );
    draw_small_chevron(
        painter,
        Point::new(rect.center_x(), rect.center_y() + 3.0),
        false,
        color,
    );
}

pub(super) struct StepperParams<'a> {
    pub(super) painter: Painter<'a>,
    pub(super) theme: &'a SettingsTheme,
    pub(super) value: Rect,
    pub(super) increment: Rect,
    pub(super) decrement: Rect,
    pub(super) hover: Option<(f32, f32)>,
    pub(super) enabled: bool,
    pub(super) editing: bool,
}

/// Draws the value field and the stacked arrow buttons of a stepper.
pub(super) fn draw_stepper(params: StepperParams<'_>) {
    let StepperParams {
        painter,
        theme,
        value,
        increment,
        decrement,
        hover,
        enabled,
        editing,
    } = params;
    draw_push_button(painter, theme, value, false, enabled);
    if editing {
        painter.stroke_round_rect(
            value.inset(-1.0),
            Radius::uniform(POPUP_BTN_R + 1.0),
            2.0,
            theme.accent.with_alpha_f(0.6),
        );
    }
    let arrows = Rect::from_ltrb(
        increment.left,
        increment.top,
        increment.right,
        decrement.bottom,
    );
    draw_push_button(painter, theme, arrows, false, enabled);
    let hovered = |rect: Rect| hover.is_some_and(|(x, y)| rect.contains(Point::new(x, y)));
    for (rect, up) in [(increment, true), (decrement, false)] {
        if enabled && hovered(rect) {
            painter.save();
            painter.clip_rect(rect);
            painter.fill_round_rect(arrows, Radius::uniform(POPUP_BTN_R), theme.sidebar_hover);
            painter.restore();
        }
        draw_small_chevron(
            painter,
            Point::new(
                rect.center_x(),
                rect.center_y() + if up { 0.5 } else { -0.5 },
            ),
            up,
            if enabled {
                theme.text_pri
            } else {
                theme.disabled
            },
        );
    }
    painter.stroke_line(
        Point::new(arrows.left + 3.0, arrows.center_y()),
        Point::new(arrows.right - 3.0, arrows.center_y()),
        0.5,
        theme.separator,
        StrokeCap::Butt,
    );
}

pub(crate) fn ellipsize_text(
    fm: &FontManager,
    text: &str,
    size: f32,
    style: impl Into<FontStyle>,
    max_width: f32,
) -> String {
    let style = style.into();
    if fm.measure_text_cached(text, size, style) <= max_width {
        return text.to_string();
    }
    let ellipsis = "…";
    let ellipsis_width = fm.measure_text_cached(ellipsis, size, style);
    let mut fitted = String::new();
    for character in text.chars() {
        fitted.push(character);
        if fm.measure_text_cached(&fitted, size, style) + ellipsis_width > max_width {
            fitted.pop();
            break;
        }
    }
    fitted.push_str(ellipsis);
    fitted
}

/// Returns the baseline that vertically centers digits and CJK text of `size` on `center_y`.
pub(crate) fn text_baseline(center_y: f32, size: f32) -> f32 {
    let bounds = FontManager::global().measure_str("0", size, FontWeight::REGULAR);
    center_y - bounds.height() / 2.0 - bounds.top
}
