mod background;
mod expanded;
mod mini;
pub(crate) mod multitask;

use std::cell::RefCell;
use std::collections::HashMap;

pub(crate) use mini::{
    LYRIC_WEIGHT as MINI_LYRIC_WEIGHT, lyric_font_size as mini_lyric_font_size,
    lyric_insets as mini_lyric_insets, lyric_pair_height as mini_lyric_pair_height,
};

use self::background::{BackgroundParams, draw_background};
use self::expanded::{ExpandedContentParams, draw_expanded_content};
use self::mini::{MiniContentParams, draw_mini_content};

use crate::core::smtc::MediaInfo;
use crate::ui::compact::CompactOverlay;
use crate::ui::expanded::music_view::{
    SpectrumFill, default_spectrum_fill, get_media_spectrum_fill,
};
use crate::ui::expanded::pager::{self, ExpandedPage, PagerParams};
use winisland_core::config::{
    CompactWidgetSlot, LyricTransitionAnimation, PluginWidgetSlot, WidgetSlot,
};
use winisland_core::lyrics::LyricHighlight;
use winisland_core::multitask::TaskFrame;
use winisland_plugin_host::draw::replay::PreparedFrame;
use winisland_plugin_host::host::PluginHost;
use winisland_render::text::FontManager;
use winisland_render::{
    Backdrop, BlurSpec, FontWeight, GradientStop, Image, LiquidMaterial, LiquidShape, Painter,
    Path, Point, RasterSurface, Rect, Rgba, TileMode, Vec2,
};

pub struct LayoutParams {
    pub pager_above: bool,
    pub pager_bar_hover: f32,
    pub pager_close_hover: f32,
    pub current_w: f32,
    pub current_h: f32,
    pub current_r: f32,
    pub sigmas: (f32, f32),
    pub shadow_static: bool,
    pub expansion_progress: f32,
    pub view_offset: f32,
    pub compact_scale: f32,
    pub expanded_scale: f32,
    pub hide_progress: f32,
    pub compact_widget_opacity: f32,
    pub island_x: f32,
    pub island_y: f32,
    pub stable_island_y: f32,
    pub base_h: f32,
}

pub struct MediaParams<'a> {
    pub media: &'a MediaInfo,
    pub backdrop: &'a MediaInfo,
    pub music_active: bool,
    pub available_controls: u32,
}

pub struct LyricsParams<'a> {
    pub current_lyric: &'a str,
    pub current_secondary_lyric: &'a str,
    pub old_lyric: &'a str,
    pub old_secondary_lyric: &'a str,
    pub lyric_highlight: Option<LyricHighlight>,
    pub lyric_transition: f32,
    pub lyric_transition_animation: LyricTransitionAnimation,
    pub lyric_scroll_offset: f32,
    pub lyric_side_gap: f32,
}

#[derive(Clone, Copy)]
pub struct LiquidOptics {
    pub refraction: f32,
    pub dispersion: f32,
    pub tint: f32,
}

impl LiquidOptics {
    pub(crate) fn apply(self, material: LiquidMaterial) -> LiquidMaterial {
        material
            .with_optics(self.refraction, self.dispersion)
            .with_tint(self.tint)
    }
}

pub struct StyleParams<'a> {
    pub island_style: &'a str,
    pub host_backdrop: bool,
    pub liquid_backdrop: Option<Backdrop>,
    pub liquid_optics: LiquidOptics,
    pub liquid_siri: bool,
    pub lingosoul: LingoSoulStyle,
    pub lingosoul_reply: &'a str,
    pub lingosoul_chat: Option<&'a crate::ui::lingosoul_chat::ChatPanel>,
    pub use_blur: bool,
    pub font_size: f32,
    pub music_text_y_offset: f32,
    pub music_text_line_spacing: f32,
    pub dt: f32,
    pub expanded_width: f32,
    pub expanded_height: f32,
    pub widget_layout: &'a [WidgetSlot],
    pub plugin_widget_layout: &'a [PluginWidgetSlot],
    pub plugin_widgets: &'a winisland_core::widgets::WidgetManager,
    pub plugin_frames: &'a HashMap<u64, PreparedFrame>,
    pub plugin_host: Option<&'a PluginHost>,
    pub compact_widget_layout: &'a [CompactWidgetSlot],
    pub pages: &'a [ExpandedPage],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LingoSoulStyle {
    pub drop: f32,
    pub level: f32,
    pub phase: f32,
    pub dots: f32,
    pub dots_alpha: f32,
    pub cover: f32,
    pub unit: f32,
    pub darken: f32,
    pub warmup: bool,
    pub reply: f32,
    pub beam: f32,
    pub panel: f32,
    pub wave_fade: f32,
    pub wave_intro: f32,
    pub wave_time: f32,
}

impl LingoSoulStyle {
    fn covers_content(&self) -> f32 {
        self.cover.max((self.drop * 4.0).min(1.0)).clamp(0.0, 1.0)
    }
}

pub(crate) use self::mini::MiniContent;

const MIN_VISIBLE_OPACITY: f32 = 0.01;
const MIN_BLUR_SIGMA: f32 = 0.1;
const MINI_FADE_RATE: f32 = 1.5;
const COLLAPSED_VISUALIZER_HEIGHT_SCALE: f32 = 0.45;
const BORDER_WIDTH: f32 = 1.0;
const SOLID_STYLE: &str = "default";
const LIQUID_STYLE: &str = "liquid";
const LIQUID_FUSION: f32 = 6.0;
const SOLID_BORDER_ALPHA: u8 = 30;
const EFFECT_BORDER_ALPHA: u8 = 40;

struct CachedShadow {
    key: (i32, i32, i32, i32, u8),
    image: Image,
}

thread_local! {
    static SHADOW_CACHE: RefCell<Option<CachedShadow>> = const { RefCell::new(None) };
}

pub struct DrawIslandParams<'a> {
    pub layout: LayoutParams,
    pub media: MediaParams<'a>,
    pub lyrics: LyricsParams<'a>,
    pub mini_content: Option<MiniContent<'a>>,
    pub compact_overlay: &'a CompactOverlay,
    pub style: StyleParams<'a>,
    pub attention_alpha: f32,
    pub surface: &'a multitask::IslandSurface,
    pub secondary_task: Option<TaskFrame<multitask::activities::Activity>>,
}

pub fn expanded_content_alpha(
    expansion_progress: f32,
    hide_progress: f32,
    compact_overlay_visible: bool,
) -> f32 {
    if compact_overlay_visible {
        0.0
    } else {
        expansion_progress.powi(2).clamp(0.0, 1.0) * (1.0 - hide_progress)
    }
}

pub fn draw_island(painter: Painter<'_>, params: DrawIslandParams<'_>) -> bool {
    let layout = &params.layout;
    let rect = Rect::from_xywh(
        layout.island_x,
        layout.island_y,
        layout.current_w,
        layout.current_h,
    );
    let island_path = &params.surface.primary;
    let blur_filter = if layout.sigmas.0 > MIN_BLUR_SIGMA || layout.sigmas.1 > MIN_BLUR_SIGMA {
        Some(BlurSpec {
            sigma: layout.sigmas,
            tile: None,
        })
    } else {
        None
    };
    if let Some(host) = params.style.plugin_host {
        host.runtime().extensions.begin_frame();
    }
    let (widget_animating, expanded_alpha) = match params.style.liquid_backdrop.as_ref() {
        Some(backdrop)
            if params.style.island_style == LIQUID_STYLE || params.style.lingosoul.drop > 0.0 =>
        {
            draw_liquid_island(painter, &params, rect, blur_filter, backdrop)
        }
        _ => {
            draw_expanded_shadow(painter, &params, island_path);
            draw_background_layer(painter, &params, rect, &params.surface.outline);
            draw_island_content(painter, &params, rect, blur_filter)
        }
    };
    if params.style.lingosoul.warmup
        && let Some(backdrop) = params.style.liquid_backdrop.as_ref()
    {
        warm_liquid_glass(painter, &params, rect, backdrop);
    }
    draw_island_border(painter, &params);
    multitask::draw_content(
        painter,
        params.surface,
        params.secondary_task,
        layout.compact_scale,
        params
            .style
            .plugin_host
            .map(|host| (host, params.style.plugin_frames)),
    );
    draw_pager(painter, &params, rect, expanded_alpha);
    if params.attention_alpha > 0.0 {
        let layout = &params.layout;
        let inset = -2.0 * layout.compact_scale;
        let outline = Path::continuous_rounded_rect(
            Rect::from_xywh(
                layout.island_x + inset,
                layout.island_y + inset,
                layout.current_w - 2.0 * inset,
                layout.current_h - 2.0 * inset,
            ),
            layout.current_r - inset,
        );
        painter.stroke_path(
            &outline,
            2.5 * layout.compact_scale,
            Rgba::from_argb(
                (220.0 * params.attention_alpha.clamp(0.0, 1.0)) as u8,
                255,
                207,
                45,
            ),
            winisland_render::StrokeCap::Butt,
            winisland_render::StrokeJoin::Miter,
        );
    }
    widget_animating
}

fn draw_expanded_shadow(painter: Painter<'_>, params: &DrawIslandParams<'_>, island_path: &Path) {
    let layout = &params.layout;
    let opacity = layout.expansion_progress.clamp(0.0, 1.0).powi(2)
        * (1.0 - layout.hide_progress).clamp(0.0, 1.0);
    if opacity <= MIN_VISIBLE_OPACITY {
        return;
    }
    let scale = layout.expanded_scale;
    let offset_y = 2.0 * scale;
    let bounds = island_path.bounds();
    let (surface_width, surface_height) = painter.surface_size();
    let margin = bounds
        .left
        .min(surface_width as f32 - bounds.right)
        .min(bounds.top + offset_y)
        .min(surface_height as f32 - bounds.bottom - offset_y)
        .max(0.0);
    let sigma = (3.0 * scale).min(margin / 3.0);
    let alpha = (28.0 * opacity) as u8;
    if layout.shadow_static && sigma > 0.0 {
        let key = (
            (bounds.width() * 16.0).round() as i32,
            (bounds.height() * 16.0).round() as i32,
            (layout.current_r * 16.0).round() as i32,
            (sigma * 16.0).round() as i32,
            alpha,
        );
        let cached = SHADOW_CACHE.with(|cell| {
            let mut cache = cell.borrow_mut();
            if cache.as_ref().is_none_or(|entry| entry.key != key) {
                let padding = (sigma * 3.0 + offset_y).ceil() as i32 + 2;
                let width = bounds.width().ceil() as i32 + padding * 2;
                let height = bounds.height().ceil() as i32 + padding * 2;
                let mut surface = RasterSurface::new(width, height)?;
                surface.clear(Rgba::TRANSPARENT);
                let raster = surface.painter();
                raster.translate(Vec2::new(
                    padding as f32 - bounds.left,
                    padding as f32 - bounds.top,
                ));
                raster.translate(Vec2::new(0.0, offset_y));
                raster.fill_path_blurred(
                    island_path,
                    Rgba::BLACK.with_alpha(alpha),
                    BlurSpec::uniform(sigma),
                );
                *cache = Some(CachedShadow {
                    key,
                    image: surface.snapshot(),
                });
            }
            cache.as_ref().map(|entry| entry.image.clone())
        });
        if let Some(image) = cached {
            let padding = (sigma * 3.0 + offset_y).ceil() + 2.0;
            painter.save();
            painter.clip_path_difference(island_path);
            painter.draw_image_at(
                &image,
                Point::new(bounds.left - padding, bounds.top - padding),
            );
            painter.restore();
            return;
        }
    }
    painter.save();
    painter.clip_path_difference(island_path);
    painter.translate(Vec2::new(0.0, offset_y));
    painter.fill_path_blurred(
        island_path,
        Rgba::BLACK.with_alpha(alpha),
        BlurSpec::uniform(sigma),
    );
    painter.restore();
}

fn draw_background_layer(
    painter: Painter<'_>,
    params: &DrawIslandParams<'_>,
    rect: Rect,
    island_path: &Path,
) {
    draw_background(BackgroundParams {
        painter,
        rect,
        island_path,
        island_style: params.style.island_style,
        host_backdrop: params.style.host_backdrop,
        media: params.media.backdrop,
    });
}

fn draw_expanded_layer(
    painter: Painter<'_>,
    blur_filter: Option<BlurSpec>,
    params: &DrawIslandParams<'_>,
    spectrum_fill: &SpectrumFill,
    alpha: f32,
    visualizer_height_scale: f32,
) -> bool {
    let layout = &params.layout;
    let media = &params.media;
    let style = &params.style;
    draw_expanded_content(ExpandedContentParams {
        painter,
        blur_filter,
        expanded_alpha: alpha,
        pages: style.pages,
        view_offset: layout.view_offset,
        current_w: layout.current_w,
        offset_x: layout.island_x,
        offset_y: layout.island_y,
        current_h: layout.current_h,
        media: media.media,
        music_active: media.music_active,
        available_controls: media.available_controls,
        global_scale: layout.expanded_scale,
        expansion_progress: layout.expansion_progress,
        viz_h_scale: visualizer_height_scale,
        use_blur: style.use_blur,
        font_size: style.font_size,
        music_text_y_offset: style.music_text_y_offset,
        music_text_line_spacing: style.music_text_line_spacing,
        dt: style.dt,
        expanded_width: style.expanded_width,
        expanded_height: style.expanded_height,
        text_color: Rgba::WHITE,
        text_color_sec: Rgba::WHITE,
        spectrum_fill,
        widget_layout: style.widget_layout,
        plugin_widget_layout: style.plugin_widget_layout,
        plugin_widgets: style.plugin_widgets,
        plugin_frames: style.plugin_frames,
        plugin_host: style.plugin_host,
    })
}

fn draw_compact_layer(
    painter: Painter<'_>,
    params: &DrawIslandParams<'_>,
    spectrum_fill: &SpectrumFill,
    alpha: f32,
    visualizer_height_scale: f32,
) {
    let layout = &params.layout;
    let lyrics = &params.lyrics;
    let style = &params.style;
    let has_mini_content = params.mini_content.is_some();
    let center_occupied = crate::ui::widget::compact::has_center_widget(
        style.compact_widget_layout,
        has_mini_content,
    );
    let visible_mini_content = if center_occupied {
        None
    } else {
        params.mini_content
    };
    let (left_extension, right_extension) = crate::ui::widget::compact::side_extensions(
        style.compact_widget_layout,
        center_occupied || has_mini_content,
        has_mini_content,
    );
    let (plugin_left, plugin_right) = crate::ui::plugin::compact_widths(style.plugin_host);
    let left_extension = (left_extension + plugin_left) * layout.compact_scale;
    let right_extension = (right_extension + plugin_right) * layout.compact_scale;
    draw_mini_content(MiniContentParams {
        painter,
        plugin_host: style.plugin_host,
        plugin_frames: style.plugin_frames,
        content: visible_mini_content,
        mini_alpha: alpha,
        current_w: (layout.current_w - left_extension - right_extension).max(0.0),
        global_scale: layout.compact_scale,
        media: params.media.media,
        offset_x: layout.island_x + left_extension,
        stable_offset_y: layout.stable_island_y,
        base_h: layout.base_h,
        spectrum_fill,
        viz_h_scale: visualizer_height_scale,
        current_lyric: lyrics.current_lyric,
        current_secondary_lyric: lyrics.current_secondary_lyric,
        old_lyric: lyrics.old_lyric,
        old_secondary_lyric: lyrics.old_secondary_lyric,
        lyric_highlight: lyrics.lyric_highlight,
        expansion_progress: layout.expansion_progress,
        font_size: style.font_size,
        lyric_scroll_offset: lyrics.lyric_scroll_offset,
        lyric_side_gap: lyrics.lyric_side_gap,
        lyric_transition: lyrics.lyric_transition,
        lyric_transition_animation: lyrics.lyric_transition_animation,
        text_color: Rgba::WHITE,
    });
    crate::ui::widget::compact::draw(
        painter,
        style.compact_widget_layout,
        Rect::from_xywh(
            layout.island_x + plugin_left * layout.compact_scale,
            layout.stable_island_y,
            (layout.current_w - (plugin_left + plugin_right) * layout.compact_scale).max(0.0),
            layout.base_h,
        ),
        layout.compact_scale,
        (alpha * layout.compact_widget_opacity * f32::from(u8::MAX)) as u8,
        has_mini_content,
    );
    if let Some(host) = style.plugin_host {
        crate::ui::plugin::draw_compact(
            painter,
            host,
            style.plugin_frames,
            Rect::from_xywh(
                layout.island_x,
                layout.stable_island_y,
                layout.current_w,
                layout.base_h,
            ),
            layout.compact_scale,
            (alpha * layout.compact_widget_opacity * 255.0) as u8,
        );
    }
}

fn draw_pager(painter: Painter<'_>, params: &DrawIslandParams<'_>, rect: Rect, alpha: f32) {
    let pages = params.style.pages;
    if alpha <= MIN_VISIBLE_OPACITY {
        return;
    }
    let backdrop = (params.style.island_style == "dynamic")
        .then(|| crate::utils::backdrop::get_blurred_cover_background(params.media.backdrop))
        .flatten();
    pager::draw(PagerParams {
        painter,
        island: rect,
        count: pages.len(),
        position: params.layout.view_offset,
        scale: params.layout.expanded_scale,
        above: params.layout.pager_above,
        alpha,
        bar_hover: params.layout.pager_bar_hover,
        close_hover: params.layout.pager_close_hover,
        host_blur: params.style.host_backdrop,
        backdrop: backdrop.as_ref(),
        liquid: params
            .style
            .liquid_backdrop
            .as_ref()
            .filter(|_| params.style.island_style == LIQUID_STYLE)
            .map(|backdrop| (backdrop, params.style.liquid_optics)),
    });
}

fn draw_island_border(painter: Painter<'_>, params: &DrawIslandParams<'_>) {
    if params.style.island_style == LIQUID_STYLE {
        return;
    }
    let layout = &params.layout;
    let alpha = if params.style.island_style == SOLID_STYLE {
        SOLID_BORDER_ALPHA
    } else {
        EFFECT_BORDER_ALPHA
    };
    let opacity = if params.style.island_style == SOLID_STYLE {
        1.0 - layout.expansion_progress.clamp(0.0, 1.0)
    } else {
        1.0
    };
    painter.save();
    painter.clip_path(&params.surface.outline);
    painter.stroke_path(
        &params.surface.outline,
        BORDER_WIDTH * 2.0,
        Rgba::WHITE
            .with_alpha((alpha as f32 * opacity * (1.0 - params.style.lingosoul.drop)) as u8),
        winisland_render::StrokeCap::Butt,
        winisland_render::StrokeJoin::Miter,
    );
    painter.restore();
}

fn draw_liquid_island(
    painter: Painter<'_>,
    params: &DrawIslandParams<'_>,
    rect: Rect,
    blur_filter: Option<BlurSpec>,
    backdrop: &Backdrop,
) -> (bool, f32) {
    let layout = &params.layout;
    let expansion = layout.expansion_progress.clamp(0.0, 1.0);
    let scale = layout.compact_scale + (layout.expanded_scale - layout.compact_scale) * expansion;
    let shape = LiquidShape {
        rect,
        radius: layout.current_r,
        secondary: params.surface.secondary,
        smoothing: LIQUID_FUSION * layout.compact_scale,
    };
    let mut material = params
        .style
        .liquid_optics
        .apply(LiquidMaterial::island(scale));
    if params.style.liquid_siri {
        material.black_fade = 1.0;
    }
    let lingosoul = params.style.lingosoul;
    if lingosoul.drop > 0.0 {
        apply_lingosoul_drop(&mut material, &lingosoul, scale);
    } else if lingosoul.cover > 0.0 {
        material.black_fade = material.black_fade.max(lingosoul.cover);
    }
    if !painter.draw_liquid_glass(backdrop, &shape, &material, 1.0, 1.0) {
        draw_background_layer(painter, params, rect, &params.surface.outline);
    }
    draw_island_content(painter, params, rect, blur_filter)
}

fn draw_island_content(
    painter: Painter<'_>,
    params: &DrawIslandParams<'_>,
    rect: Rect,
    blur_filter: Option<BlurSpec>,
) -> (bool, f32) {
    let layout = &params.layout;
    let lingosoul = params.style.lingosoul;
    let base = painter.save();
    painter.clip_path(&params.surface.primary);
    let cover = lingosoul.covers_content();
    if cover > 0.0 {
        painter.save_alpha(((1.0 - cover) * 255.0) as u8);
    }

    if let Some(host) = params.style.plugin_host {
        crate::ui::plugin::draw_layer(
            painter,
            host,
            params.style.plugin_frames,
            winisland_plugin_api::SURFACE_BACKGROUND,
            rect,
            layout.compact_scale,
            ((1.0 - layout.hide_progress) * 255.0) as u8,
        );
    }
    let compact_overlay_visible = params.compact_overlay.is_visible();
    let expanded_alpha = expanded_content_alpha(
        layout.expansion_progress,
        layout.hide_progress,
        compact_overlay_visible,
    );
    let mini_alpha = if compact_overlay_visible {
        0.0
    } else {
        (1.0 - layout.expansion_progress * MINI_FADE_RATE).clamp(0.0, 1.0)
            * (1.0 - layout.hide_progress)
    };
    let spectrum_fill = if expanded_alpha > MIN_VISIBLE_OPACITY || mini_alpha > MIN_VISIBLE_OPACITY
    {
        get_media_spectrum_fill(params.media.media)
    } else {
        default_spectrum_fill()
    };
    let visualizer_height_scale = COLLAPSED_VISUALIZER_HEIGHT_SCALE
        + (1.0 - COLLAPSED_VISUALIZER_HEIGHT_SCALE) * layout.expansion_progress;
    let widget_animating = draw_expanded_layer(
        painter,
        blur_filter,
        params,
        &spectrum_fill,
        expanded_alpha,
        visualizer_height_scale,
    );
    if compact_overlay_visible {
        params.compact_overlay.draw(
            painter,
            rect,
            layout.compact_scale,
            1.0 - layout.hide_progress,
        );
    } else {
        draw_compact_layer(
            painter,
            params,
            &spectrum_fill,
            mini_alpha,
            visualizer_height_scale,
        );
    }
    if let Some(host) = params.style.plugin_host {
        crate::ui::plugin::draw_layer(
            painter,
            host,
            params.style.plugin_frames,
            winisland_plugin_api::SURFACE_FOREGROUND,
            rect,
            layout.compact_scale,
            ((1.0 - layout.hide_progress) * 255.0) as u8,
        );
        host.runtime().extensions.finish_frame();
    }
    painter.restore_to(base);
    if lingosoul.dots_alpha > 0.0 {
        draw_lingosoul_dots(painter, rect, &lingosoul, 1.0 - layout.hide_progress);
    }
    if lingosoul.reply > 0.0 && !params.style.lingosoul_reply.is_empty() {
        draw_lingosoul_reply(
            painter,
            rect,
            params.style.lingosoul_reply,
            lingosoul.reply * (1.0 - layout.hide_progress),
            layout.expanded_scale,
            &lingosoul,
        );
    }
    if lingosoul.panel > 0.0
        && let Some(chat) = params.style.lingosoul_chat
    {
        chat.draw(
            painter,
            rect,
            layout.expanded_scale,
            lingosoul.panel * lingosoul.panel * (1.0 - layout.hide_progress),
            lingosoul.dots,
        );
    }
    (widget_animating, expanded_alpha)
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn apply_lingosoul_drop(material: &mut LiquidMaterial, lingosoul: &LingoSoulStyle, scale: f32) {
    let t = lingosoul.drop.clamp(0.0, 1.0);
    material.black_fade = material.black_fade.max(t).max(lingosoul.cover);
    material.bezel_width = lerp(material.bezel_width, 22.0 * scale, t);
    material.thickness = lerp(material.thickness, 18.0 * scale, t);
    material.displacement_factor = lerp(material.displacement_factor, 0.65, t);
    material.dispersion = lerp(material.dispersion, 0.025, t);
    material.tint = material
        .tint
        .with_alpha((f32::from(material.tint.a()) * (1.0 - 0.8 * t)) as u8);
    material.legibility *= 1.0 - 0.8 * t;
    material.light = Vec2::new(
        lerp(material.light.x, 0.0, t),
        lerp(material.light.y, -1.0, t),
    );
    material.specular_opacity *= 1.0 - t;
    material.stroke_opacity *= 1.0 - t;
    material.shadow_offset = Vec2::new(0.0, lerp(material.shadow_offset.y, 20.0 * scale, t));
    material.shadow_radius = lerp(material.shadow_radius, 28.0 * scale, t);
    material.shadow_alpha = lerp(material.shadow_alpha, 0.24, t);
    material.lingosoul = [
        lingosoul.wave_fade * t,
        0.0,
        lingosoul.level,
        t * (1.0 - lingosoul.darken),
    ];
    material.lingosoul_wave = [lingosoul.wave_intro, lingosoul.wave_time, 0.0, 0.0];
}

fn draw_lingosoul_dots(painter: Painter<'_>, rect: Rect, lingosoul: &LingoSoulStyle, opacity: f32) {
    const COUNT: usize = 7;
    let h = if lingosoul.unit > 0.0 {
        lingosoul.unit.min(rect.height())
    } else {
        rect.height()
    };
    let center = Point::new(rect.left + h * 0.5, rect.top + rect.height() * 0.5);
    let ring = h * 0.24;
    let alpha = lingosoul.dots_alpha * opacity.clamp(0.0, 1.0);
    let spin = 0.38 * (lingosoul.dots * 1.7).sin();
    for i in 0..COUNT {
        let angle =
            i as f32 / COUNT as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2 + spin;
        let wave = (lingosoul.dots * 7.0 - i as f32 * std::f32::consts::TAU / COUNT as f32).cos();
        let pulse = wave.max(0.0).powi(3);
        let reach = ring * (1.0 + 0.12 * pulse);
        let dot = Point::new(
            center.x + angle.cos() * reach,
            center.y + angle.sin() * reach,
        );
        let radius = h * (0.05 + 0.02 * pulse);
        let strength = alpha * (0.4 + 0.6 * pulse);
        let halo = radius * 1.6;
        painter.fill_path_blurred(
            &Path::rounded_rect(
                Rect::from_xywh(dot.x - halo, dot.y - halo, halo * 2.0, halo * 2.0),
                halo,
            ),
            Rgba::WHITE.with_alpha((255.0 * strength * 0.55) as u8),
            BlurSpec {
                sigma: (radius * 1.1, radius * 1.1),
                tile: None,
            },
        );
        painter.fill_circle(
            dot,
            radius,
            Rgba::WHITE.with_alpha((255.0 * strength) as u8),
        );
    }
}

fn warm_liquid_glass(
    painter: Painter<'_>,
    params: &DrawIslandParams<'_>,
    rect: Rect,
    backdrop: &Backdrop,
) {
    let scale = params.layout.compact_scale;
    let shape = LiquidShape {
        rect: Rect::from_xywh(rect.center_x() - 1.0, rect.center_y() - 1.0, 2.0, 2.0),
        radius: 1.0,
        secondary: None,
        smoothing: 0.0,
    };
    let mut material = params
        .style
        .liquid_optics
        .apply(LiquidMaterial::island(scale));
    apply_lingosoul_drop(&mut material, &params.style.lingosoul, scale);
    material.lingosoul[0] = 1.0;
    painter.draw_liquid_glass(backdrop, &shape, &material, 1.0, 1.0 / 255.0);
}

pub(crate) struct LingoSoulReplyLayout {
    pub lines: Vec<String>,
    pub size: f32,
    pub line_height: f32,
    pub padding: f32,
    pub height: f32,
}

pub(crate) fn lingosoul_reply_layout(text: &str, width: f32, scale: f32) -> LingoSoulReplyLayout {
    const MAX_LINES: usize = 6;
    let size = 15.0 * scale;
    let padding = 20.0 * scale;
    let line_height = size * 1.4;
    let mut lines = FontManager::global().wrap_text(
        text,
        size,
        FontWeight::MEDIUM,
        (width - padding * 2.0).max(size),
    );
    lines.truncate(MAX_LINES);
    let height = padding * 2.0 + line_height * lines.len().max(1) as f32;
    LingoSoulReplyLayout {
        lines,
        size,
        line_height,
        padding,
        height,
    }
}

fn draw_lingosoul_reply(
    painter: Painter<'_>,
    rect: Rect,
    text: &str,
    alpha: f32,
    scale: f32,
    lingosoul: &LingoSoulStyle,
) {
    let layout = lingosoul_reply_layout(text, rect.width(), scale);
    let alpha = alpha.clamp(0.0, 1.0);
    let blur = (1.0 - alpha) * 6.0 * scale;
    let beam = lingosoul.beam * alpha;
    let color =
        Rgba::WHITE.with_alpha((255.0 * alpha * (1.0 - 0.3 * beam * lingosoul.level)) as u8);
    painter.save();
    painter.clip_path(&Path::continuous_rounded_rect(
        rect,
        crate::utils::shape::expanded_island_radius(rect.width(), rect.height(), scale)
            .min(rect.height() * 0.36),
    ));
    if beam > 0.001 {
        draw_lingosoul_beam(painter, rect, lingosoul, beam);
    }
    for (i, line) in layout.lines.iter().enumerate() {
        FontManager::global().draw_text_cached(winisland_render::text::DrawTextCachedParams {
            painter,
            text: line,
            x: rect.left + layout.padding,
            y: rect.top
                + ((rect.height() - layout.height) * 0.5).max(0.0)
                + layout.padding
                + layout.line_height * i as f32
                + layout.size * 1.05,
            size: layout.size,
            weight: FontWeight::MEDIUM,
            color,
            blur: (blur > 0.3).then_some(BlurSpec {
                sigma: (blur, blur),
                tile: None,
            }),
        });
    }
    painter.restore();
}

fn draw_lingosoul_beam(
    painter: Painter<'_>,
    rect: Rect,
    lingosoul: &LingoSoulStyle,
    strength: f32,
) {
    const SLICES: usize = 96;
    let level = lingosoul.level.clamp(0.0, 1.0);
    let phase = lingosoul.phase;
    let h = rect.height();
    let w = rect.width();
    let radius = crate::utils::shape::expanded_island_radius(w, h, 1.0).min(h * 0.36);
    let dim = 0.6;
    let glow = 0.55 + 0.45 * level;
    let stop = |offset: f32, c: (f32, f32, f32), alpha: f32, fade: f32| GradientStop {
        offset,
        color: Rgba::from_argb(
            (255.0 * (alpha * fade * glow * strength * dim).clamp(0.0, 1.0)) as u8,
            c.0 as u8,
            c.1 as u8,
            c.2 as u8,
        ),
    };
    let slot = w / SLICES as f32;
    for i in 0..SLICES {
        let t = (i as f32 + 0.5) / SLICES as f32;
        let centre = 1.0 - (t * 2.0 - 1.0).powi(2) * 0.45;
        let band = 0.5
            + 0.5
                * (0.55 * (t * 7.0 + phase * 2.3).sin()
                    + 0.3 * (t * 12.5 - phase * 3.1).sin()
                    + 0.15 * (t * 21.0 + phase * 4.7).sin());
        let value = level * centre * (0.25 + 0.75 * band);
        let reach = h * (0.1 + 0.34 * value);
        let x = (rect.left + slot * i as f32).floor();
        let x_end = (rect.left + slot * (i + 1) as f32).floor();
        let side = (x + slot * 0.5 - rect.left).min(rect.right - x - slot * 0.5);
        let lift = if side < radius {
            let dx = radius - side.max(0.0);
            radius - (radius * radius - dx * dx).max(0.0).sqrt()
        } else {
            0.0
        };
        let base = rect.bottom - lift;
        let corner = (lift / radius.max(1.0)).clamp(0.0, 1.0);
        let top = base - reach * (1.0 - 0.75 * corner);
        let fade = 1.0 - 0.6 * corner;
        let stops = [
            stop(0.0, (255.0, 232.0, 210.0), 0.9, fade),
            stop(0.3, (236.0, 239.0, 255.0), 0.7, fade),
            stop(0.65, (150.0, 160.0, 255.0), 0.28, fade),
            stop(1.0, (150.0, 160.0, 255.0), 0.0, fade),
        ];
        painter.fill_rect_with_gradient(
            Rect::from_ltrb(x, top, x_end, rect.bottom),
            Point::new(x, base),
            Point::new(x, top),
            &stops,
            TileMode::Clamp,
        );
    }
}
