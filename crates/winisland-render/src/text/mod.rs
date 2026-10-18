//! Cached font measurement and drawing for the rendering layer.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use skia_safe::font_arguments::{VariationPosition, variation_position::Coordinate};
use skia_safe::font_style::{Slant as SkSlant, Width as SkWidth};
use skia_safe::{
    Font, FontArguments, FontMgr, FontStyle as SkFontStyle, Paint, Path, Typeface, image_filters,
    paint,
};

use crate::painter::Painter;
use crate::types::{BlurSpec, FontStyle, FontWeight, Rgba};

mod plugin;
pub use plugin::{PluginTextMetrics, PluginTextParams, PluginTextRun};

static GLOBAL_FONT_MANAGER: OnceLock<FontManager> = OnceLock::new();

#[derive(Clone)]
struct Face {
    typeface: Typeface,
    synthetic: f32,
}

type TextGroup = (String, Face, f32);
type TextGroups = Vec<TextGroup>;
type TextCacheValue = (f32, TextGroups);
type TextCacheMap = HashMap<u64, TextCacheValue>;
type TextPathCacheMap = HashMap<u64, Vec<(Path, f32)>>;
type TextPrefixCacheMap = HashMap<u64, Vec<(usize, f32)>>;

fn text_paint(color: Rgba, blur: Option<BlurSpec>) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(crate::convert::to_skia_color(color));
    if let Some(BlurSpec { sigma, tile }) = blur
        && let Some(filter) = image_filters::blur(
            sigma,
            tile.map(crate::convert::to_skia_tile_mode),
            None,
            None,
        )
    {
        paint.set_image_filter(filter);
    }
    paint
}

fn emboldened_paint(paint: &Paint, stroke_width: f32) -> Paint {
    let mut paint = paint.clone();
    paint.set_style(paint::Style::StrokeAndFill);
    paint.set_stroke_width(stroke_width);
    paint.set_stroke_join(paint::Join::Round);
    paint
}

pub struct DrawTextCachedParams<'a> {
    pub painter: Painter<'a>,
    pub text: &'a str,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub weight: FontWeight,
    pub color: Rgba,
    pub blur: Option<BlurSpec>,
}

pub struct FontSelection<'a> {
    pub family: Option<&'a str>,
    pub file: Option<&'a str>,
    pub weight_offset: i16,
    pub locale: &'a str,
}

pub struct FontManager;

struct FontChoice {
    family: Option<String>,
    file: Option<String>,
    weight_offset: i16,
    locale: String,
    user: Option<Option<Typeface>>,
}

thread_local! {
    static FONT_MGR: FontMgr = FontMgr::new();
    static SYSTEM_TYPEFACE: Typeface = system_typeface();
    static FONT_CHOICE: RefCell<FontChoice> = const { RefCell::new(FontChoice {
        family: None,
        file: None,
        weight_offset: 0,
        locale: String::new(),
        user: None,
    }) };
    static FACE_CACHE: RefCell<HashMap<(u32, u16), Face>> = RefCell::new(HashMap::new());
    static FALLBACK_CACHE: RefCell<HashMap<(char, u16), Face>> = RefCell::new(HashMap::new());
    static TEXT_CACHE: RefCell<TextCacheMap> = RefCell::new(HashMap::new());
    static TEXT_PATH_CACHE: RefCell<TextPathCacheMap> = RefCell::new(HashMap::new());
    static TEXT_PREFIX_CACHE: RefCell<TextPrefixCacheMap> = RefCell::new(HashMap::new());
    static TEXT_WRAP_CACHE: RefCell<HashMap<u64, Vec<String>>> = RefCell::new(HashMap::new());
}

const SYSTEM_FAMILIES: [&str; 3] = ["Segoe UI Variable Text", "Segoe UI", "Microsoft YaHei"];
const FALLBACK_FAMILIES: [&str; 4] = [
    "Segoe UI Emoji",
    "Microsoft YaHei",
    "Segoe UI Symbol",
    "Segoe UI",
];
const DEFAULT_FALLBACK_LOCALE: &str = "zh-CN";
const FALLBACK_CACHE_LIMIT: usize = 2000;
const TEXT_CACHE_LIMIT: usize = 500;
const TEXT_PATH_CACHE_LIMIT: usize = 100;
const TEXT_PREFIX_CACHE_LIMIT: usize = 100;
const TEXT_WRAP_CACHE_LIMIT: usize = 32;
const NO_LINE_START: &str = ",.;:!?)%，。、；：！？）」』》";
const TEXT_PREFIX_WIDTH_LIMIT: usize = 256;

fn evict_one_if_full<K, V>(cache: &mut HashMap<K, V>, limit: usize)
where
    K: Clone + std::cmp::Eq + std::hash::Hash,
{
    if cache.len() >= limit
        && let Some(key) = cache.keys().next().cloned()
    {
        cache.remove(&key);
    }
}

fn breaks_anywhere(character: char) -> bool {
    matches!(
        character,
        '\u{2E80}'..='\u{9FFF}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FF00}'..='\u{FFEF}'
    )
}

fn break_tokens(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    for character in text.chars() {
        let previous = tokens.last().and_then(|token| token.chars().last());
        let attaches = previous.is_some_and(|previous| {
            !previous.is_whitespace()
                && (NO_LINE_START.contains(character)
                    || (!character.is_whitespace()
                        && !breaks_anywhere(character)
                        && !breaks_anywhere(previous)))
        });
        match tokens.last_mut() {
            Some(token) if attaches => token.push(character),
            _ => tokens.push(character.to_string()),
        }
    }
    tokens
}

fn hash_cache_key(text: &str, style: FontStyle, size: f32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    style.cache_key().hash(&mut hasher);
    ((size * 100.0).round() as i32).hash(&mut hasher);
    hasher.finish()
}

fn system_typeface() -> Typeface {
    FONT_MGR.with(|mgr| {
        SYSTEM_FAMILIES
            .into_iter()
            .find_map(|family| mgr.match_family_style(family, SkFontStyle::normal()))
            .unwrap_or_else(|| {
                mgr.legacy_make_typeface(None, SkFontStyle::normal())
                    .unwrap()
            })
    })
}

fn load_user_typeface(family: Option<&str>, file: Option<&str>) -> Option<Typeface> {
    if let Some(path) = file {
        let data = std::fs::read(path).ok()?;
        return FONT_MGR.with(|mgr| mgr.new_from_data(&data, None));
    }
    let family = family?;
    FONT_MGR.with(|mgr| mgr.match_family_style(family, SkFontStyle::normal()))
}

fn user_typeface() -> Option<Typeface> {
    FONT_CHOICE.with(|choice| {
        let mut choice = choice.borrow_mut();
        if choice.user.is_none() {
            let typeface = load_user_typeface(choice.family.as_deref(), choice.file.as_deref());
            choice.user = Some(typeface);
        }
        choice.user.clone().flatten()
    })
}

fn is_font_file(typeface: &Typeface) -> bool {
    FONT_CHOICE.with(|choice| {
        let choice = choice.borrow();
        choice.file.is_some()
            && choice
                .user
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|user| user.unique_id() == typeface.unique_id())
    })
}

fn primary_typeface() -> Typeface {
    user_typeface().unwrap_or_else(|| SYSTEM_TYPEFACE.with(Typeface::clone))
}

fn effective_weight(style: FontStyle) -> u16 {
    let offset = FONT_CHOICE.with(|choice| choice.borrow().weight_offset);
    (i32::from(style.weight().value()) + i32::from(offset)).clamp(100, 900) as u16
}

fn weight_axis(typeface: &Typeface) -> Option<(f32, f32)> {
    typeface
        .variation_design_parameters()?
        .into_iter()
        .find(|axis| axis.tag == Coordinate::wght)
        .map(|axis| (axis.min, axis.max))
}

/// Stroke width, as a fraction of the font size, that stands in for weight a family lacks.
fn synthetic_amount(typeface: &Typeface, missing: f32) -> f32 {
    if missing < 40.0 || typeface.family_name().contains("Emoji") {
        0.0
    } else {
        (0.005 + missing / 10_000.0).min(0.04)
    }
}

/// Picks the heaviest face of the family that does not exceed `weight`.
fn closest_face(base: &Typeface, weight: u16) -> Typeface {
    let family = base.family_name();
    let target = i32::from(weight);
    FONT_MGR
        .with(|mgr| {
            let mut set = mgr.match_family(&family);
            let mut below: Option<(i32, usize)> = None;
            let mut above: Option<(i32, usize)> = None;
            for index in 0..set.count() {
                let (style, _) = set.style(index);
                if style.slant() != SkSlant::Upright || style.width() != SkWidth::NORMAL {
                    continue;
                }
                let face_weight = *style.weight();
                if face_weight <= target {
                    if below.is_none_or(|(best, _)| face_weight > best) {
                        below = Some((face_weight, index));
                    }
                } else if above.is_none_or(|(best, _)| face_weight < best) {
                    above = Some((face_weight, index));
                }
            }
            below
                .or(above)
                .and_then(|(_, index)| set.new_typeface(index))
        })
        .unwrap_or_else(|| base.clone())
}

fn resolve_face(base: &Typeface, weight: u16) -> Face {
    let key = (base.unique_id(), weight);
    if let Some(face) = FACE_CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return face;
    }
    let variable = weight_axis(base).and_then(|(min, max)| {
        let target = f32::from(weight).clamp(min, max);
        let coordinates = [Coordinate {
            axis: Coordinate::wght,
            value: target,
        }];
        let arguments = FontArguments::new().set_variation_design_position(VariationPosition {
            coordinates: &coordinates,
        });
        base.clone_with_arguments(&arguments).map(|typeface| Face {
            synthetic: synthetic_amount(base, f32::from(weight) - target),
            typeface,
        })
    });
    let face = variable.unwrap_or_else(|| {
        let typeface = if is_font_file(base) {
            base.clone()
        } else {
            closest_face(base, weight)
        };
        let missing = f32::from(weight) - *typeface.font_style().weight() as f32;
        Face {
            synthetic: synthetic_amount(&typeface, missing),
            typeface,
        }
    });
    FACE_CACHE.with(|cache| cache.borrow_mut().insert(key, face.clone()));
    face
}

fn make_font(face: &Face, size: f32) -> Font {
    let mut font = Font::from_typeface(face.typeface.clone(), size);
    font.set_subpixel(true);
    font
}

fn synthetic_stroke(face: &Face, size: f32) -> f32 {
    size * face.synthetic * (18.0 / size).clamp(1.0, 1.4)
}

fn face_paint(paint: &Paint, face: &Face, size: f32) -> Option<Paint> {
    (face.synthetic > 0.0).then(|| emboldened_paint(paint, synthetic_stroke(face, size)))
}

fn measure_group(text: &str, face: &Face, size: f32) -> f32 {
    make_font(face, size).measure_str(text, None).0
}

fn typeface_supports_char(typeface: &Typeface, character: char) -> bool {
    let mut glyphs = [0u16; 1];
    typeface.unichars_to_glyphs(&[character as i32], &mut glyphs);
    glyphs[0] != 0
}

fn typeface_supports_text(typeface: &Typeface, text: &str) -> bool {
    let characters: Vec<i32> = text.chars().map(|character| character as i32).collect();
    let mut glyphs = vec![0u16; characters.len()];
    typeface.unichars_to_glyphs(&characters, &mut glyphs);
    glyphs.iter().all(|glyph| *glyph != 0)
}

fn fallback_typeface(c: char) -> Typeface {
    let locale = FONT_CHOICE.with(|choice| choice.borrow().locale.clone());
    let locale = if locale.is_empty() {
        DEFAULT_FALLBACK_LOCALE
    } else {
        locale.as_str()
    };
    FONT_MGR.with(|mgr| {
        mgr.match_family_style_character("", SkFontStyle::normal(), &[locale], c as i32)
            .filter(|tf| typeface_supports_char(tf, c))
            .or_else(|| {
                FALLBACK_FAMILIES.into_iter().find_map(|family| {
                    mgr.match_family_style(family, SkFontStyle::normal())
                        .filter(|tf| typeface_supports_char(tf, c))
                })
            })
            .unwrap_or_else(|| {
                mgr.legacy_make_typeface(None, SkFontStyle::normal())
                    .unwrap()
            })
    })
}

fn face_for_char(c: char, weight: u16) -> Face {
    FALLBACK_CACHE.with(|cache| {
        if let Some(face) = cache.borrow().get(&(c, weight)) {
            return face.clone();
        }
        let primary = primary_typeface();
        let base = if typeface_supports_char(&primary, c) {
            primary
        } else {
            fallback_typeface(c)
        };
        let face = resolve_face(&base, weight);
        let mut cache = cache.borrow_mut();
        evict_one_if_full(&mut cache, FALLBACK_CACHE_LIMIT);
        cache.insert((c, weight), face.clone());
        face
    })
}

fn inherits_previous_typeface(c: char) -> bool {
    matches!(c, '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}')
        || ('\u{E0100}'..='\u{E01EF}').contains(&c)
        || ('\u{0300}'..='\u{036F}').contains(&c)
}

fn is_ascii_text(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii())
}

fn same_face(a: &Face, b: &Face) -> bool {
    a.typeface.unique_id() == b.typeface.unique_id() && a.synthetic == b.synthetic
}

/// Compute text groups and total width.
/// Falls back to a single typeface for ASCII-only text to skip per-char lookups.
fn compute_text_groups(text: &str, size: f32, style: FontStyle) -> (f32, TextGroups) {
    let weight = effective_weight(style);
    let primary = primary_typeface();
    if is_ascii_text(text) && typeface_supports_text(&primary, text) {
        let face = resolve_face(&primary, weight);
        let width = measure_group(text, &face, size);
        return (width, vec![(text.to_string(), face, width)]);
    }

    let mut current_w = 0.0;
    let mut groups: TextGroups = Vec::new();
    let mut current_group = String::new();
    let mut last_face: Option<Face> = None;
    for c in text.chars() {
        let face = match &last_face {
            Some(face) if inherits_previous_typeface(c) => face.clone(),
            _ => face_for_char(c, weight),
        };
        if let Some(last) = &last_face
            && !same_face(last, &face)
        {
            let group_text = std::mem::take(&mut current_group);
            let width = measure_group(&group_text, last, size);
            current_w += width;
            groups.push((group_text, last.clone(), width));
        }
        last_face = Some(face);
        current_group.push(c);
    }
    if let Some(last) = last_face {
        let width = measure_group(&current_group, &last, size);
        current_w += width;
        groups.push((current_group, last, width));
    }

    (current_w, groups)
}

fn compute_text_paths(text: &str, size: f32, style: FontStyle) -> Vec<(Path, f32)> {
    let (_, groups) = compute_text_groups(text, size, style);
    let mut x = 0.0;
    groups
        .into_iter()
        .map(|(text, face, width)| {
            let font = make_font(&face, size);
            let path = skia_safe::utils::text_utils::get_path(text.as_str(), (x, 0.0), &font);
            x += width;
            (path, synthetic_stroke(&face, size))
        })
        .collect()
}

fn with_text_groups<R>(
    text: &str,
    size: f32,
    style: FontStyle,
    f: impl FnOnce(&TextCacheValue) -> R,
) -> R {
    let cache_key = hash_cache_key(text, style, size);
    TEXT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(&cache_key) {
            evict_one_if_full(&mut cache, TEXT_CACHE_LIMIT);
        }
        let entry = cache
            .entry(cache_key)
            .or_insert_with(|| compute_text_groups(text, size, style));
        f(entry)
    })
}

impl FontManager {
    pub fn global() -> &'static FontManager {
        GLOBAL_FONT_MANAGER.get_or_init(|| FontManager)
    }

    /// Selects the interface font: a font file wins over an installed family,
    /// `weight_offset` shifts every requested weight, and `locale` (BCP 47) picks the
    /// regional glyphs of fallback fonts.
    pub fn configure(&self, selection: FontSelection<'_>) {
        FONT_CHOICE.with(|choice| {
            let mut choice = choice.borrow_mut();
            choice.family = selection.family.map(str::to_owned);
            choice.file = selection.file.map(str::to_owned);
            choice.weight_offset = selection.weight_offset;
            choice.locale = selection.locale.to_owned();
            choice.user = None;
        });
        FACE_CACHE.with(|cache| cache.borrow_mut().clear());
        FALLBACK_CACHE.with(|cache| cache.borrow_mut().clear());
        self.clear_text_caches();
    }

    /// Family name of the font chosen through [`Self::configure`], when it loaded.
    pub fn custom_family_name(&self) -> Option<String> {
        user_typeface().map(|typeface| typeface.family_name())
    }

    pub fn has_family(&self, family: &str) -> bool {
        FONT_MGR.with(|mgr| mgr.match_family(family).count() > 0)
    }

    pub fn installed_families(&self) -> Vec<String> {
        let mut families: Vec<String> = FONT_MGR.with(|mgr| {
            mgr.family_names()
                .filter(|family| !family.is_empty() && !family.starts_with('@'))
                .collect()
        });
        families.sort_by_key(|family| family.to_lowercase());
        families.dedup();
        families
    }

    pub fn clear_text_caches(&self) {
        TEXT_CACHE.with(|cache| cache.borrow_mut().clear());
        TEXT_PATH_CACHE.with(|cache| cache.borrow_mut().clear());
        TEXT_PREFIX_CACHE.with(|cache| cache.borrow_mut().clear());
        TEXT_WRAP_CACHE.with(|cache| cache.borrow_mut().clear());
    }

    pub fn measure_str(&self, text: &str, size: f32, weight: FontWeight) -> crate::types::Rect {
        let paint = text_paint(Rgba::WHITE, None);
        with_text_groups(text, size, weight.into(), |(_, groups)| {
            let mut x = 0.0;
            let mut bounds: Option<skia_safe::Rect> = None;
            for (text, face, width) in groups {
                let synthetic = face_paint(&paint, face, size);
                let (_, rect) = make_font(face, size).measure_str(text, synthetic.as_ref());
                let rect = rect.with_offset((x, 0.0));
                bounds = Some(bounds.map_or(rect, |bounds| skia_safe::Rect::join2(bounds, rect)));
                x += width;
            }
            let bounds = bounds.unwrap_or_default();
            crate::types::Rect::from_ltrb(bounds.left, bounds.top, bounds.right, bounds.bottom)
        })
    }

    pub fn ascent(&self, size: f32, weight: FontWeight) -> f32 {
        let face = resolve_face(&primary_typeface(), effective_weight(weight.into()));
        let (_, metrics) = make_font(&face, size).metrics();
        metrics.ascent
    }

    pub fn draw_str(
        &self,
        painter: Painter<'_>,
        text: &str,
        at: crate::types::Point,
        size: f32,
        weight: FontWeight,
        color: Rgba,
    ) {
        self.draw_text_cached(DrawTextCachedParams {
            painter,
            text,
            x: at.x,
            y: at.y,
            size,
            weight,
            color,
            blur: None,
        });
    }

    pub fn wrap_text(
        &self,
        text: &str,
        size: f32,
        style: impl Into<FontStyle>,
        max_width: f32,
    ) -> Vec<String> {
        let style = style.into();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hash_cache_key(text, style, size).hash(&mut hasher);
        max_width.to_bits().hash(&mut hasher);
        let cache_key = hasher.finish();
        if let Some(lines) = TEXT_WRAP_CACHE.with(|cache| cache.borrow().get(&cache_key).cloned()) {
            return lines;
        }
        let mut lines = Vec::new();
        let mut line = String::new();
        for token in break_tokens(text) {
            let candidate = format!("{line}{token}");
            if !line.trim().is_empty()
                && self.measure_text_cached(&candidate, size, style) > max_width
            {
                lines.push(line.trim_end().to_string());
                line = token.trim_start().to_string();
            } else {
                line = candidate;
            }
        }
        if !line.trim().is_empty() {
            lines.push(line.trim_end().to_string());
        }
        TEXT_WRAP_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            evict_one_if_full(&mut cache, TEXT_WRAP_CACHE_LIMIT);
            cache.insert(cache_key, lines.clone());
        });
        lines
    }

    pub fn measure_text_cached(&self, text: &str, size: f32, style: impl Into<FontStyle>) -> f32 {
        with_text_groups(text, size, style.into(), |(width, _)| *width)
    }

    pub fn measure_text_prefixes_cached(
        &self,
        text: &str,
        first_end: usize,
        second_end: usize,
        size: f32,
        style: impl Into<FontStyle>,
    ) -> (f32, f32) {
        let style = style.into();
        let cache_key = hash_cache_key(text, style, size);
        TEXT_PREFIX_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if !cache.contains_key(&cache_key) {
                evict_one_if_full(&mut cache, TEXT_PREFIX_CACHE_LIMIT);
            }
            let widths = cache.entry(cache_key).or_default();
            let mut width_at = |end: usize| {
                if end == 0 {
                    return 0.0;
                }
                if let Some((_, width)) = widths.iter().find(|(cached_end, _)| *cached_end == end) {
                    return *width;
                }
                let width = compute_text_groups(&text[..end], size, style).0;
                if widths.len() >= TEXT_PREFIX_WIDTH_LIMIT {
                    widths.remove(0);
                }
                widths.push((end, width));
                width
            };
            (width_at(first_end), width_at(second_end))
        })
    }

    pub fn draw_text_cached(&self, params: DrawTextCachedParams<'_>) {
        let paint = text_paint(params.color, params.blur);
        with_text_groups(
            params.text,
            params.size,
            params.weight.into(),
            |(_, groups)| {
                let canvas = params.painter.canvas();
                let mut x = params.x;
                for (text, face, width) in groups {
                    let font = make_font(face, params.size);
                    let synthetic = face_paint(&paint, face, params.size);
                    canvas.draw_str(
                        &**text,
                        (x, params.y),
                        &font,
                        synthetic.as_ref().unwrap_or(&paint),
                    );
                    x += *width;
                }
            },
        );
    }

    pub fn draw_text_as_paths_cached(&self, params: DrawTextCachedParams<'_>) {
        let style = FontStyle::from(params.weight);
        let cache_key = hash_cache_key(params.text, style, params.size);
        let paint = text_paint(params.color, params.blur);
        TEXT_PATH_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if !cache.contains_key(&cache_key) {
                evict_one_if_full(&mut cache, TEXT_PATH_CACHE_LIMIT);
            }
            let paths = cache
                .entry(cache_key)
                .or_insert_with(|| compute_text_paths(params.text, params.size, style));
            let canvas = params.painter.canvas();
            canvas.save();
            canvas.translate((params.x, params.y));
            for (path, stroke_width) in paths.iter() {
                if *stroke_width > 0.0 {
                    canvas.draw_path(path, &emboldened_paint(&paint, *stroke_width));
                } else {
                    canvas.draw_path(path, &paint);
                }
            }
            canvas.restore();
        });
    }
}
