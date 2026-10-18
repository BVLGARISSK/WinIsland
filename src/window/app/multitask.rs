use std::time::{Duration, Instant};

use winisland_core::multitask::{Presentation, Task, TaskId};
use winisland_platform::DeviceActivity;

use crate::platform::WindowRef;

use super::App;

const SETTLE_SPEED: f32 = 1.0;
const POP_AT_COLLAPSE: f32 = 0.5;

impl App {
    pub(super) fn timer_orb_rect(&self) -> Option<winisland_render::Rect> {
        let frame = self
            .multitask
            .frame()
            .filter(|frame| frame.task.content.has_timer() && frame.content_opacity > 0.5)?;
        let layout = self.compute_island_layout();
        crate::ui::island::multitask::surface(
            winisland_render::Rect::from_xywh(
                layout.current_island_x as f32,
                layout.current_island_y as f32,
                self.springs.w.value,
                self.springs.h.value,
            ),
            self.springs.r.value,
            self.config.compact_scale,
            self.compact_content_height(),
            layout.secondary_left,
            Some(frame),
        )
        .secondary
    }

    pub(super) fn update_multitask(&mut self, window: &WindowRef, now: Instant) {
        if !self.config.device_status_indicators {
            self.device_feed = None;
            self.device_monitor_retry.clear();
        } else if self.device_feed.is_none() && self.device_monitor_retry.is_ready(now) {
            match crate::platform::device_activity().monitor(crate::platform::wake) {
                Ok(feed) => self.device_feed = Some(feed),
                Err(error) => {
                    log::warn!("Cannot start device activity monitor: {error}");
                    self.device_monitor_retry
                        .start(now, Duration::from_secs(10));
                }
            }
        }
        let activity = self
            .device_feed
            .as_ref()
            .map_or(DeviceActivity::default(), |feed| feed.snapshot());
        let indicators_changed = self.status_indicators.update(
            activity,
            self.timer_content.filter(|_| self.timer_orb),
            now,
        );
        let task = self.status_indicators.activity().map(|content| Task {
            id: TaskId(1),
            content,
        });
        self.multitask.set_secondary(task);
        let presentation = if self.expanded {
            Presentation::Expanded
        } else if !self.visible
            || self.is_hidden()
            || self.components_hidden
            || self.compact_overlay.is_visible()
        {
            Presentation::Hidden
        } else {
            Presentation::Compact
        };
        let compact_h = self.compact_content_height();
        if self.springs.expanded_target {
            self.orb_collapse_from = Some(self.springs.h.value);
        }
        let collapse_progress = self
            .orb_collapse_from
            .filter(|from| (from - compact_h).abs() > self.config.compact_scale)
            .map(|from| (from - self.springs.h.value) / (from - compact_h));
        let settle_speed = SETTLE_SPEED * self.config.compact_scale;
        let settled = !self.springs.expanded_target
            && (self.springs.h.value - compact_h).abs() < 1.5 * self.config.compact_scale
            && self.springs.w.velocity.abs() < settle_speed
            && self.springs.h.velocity.abs() < settle_speed
            && self.springs.r.velocity.abs() < settle_speed;
        if settled {
            self.orb_collapse_from = None;
        }
        let compact_settled = settled
            || (!self.springs.expanded_target
                && collapse_progress.is_some_and(|progress| progress >= POP_AT_COLLAPSE));
        let separation_before = self.multitask.frame().map_or(0.0, |frame| frame.separation);
        let changed = self.multitask.update(now, presentation, compact_settled);
        let separation = self.multitask.frame().map_or(0.0, |frame| frame.separation);
        let impulse = if separation_before >= 0.7 && separation < 0.7 {
            -1.8
        } else {
            0.0
        };
        if impulse != 0.0 {
            let direction = if self.compute_island_layout().secondary_left {
                -1.0
            } else {
                1.0
            };
            self.springs.multitask_x.velocity += impulse * direction * self.config.compact_scale;
        }
        if changed || indicators_changed {
            window.request_redraw();
        }
    }
}
