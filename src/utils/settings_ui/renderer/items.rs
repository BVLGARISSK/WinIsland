use winisland_render::FontStyle;
use winisland_render::FontWeight;
use winisland_render::Image;
use winisland_render::ImageOptions;
use winisland_render::Painter;
use winisland_render::Point;
use winisland_render::Radius;
use winisland_render::Rect;
use winisland_render::Rgba;
use winisland_render::Sampling;
use winisland_render::StrokeCap;
use winisland_render::StrokeJoin;
use winisland_render::text::FontManager;

use crate::utils::color::SettingsTheme;
use crate::utils::color::settings_color;

use super::super::items::{
    CONTENT_PADDING, GROUP_INNER_PAD, GROUP_RADIUS, POPUP_BTN_H, POPUP_BTN_R, POPUP_BTN_W,
    ROW_HEIGHT, SettingsItem, TEXT_FIELD_W, TOGGLE_H, TOGGLE_W, picker_button_rects, stepper_rects,
    trailing_control_rect,
};
use super::controls::{
    PillBtnParams, SettingsPainter, StepperParams, draw_pill_btn, draw_row_separator, draw_stepper,
    draw_switch, draw_updown_indicator, ellipsize_text, text_baseline,
};
use super::widget_preview::{WidgetPreviewParams, draw_widget_preview};
use super::{ActiveStepperValue, DrawItemsParams};

struct ItemCtx<'a> {
    painter: Painter<'a>,
    theme: &'a SettingsTheme,
    content_w: f32,
    width: f32,
    visible_min_y: f32,
    visible_max_y: f32,
    hover_pos: Option<(f32, f32)>,
}

impl ItemCtx<'_> {
    fn row_visible(&self, y: f32, height: f32) -> bool {
        y + height >= self.visible_min_y && y <= self.visible_max_y
    }

    fn hovered(&self, rect: Rect) -> bool {
        self.hover_pos
            .is_some_and(|(x, y)| rect.contains(Point::new(x, y)))
    }
}

#[derive(Default)]
struct GroupRows {
    in_group: bool,
    row_count: usize,
    current_row: usize,
}

fn row_text_color(ctx: &ItemCtx, enabled: bool) -> Rgba {
    settings_color(if enabled {
        ctx.theme.text_pri
    } else {
        ctx.theme.text_sec
    })
}

fn draw_row_text(ctx: &ItemCtx, y: f32, height: f32, label: &str, color: Rgba) -> (f32, bool) {
    let cy = y + height / 2.0;
    let visible = ctx.row_visible(y, height);
    if visible {
        FontManager::global().draw_str(
            ctx.painter,
            label,
            Point::new(CONTENT_PADDING + GROUP_INNER_PAD, text_baseline(cy, 13.0)),
            13.0,
            FontWeight::REGULAR,
            color,
        );
    }
    (cy, visible)
}

fn draw_row_stepper(
    ctx: &ItemCtx,
    y: f32,
    label: &str,
    value: &str,
    enabled: bool,
    active_stepper_value: &Option<ActiveStepperValue>,
    groups: &mut GroupRows,
) {
    let painter = ctx.painter;
    let theme = ctx.theme;
    let (cy, visible) = draw_row_text(ctx, y, ROW_HEIGHT, label, row_text_color(ctx, enabled));
    let (value_rect, increment, decrement) = stepper_rects(y, ROW_HEIGHT, ctx.content_w);
    if visible {
        let is_editing = active_stepper_value.as_ref().is_some_and(|input| {
            (input.rect.left - value_rect.left).abs() < 0.5
                && (input.rect.top - value_rect.top).abs() < 0.5
        });
        draw_stepper(StepperParams {
            painter,
            theme,
            value: value_rect,
            increment,
            decrement,
            hover: ctx.hover_pos,
            enabled,
            editing: is_editing,
        });
        let display_value = active_stepper_value
            .as_ref()
            .filter(|_| is_editing)
            .map(|input| input.text)
            .unwrap_or(value);
        let show_caret = active_stepper_value
            .as_ref()
            .is_some_and(|input| is_editing && input.show_caret);
        let fm = FontManager::global();
        let text_color = settings_color(if enabled {
            theme.text_pri
        } else {
            theme.text_sec
        });
        let value_w = fm.measure_text_cached(display_value, 13.0, FontStyle::normal());
        let value_x = value_rect.center_x() - value_w / 2.0;
        fm.draw_str(
            painter,
            display_value,
            Point::new(value_x, text_baseline(cy, 13.0)),
            13.0,
            FontWeight::REGULAR,
            text_color,
        );
        if show_caret {
            let caret_x = value_x + value_w + 1.5;
            painter.stroke_line(
                Point::new(caret_x, value_rect.top + 5.0),
                Point::new(caret_x, value_rect.bottom - 5.0),
                1.0,
                settings_color(theme.accent),
                StrokeCap::Butt,
            );
        }
    }

    advance_group_row(ctx, y + ROW_HEIGHT, groups, visible);
}

fn draw_row_switch(
    ctx: &ItemCtx,
    y: f32,
    label: &str,
    enabled: bool,
    switch_pos: f32,
    groups: &mut GroupRows,
) {
    let painter = ctx.painter;
    let theme = ctx.theme;
    let content_w = ctx.content_w;
    let (_, visible) = draw_row_text(ctx, y, ROW_HEIGHT, label, row_text_color(ctx, enabled));

    let toggle = trailing_control_rect(y, ROW_HEIGHT, content_w, TOGGLE_W, TOGGLE_H);
    if visible {
        draw_switch(painter, toggle.left, toggle.top, switch_pos, enabled, theme);
    }

    advance_group_row(ctx, y + ROW_HEIGHT, groups, visible);
}

fn draw_row_folder_picker(
    ctx: &ItemCtx,
    y: f32,
    labels: (&str, &str, &Option<String>),
    current_path: &Option<String>,
    enabled: bool,
    groups: &mut GroupRows,
) {
    let (label, btn_label, clear_label) = labels;
    let painter = ctx.painter;
    let theme = ctx.theme;
    let content_w = ctx.content_w;
    let fm = FontManager::global();
    let has_path = current_path.as_ref().is_some_and(|p| !p.is_empty());
    let row_h = if has_path { 64.0 } else { ROW_HEIGHT };
    let row_x = CONTENT_PADDING + GROUP_INNER_PAD;
    let label_y = if has_path { y + 22.0 } else { y + row_h / 2.0 };
    let visible = ctx.row_visible(y, row_h);

    if visible {
        fm.draw_str(
            painter,
            label,
            Point::new(row_x, text_baseline(label_y, 13.0)),
            13.0,
            FontWeight::REGULAR,
            row_text_color(ctx, enabled),
        );
        if let Some(path) = current_path
            && !path.is_empty()
        {
            let max_w = content_w - GROUP_INNER_PAD * 2.0 - 160.0;
            let display = ellipsize_text(fm, path, 11.0, FontStyle::normal(), max_w);
            fm.draw_str(
                painter,
                &display,
                Point::new(row_x, text_baseline(y + 42.0, 11.0)),
                11.0,
                FontWeight::REGULAR,
                settings_color(theme.text_sec),
            );
        }

        let (select, clear) = picker_button_rects(y, row_h, content_w);
        draw_pill_btn(PillBtnParams {
            painter,
            theme,
            rect: select,
            label: btn_label,
            text_color: row_text_color(ctx, enabled),
            hovered: enabled && ctx.hovered(select),
            enabled,
        });
        if let Some(clear_label) = clear_label {
            draw_pill_btn(PillBtnParams {
                painter,
                theme,
                rect: clear,
                label: clear_label,
                text_color: settings_color(if enabled {
                    theme.danger
                } else {
                    theme.text_sec
                }),
                hovered: enabled && ctx.hovered(clear),
                enabled,
            });
        }
    }

    advance_group_row(ctx, y + row_h, groups, visible);
}

fn draw_row_source_select(
    ctx: &ItemCtx,
    y: f32,
    label: &str,
    options: &[(String, bool)],
    enabled: bool,
    active_source_button: Option<Rect>,
    groups: &mut GroupRows,
) {
    let painter = ctx.painter;
    let theme = ctx.theme;
    let content_w = ctx.content_w;
    let fm = FontManager::global();
    let (cy, visible) = draw_row_text(ctx, y, ROW_HEIGHT, label, row_text_color(ctx, enabled));

    let selected_label = options
        .iter()
        .find(|(_, active)| *active)
        .map(|(l, _)| l.as_str())
        .unwrap_or("");

    let button = trailing_control_rect(y, ROW_HEIGHT, content_w, POPUP_BTN_W, POPUP_BTN_H);
    let is_open = active_source_button.is_some_and(|open| {
        (open.left - button.left).abs() < 0.5 && (open.top - button.top).abs() < 0.5
    });

    if visible {
        let indicator = Rect::from_xywh(button.right - 18.0, cy - 10.0, 18.0, 20.0);
        let hovered = ctx.hovered(button);
        draw_updown_indicator(painter, theme, indicator, is_open, hovered, enabled);
        let text_max = indicator.left - 8.0 - button.left;
        let text = ellipsize_text(fm, selected_label, 13.0, FontStyle::normal(), text_max);
        let text_w = fm.measure_text_cached(&text, 13.0, FontStyle::normal());
        fm.draw_str(
            painter,
            &text,
            Point::new(indicator.left - 8.0 - text_w, text_baseline(cy, 13.0)),
            13.0,
            FontWeight::REGULAR,
            settings_color(if enabled {
                theme.text_pri
            } else {
                theme.text_sec
            }),
        );
    }

    advance_group_row(ctx, y + ROW_HEIGHT, groups, visible);
}

fn draw_row_button(
    ctx: &ItemCtx,
    y: f32,
    label: &str,
    btn_label: &str,
    enabled: bool,
    groups: &mut GroupRows,
) {
    let painter = ctx.painter;
    let theme = ctx.theme;
    let content_w = ctx.content_w;
    let (_, visible) = draw_row_text(ctx, y, ROW_HEIGHT, label, row_text_color(ctx, enabled));
    if visible {
        let button = trailing_control_rect(y, ROW_HEIGHT, content_w, POPUP_BTN_W, POPUP_BTN_H);
        draw_pill_btn(PillBtnParams {
            painter,
            theme,
            rect: button,
            label: btn_label,
            text_color: row_text_color(ctx, enabled),
            hovered: enabled && ctx.hovered(button),
            enabled,
        });
    }

    advance_group_row(ctx, y + ROW_HEIGHT, groups, visible);
}

/// What a stored secret shows when not being edited: its last four characters.
fn masked(value: &str) -> String {
    let tail: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("••••••{tail}")
}

struct TextFieldRow<'a> {
    label: &'a str,
    value: &'a str,
    placeholder: &'a str,
    secret: bool,
    enabled: bool,
}

fn draw_row_text_field(
    ctx: &ItemCtx,
    y: f32,
    row: &TextFieldRow<'_>,
    active: &Option<ActiveStepperValue>,
    groups: &mut GroupRows,
) {
    let painter = ctx.painter;
    let theme = ctx.theme;
    let (cy, visible) = draw_row_text(
        ctx,
        y,
        ROW_HEIGHT,
        row.label,
        row_text_color(ctx, row.enabled),
    );
    if visible {
        let field = trailing_control_rect(y, ROW_HEIGHT, ctx.content_w, TEXT_FIELD_W, POPUP_BTN_H);
        let editing = active.as_ref().filter(|input| {
            (input.rect.left - field.left).abs() < 0.5 && (input.rect.top - field.top).abs() < 0.5
        });
        painter.fill_round_rect(
            field,
            Radius::uniform(POPUP_BTN_R),
            settings_color(theme.field_bg),
        );
        painter.stroke_round_rect(
            field,
            Radius::uniform(POPUP_BTN_R),
            1.0,
            settings_color(if editing.is_some() {
                theme.accent
            } else {
                theme.button_border
            }),
        );
        let (text, color) = match editing {
            Some(input) => (input.text.to_owned(), theme.text_pri),
            None if row.value.is_empty() => (row.placeholder.to_owned(), theme.text_sec),
            None if row.secret => (masked(row.value), theme.text_pri),
            None => (row.value.to_owned(), theme.text_pri),
        };
        let fm = FontManager::global();
        let inner = field.inset(8.0);
        let width = fm.measure_text_cached(&text, 12.0, FontStyle::normal());
        let shift = if editing.is_some() {
            (width - inner.width() + 4.0).max(0.0)
        } else {
            0.0
        };
        painter.save();
        painter.clip_rect(Rect::from_ltrb(
            inner.left,
            field.top,
            inner.right,
            field.bottom,
        ));
        let shown = if editing.is_some() {
            text
        } else {
            ellipsize_text(fm, &text, 12.0, FontStyle::normal(), inner.width())
        };
        fm.draw_str(
            painter,
            &shown,
            Point::new(inner.left - shift, text_baseline(cy, 12.0)),
            12.0,
            FontWeight::REGULAR,
            settings_color(if row.enabled { color } else { theme.text_sec }),
        );
        if editing.is_some_and(|input| input.show_caret) {
            let caret_x = inner.left - shift + width + 1.0;
            painter.stroke_line(
                Point::new(caret_x, field.top + 5.0),
                Point::new(caret_x, field.bottom - 5.0),
                1.0,
                settings_color(theme.accent),
                StrokeCap::Butt,
            );
        }
        painter.restore();
    }
    advance_group_row(ctx, y + ROW_HEIGHT, groups, visible);
}

fn draw_row_app_item(
    ctx: &ItemCtx,
    y: f32,
    label: &str,
    active: bool,
    enabled: bool,
    groups: &mut GroupRows,
) {
    let painter = ctx.painter;
    let theme = ctx.theme;
    let content_w = ctx.content_w;
    let fm = FontManager::global();
    let row_x = CONTENT_PADDING + GROUP_INNER_PAD;
    let cy = y + ROW_HEIGHT / 2.0;
    let visible = ctx.row_visible(y, ROW_HEIGHT);

    let check_size = 20.0;
    let check_x = CONTENT_PADDING + content_w - GROUP_INNER_PAD - check_size;
    let check_y = cy - check_size / 2.0;

    if visible && active && enabled {
        painter.fill_round_rect(
            Rect::from_xywh(check_x, check_y, check_size, check_size),
            Radius::uniform(5.0),
            settings_color(theme.accent),
        );
        let svg = format!(
            "M {} {} L {} {} L {} {}",
            check_x + 5.0,
            check_y + 10.0,
            check_x + 9.0,
            check_y + 14.0,
            check_x + 15.0,
            check_y + 6.0,
        );
        if let Some(path) = winisland_render::Path::from_svg(&svg) {
            painter.stroke_path(&path, 2.0, Rgba::WHITE, StrokeCap::Butt, StrokeJoin::Miter);
        }
    } else if visible {
        painter.stroke_round_rect(
            Rect::from_xywh(check_x, check_y, check_size, check_size),
            Radius::uniform(5.0),
            1.5,
            settings_color(if enabled {
                theme.card_highlight
            } else {
                theme.disabled
            }),
        );
    }

    if visible {
        let text_color = settings_color(if enabled {
            theme.text_pri
        } else {
            theme.text_sec
        });
        let max_label_w = check_x - row_x - 8.0;
        let display = ellipsize_text(fm, label, 13.0, FontStyle::normal(), max_label_w);
        SettingsPainter::new(painter).text(
            &display,
            (row_x, cy + 5.0),
            13.0,
            FontWeight::REGULAR,
            text_color,
        );
    }

    advance_group_row(ctx, y + ROW_HEIGHT, groups, visible);
}

fn draw_row_label(ctx: &ItemCtx, y: f32, label: &str, groups: &mut GroupRows) {
    let (_, visible) = draw_row_text(
        ctx,
        y,
        ROW_HEIGHT,
        label,
        settings_color(ctx.theme.text_sec),
    );
    advance_group_row(ctx, y + ROW_HEIGHT, groups, visible);
}

fn draw_center_image(ctx: &ItemCtx, y: f32, height: f32, image: &Image, size: f32) {
    if !ctx.row_visible(y, height) || image.width() <= 0 || image.height() <= 0 {
        return;
    }
    let scale = (size / image.width() as f32).min(size / image.height() as f32);
    let width = image.width() as f32 * scale;
    let image_height = image.height() as f32 * scale;
    let rect = Rect::from_xywh(
        (ctx.width - width) / 2.0,
        y + (height - image_height) / 2.0,
        width,
        image_height,
    );
    ctx.painter.draw_image(
        image,
        rect,
        &ImageOptions::default().with_sampling(Sampling::LinearLinear),
    );
}

fn draw_center_link(ctx: &ItemCtx, y: f32, height: f32, label: &str, color: Rgba) {
    if !ctx.row_visible(y, height) {
        return;
    }
    SettingsPainter::new(ctx.painter).centered_text(
        label,
        (ctx.width / 2.0, y + 24.0),
        13.0,
        FontWeight::REGULAR,
        color,
    );
}

fn draw_center_text(ctx: &ItemCtx, y: f32, height: f32, text: &str, size: f32, color: Rgba) {
    if !ctx.row_visible(y, height) {
        return;
    }
    SettingsPainter::new(ctx.painter).centered_text(
        text,
        (ctx.width / 2.0, y + 22.0),
        size,
        FontWeight::REGULAR,
        color,
    );
}

fn advance_group_row(ctx: &ItemCtx, sep_y: f32, groups: &mut GroupRows, visible: bool) {
    if !groups.in_group {
        return;
    }
    groups.current_row += 1;
    if groups.current_row < groups.row_count && visible {
        draw_row_separator(ctx.painter, ctx.theme, ctx.content_w, sep_y);
    }
}

pub fn content_height(items: &[SettingsItem], start_y: f32) -> f32 {
    let mut h = start_y;
    for item in items {
        h += item.height();
    }
    h
}

pub fn draw_items(params: DrawItemsParams<'_>) {
    let painter = params.painter;
    let items = params.items;
    let start_y = params.start_y;
    let width = params.width;
    let anims = params.anims;
    let theme = params.theme;
    let visible_min_y = params.visible_min_y;
    let visible_max_y = params.visible_max_y;
    let island_style = params.island_style;
    let expanded_width = params.expanded_width;
    let expanded_height = params.expanded_height;
    let base_width = params.base_width;
    let base_height = params.base_height;
    let widget_editor_mode = params.widget_editor_mode;
    let widget_layout = params.widget_layout;
    let plugin_widget_layout = params.plugin_widget_layout;
    let plugin_widgets = params.plugin_widgets;
    let plugin_frames = params.plugin_frames;
    let widget_dragging = params.widget_dragging;
    let widget_drag_hover_slot = params.widget_drag_hover_slot;
    let widget_preview_hover_slot = params.widget_preview_hover_slot;
    let compact_widget_layout = params.compact_widget_layout;
    let compact_widget_dragging = params.compact_widget_dragging;
    let widget_hover = params.widget_hover;
    let widget_hover_progress = params.widget_hover_progress;
    let widget_drop_animation = params.widget_drop_animation;
    let active_source_button = params.active_source_button;
    let active_stepper_value = params.active_stepper_value;

    let mut y = start_y;
    let mut switch_idx = 0;
    let mut groups = GroupRows::default();
    let content_w = width - CONTENT_PADDING * 2.0;
    let ctx = ItemCtx {
        painter,
        theme,
        content_w,
        width,
        visible_min_y,
        visible_max_y,
        hover_pos: params.hover_pos,
    };

    let mut i = 0;
    while i < items.len() {
        let item = &items[i];
        if y > visible_max_y + 120.0 {
            break;
        }
        match item {
            SettingsItem::SectionHeader { label } => {
                let h = item.height();
                if y + h >= visible_min_y && y <= visible_max_y {
                    FontManager::global().draw_str(
                        painter,
                        label,
                        Point::new(CONTENT_PADDING + 4.0, text_baseline(y + h - 13.0, 13.0)),
                        13.0,
                        FontWeight::SEMIBOLD,
                        settings_color(theme.text_pri),
                    );
                }
            }
            SettingsItem::GroupStart => {
                groups.in_group = true;
                groups.current_row = 0;
                let total_h = group_height_from(items, i + 1);
                groups.row_count = items[i + 1..]
                    .iter()
                    .take_while(|item| !matches!(item, SettingsItem::GroupEnd))
                    .filter(|item| item.is_row())
                    .count();
                if y + total_h >= visible_min_y && y <= visible_max_y {
                    let group = Rect::from_xywh(CONTENT_PADDING, y, content_w, total_h);
                    painter.fill_path(
                        &winisland_render::Path::continuous_rounded_rect(group, GROUP_RADIUS),
                        settings_color(theme.group_bg),
                    );
                    painter.stroke_path(
                        &winisland_render::Path::continuous_rounded_rect(
                            group.inset(0.375),
                            GROUP_RADIUS - 0.375,
                        ),
                        0.75,
                        settings_color(theme.group_border),
                        StrokeCap::Butt,
                        StrokeJoin::Round,
                    );
                }
            }
            SettingsItem::GroupEnd => {
                groups.in_group = false;
            }
            SettingsItem::RowStepper {
                label,
                value,
                enabled,
            } => {
                draw_row_stepper(
                    &ctx,
                    y,
                    label,
                    value,
                    *enabled,
                    &active_stepper_value,
                    &mut groups,
                );
            }
            SettingsItem::RowSwitch {
                label,
                on: _,
                enabled,
            } => {
                draw_row_switch(&ctx, y, label, *enabled, anims.get(switch_idx), &mut groups);
                switch_idx += 1;
            }
            SettingsItem::RowFolderPicker {
                label,
                btn_label,
                clear_label,
                current_path,
                enabled,
            } => {
                draw_row_folder_picker(
                    &ctx,
                    y,
                    (label, btn_label, clear_label),
                    current_path,
                    *enabled,
                    &mut groups,
                );
            }
            SettingsItem::RowSourceSelect {
                label,
                options,
                enabled,
            } => {
                draw_row_source_select(
                    &ctx,
                    y,
                    label,
                    options,
                    *enabled,
                    active_source_button,
                    &mut groups,
                );
            }
            SettingsItem::RowButton {
                label,
                btn_label,
                enabled,
            } => {
                draw_row_button(&ctx, y, label, btn_label, *enabled, &mut groups);
            }
            SettingsItem::RowAppItem {
                label,
                active,
                enabled,
            } => {
                draw_row_app_item(&ctx, y, label, *active, *enabled, &mut groups);
            }
            SettingsItem::RowLabel { label } => {
                draw_row_label(&ctx, y, label, &mut groups);
            }
            SettingsItem::RowText {
                label,
                value,
                placeholder,
                secret,
                enabled,
            } => {
                draw_row_text_field(
                    &ctx,
                    y,
                    &TextFieldRow {
                        label,
                        value,
                        placeholder,
                        secret: *secret,
                        enabled: *enabled,
                    },
                    &active_stepper_value,
                    &mut groups,
                );
            }
            SettingsItem::CenterLink { label, color } => {
                draw_center_link(&ctx, y, item.height(), label, *color);
            }
            SettingsItem::CenterText { text, size, color } => {
                draw_center_text(&ctx, y, item.height(), text, *size, *color);
            }
            SettingsItem::CenterImage {
                image,
                size,
                height,
            } => {
                draw_center_image(&ctx, y, *height, image, *size);
            }
            SettingsItem::Spacer { .. } => {}
            SettingsItem::Custom { .. } => {}
            SettingsItem::WidgetPreview { .. } => {
                draw_widget_preview(WidgetPreviewParams {
                    painter,
                    item_y: y,
                    width,
                    content_width: content_w,
                    visible_min_y,
                    visible_max_y,
                    island_style,
                    expanded_width,
                    expanded_height,
                    base_width,
                    base_height,
                    widget_editor_mode,
                    widget_layout,
                    plugin_widget_layout,
                    plugin_widgets,
                    plugin_frames,
                    widget_dragging,
                    widget_drag_hover_slot,
                    widget_preview_hover_slot,
                    compact_widget_layout,
                    compact_widget_dragging,
                    widget_hover,
                    widget_hover_progress,
                    widget_drop_animation,
                    theme,
                });
            }
        }
        y += item.height();
        i += 1;
    }
}

fn group_height_from(items: &[SettingsItem], start: usize) -> f32 {
    let mut h = 0.0;
    for item in &items[start..] {
        if matches!(item, SettingsItem::GroupEnd) {
            break;
        }
        h += item.height();
    }
    h
}
