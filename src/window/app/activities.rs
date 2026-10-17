use std::time::Instant;

use winisland_core::context::{Activity, ActivityContent, Priority};

use crate::ui::compact::timer::{TimerContent, countdown_width};
use crate::ui::expanded::pager::ExpandedPage;
use crate::ui::island::MiniContent;

use super::App;

const MUSIC_ACTIVITY: u64 = 1;
const TIMER_ACTIVITY: u64 = 2;

impl App {
    pub(super) fn resize_activity_window(&self) {
        if let Some(window) = self.window {
            let size = self.required_window_size();
            if window.inner_size() != size {
                let _ = window.request_inner_size(size);
            }
        }
    }

    pub(super) fn update_local_activities(&mut self, music_active: bool, playing: bool) -> bool {
        let mut changed = Self::sync_local_activity(
            &mut self.activity_mgr,
            MUSIC_ACTIVITY,
            music_active,
            if playing {
                Priority::High
            } else {
                Priority::Low
            },
            playing,
            0.0,
        );
        let timer = crate::ui::expanded::timer_view::compact_content();
        let display = |timer: TimerContent| (timer.remaining_secs(), timer.total, timer.paused);
        changed |= self.timer_content.map(display) != timer.map(display);
        self.timer_content = timer;
        let plugin_widths = crate::ui::plugin::compact_widths(self.plugin_host.as_deref());
        let shares_island = self
            .activity_mgr
            .current_excluding(TIMER_ACTIVITY)
            .is_some()
            || self
                .config
                .compact_widget_layout
                .iter()
                .any(|slot| slot.widget.is_some())
            || plugin_widths.0 + plugin_widths.1 > 0.0;
        let timer_orb = timer.is_some() && shares_island;
        changed |= self.timer_orb != timer_orb;
        self.timer_orb = timer_orb;
        changed |= Self::sync_local_activity(
            &mut self.activity_mgr,
            TIMER_ACTIVITY,
            timer.is_some() && !timer_orb,
            Priority::Medium,
            true,
            timer.map_or(0.0, |timer| countdown_width(timer, self.config.base_width)),
        );
        changed
    }

    fn sync_local_activity(
        manager: &mut winisland_core::context::ActivityManager,
        id: u64,
        enabled: bool,
        priority: Priority,
        keep_visible: bool,
        preferred_width: f32,
    ) -> bool {
        if !enabled {
            return manager.remove(id);
        }
        let updated_at = manager
            .get(id)
            .filter(|item| item.priority == priority)
            .map_or_else(Instant::now, |item| item.updated_at);
        manager.upsert(Activity {
            id,
            priority,
            content: ActivityContent::Local,
            show_compact: true,
            keep_visible,
            preferred_width,
            preferred_height: 0.0,
            page: None,
            expires_at: None,
            updated_at,
        })
    }

    pub(super) fn mini_content(&self) -> Option<MiniContent<'_>> {
        Self::mini_content_for(&self.activity_mgr, self.timer_content)
    }

    pub(super) fn mini_content_for(
        manager: &winisland_core::context::ActivityManager,
        timer: Option<TimerContent>,
    ) -> Option<MiniContent<'_>> {
        let activity = manager.current()?;
        match &activity.content {
            ActivityContent::Local if activity.id == MUSIC_ACTIVITY => Some(MiniContent::Music),
            ActivityContent::Local if activity.id == TIMER_ACTIVITY => {
                timer.map(MiniContent::Timer)
            }
            ActivityContent::Local => None,
            ActivityContent::Text(text) => Some(MiniContent::Text(text)),
            ActivityContent::Surface(id) => Some(MiniContent::Surface(*id)),
        }
    }

    pub(super) fn activity_page(&self) -> Option<ExpandedPage> {
        let activity = self.activity_mgr.current()?;
        let page = match activity.content {
            ActivityContent::Local if activity.id == MUSIC_ACTIVITY => ExpandedPage::Music,
            ActivityContent::Local if activity.id == TIMER_ACTIVITY => ExpandedPage::Timer,
            _ => ExpandedPage::Plugin(activity.page?),
        };
        self.expanded_pages().contains(&page).then_some(page)
    }
}
