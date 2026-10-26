use std::cell::RefCell;
use std::collections::HashMap;
use winisland_render::FontWeight;
use winisland_render::{Image, Mipmapped, Painter, Point, Rect, Rgba};

use crate::utils::color::SettingsTheme;
use crate::utils::settings_ui::chrome::{
    SidebarRow, draw_search_field, draw_sidebar_row, draw_traffic_lights, search_rect,
};
use crate::utils::settings_ui::items::SIDEBAR_PAD;
use crate::utils::settings_ui::text_baseline;
use winisland_core::i18n::tr;
use winisland_render::DrawingContext;
use winisland_render::text::FontManager;

use super::{
    BUILTIN_SIDEBAR_PAGE_COUNT, BuiltinPage, SIDEBAR_KEY_BASE, SIDEBAR_ROW_GAP, SIDEBAR_ROW_H,
    SIDEBAR_START_Y, SIDEBAR_W, SettingsApp,
};

const SIDEBAR_GROUP_GAP: f32 = 12.0;
const SIDEBAR_ICON_BYTES: [&[u8]; BUILTIN_SIDEBAR_PAGE_COUNT] = [
    include_bytes!("../../../resources/in_app/settings/settings.png"),
    include_bytes!("../../../resources/in_app/settings/appearance.png"),
    include_bytes!("../../../resources/in_app/settings/layout.png"),
    include_bytes!("../../../resources/in_app/settings/behavior.png"),
    include_bytes!("../../../resources/in_app/settings/alerts.png"),
    include_bytes!("../../../resources/in_app/settings/music.png"),
    include_bytes!("../../../resources/in_app/settings/widget.png"),
    include_bytes!("../../../resources/in_app/settings/plugin.png"),
    include_bytes!("../../../resources/in_app/settings/lingosoul.png"),
    include_bytes!("../../../resources/in_app/settings/about.png"),
];

thread_local! {
    static SIDEBAR_ICONS: RefCell<Option<Vec<Image>>> = const { RefCell::new(None) };
    static PLUGIN_SETTINGS_ICONS: RefCell<HashMap<u64, Image>> = RefCell::new(HashMap::new());
    static SEARCH_GLYPH: RefCell<Option<Option<Image>>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SidebarEntry {
    Page(usize),
    Result(usize),
}

fn load_sidebar_icon(drawing_context: &mut DrawingContext<'_>, bytes: &[u8]) -> Image {
    let image = Image::from_encoded(bytes).expect("Failed to load sidebar icon");
    drawing_context
        .prepare_image(image, Mipmapped::Yes)
        .expect("Failed to create mipmapped sidebar icon texture")
}

fn sidebar_icon(drawing_context: &mut DrawingContext<'_>, index: usize) -> Option<Image> {
    SIDEBAR_ICONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let icons = cache.get_or_insert_with(|| {
            SIDEBAR_ICON_BYTES
                .iter()
                .map(|bytes| load_sidebar_icon(drawing_context, bytes))
                .collect()
        });
        icons.get(index).cloned()
    })
}

fn search_glyph() -> Option<Image> {
    SEARCH_GLYPH.with(|cache| {
        cache
            .borrow_mut()
            .get_or_insert_with(|| {
                crate::icons::load_tinted_glyph(
                    include_bytes!("../../../resources/in_app/icons/magnifyingglass.png"),
                    Rgba::from_rgb(128, 128, 134),
                )
                .map(|(image, _)| image)
            })
            .clone()
    })
}

pub(super) fn clear_sidebar_icon_cache() {
    SIDEBAR_ICONS.with(|cache| {
        *cache.borrow_mut() = None;
    });
}

pub(super) fn clear_plugin_settings_icon_cache() {
    PLUGIN_SETTINGS_ICONS.with(|cache| cache.borrow_mut().clear());
}

fn plugin_settings_icon(
    drawing_context: &mut DrawingContext<'_>,
    page: &winisland_core::plugin_settings::PluginSettingsPage,
) -> Option<Image> {
    if page.icon.is_empty() {
        return sidebar_icon(drawing_context, BuiltinPage::Plugins as usize);
    }
    PLUGIN_SETTINGS_ICONS
        .with(|cache| {
            if let Some(image) = cache.borrow().get(&page.resource_id) {
                return Some(image.clone());
            }
            let image = Image::from_encoded(&page.icon)?;
            let image = drawing_context.prepare_image(image, Mipmapped::Yes)?;
            cache.borrow_mut().insert(page.resource_id, image.clone());
            Some(image)
        })
        .or_else(|| sidebar_icon(drawing_context, BuiltinPage::Plugins as usize))
}

impl SettingsApp {
    pub(crate) fn search_rect(&self) -> Rect {
        search_rect(SIDEBAR_W, SIDEBAR_PAD)
    }

    /// Lays out the sidebar: grouped pages normally, or search results while searching.
    pub(crate) fn sidebar_rows(&self) -> Vec<(SidebarEntry, Rect)> {
        let row =
            |y: f32| Rect::from_xywh(SIDEBAR_PAD, y, SIDEBAR_W - SIDEBAR_PAD * 2.0, SIDEBAR_ROW_H);
        let pitch = SIDEBAR_ROW_H + SIDEBAR_ROW_GAP;
        let mut y = SIDEBAR_START_Y;
        let mut rows = Vec::new();
        if self.search.active() {
            for index in 0..self.search.results.len() {
                rows.push((SidebarEntry::Result(index), row(y)));
                y += pitch;
            }
            return rows;
        }
        let plugin_pages: Vec<usize> = (0..self.plugin_settings_pages.len())
            .map(|offset| BUILTIN_SIDEBAR_PAGE_COUNT + offset)
            .collect();
        let groups: [&[usize]; 4] = [
            &[
                BuiltinPage::General as usize,
                BuiltinPage::Appearance as usize,
                BuiltinPage::Layout as usize,
                BuiltinPage::Behavior as usize,
                BuiltinPage::Alerts as usize,
            ],
            &[
                BuiltinPage::Music as usize,
                BuiltinPage::Widgets as usize,
                BuiltinPage::Plugins as usize,
                BuiltinPage::LingoSoul as usize,
            ],
            &plugin_pages,
            &[BuiltinPage::About as usize],
        ];
        for group in groups.iter().filter(|group| !group.is_empty()) {
            for page in group.iter() {
                rows.push((SidebarEntry::Page(*page), row(y)));
                y += pitch;
            }
            y += SIDEBAR_GROUP_GAP;
        }
        rows
    }

    pub(crate) fn sidebar_entry_at(&self, x: f32, y: f32) -> Option<SidebarEntry> {
        let point = Point::new(x, y);
        self.sidebar_rows()
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(entry, _)| entry)
    }

    fn sidebar_entry_icon(
        &self,
        drawing_context: &mut DrawingContext<'_>,
        page: usize,
    ) -> Option<Image> {
        if page < BUILTIN_SIDEBAR_PAGE_COUNT {
            sidebar_icon(drawing_context, page)
        } else {
            self.plugin_settings_pages
                .get(page - BUILTIN_SIDEBAR_PAGE_COUNT)
                .and_then(|plugin_page| plugin_settings_icon(drawing_context, plugin_page))
        }
    }

    fn sidebar_page_label(&self, page: usize) -> String {
        match BuiltinPage::from_index(page) {
            Some(page) => tr(page.title_key()),
            None => self
                .plugin_settings_pages
                .get(page - BUILTIN_SIDEBAR_PAGE_COUNT)
                .map(|page| page.title.clone())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn draw_sidebar(
        &self,
        drawing_context: &mut DrawingContext<'_>,
        painter: Painter<'_>,
        theme: &SettingsTheme,
    ) {
        draw_traffic_lights(
            painter,
            self.is_light,
            self.focused,
            self.dots_hovered,
            theme.text_sec,
        );
        draw_search_field(
            painter,
            theme,
            self.search_rect(),
            search_glyph().as_ref(),
            &self.search.query,
            &self.search.composition,
            self.search.focused,
            self.search.focused && self.frame_count % 60 < 30,
        );

        let rows = self.sidebar_rows();
        for (position, (entry, rect)) in rows.iter().enumerate() {
            let (page, label, selected) = match *entry {
                SidebarEntry::Page(page) => (
                    page,
                    self.sidebar_page_label(page),
                    self.active_page == page,
                ),
                SidebarEntry::Result(index) => {
                    let result = &self.search.results[index];
                    (result.page, result.label.clone(), false)
                }
            };
            let icon = self.sidebar_entry_icon(drawing_context, page);
            draw_sidebar_row(
                painter,
                theme,
                SidebarRow {
                    rect: *rect,
                    icon: icon.as_ref(),
                    label: &label,
                    selected,
                    focused: self.focused,
                    hover: self.anim.get(SIDEBAR_KEY_BASE + position as u64),
                },
            );
        }
        if self.search.active() && rows.is_empty() {
            let text = tr("search_no_results");
            let width = FontManager::global().measure_text_cached(
                &text,
                13.0,
                winisland_render::FontStyle::normal(),
            );
            FontManager::global().draw_str(
                painter,
                &text,
                Point::new(
                    SIDEBAR_W / 2.0 - width / 2.0,
                    text_baseline(SIDEBAR_START_Y + 20.0, 13.0),
                ),
                13.0,
                FontWeight::REGULAR,
                theme.text_sec,
            );
        }
    }
}
