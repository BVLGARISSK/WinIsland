mod types;

pub use types::*;

use std::time::Instant;

#[derive(Default)]
pub struct ActivityManager {
    activities: Vec<Activity>,
}

impl ActivityManager {
    pub fn upsert(&mut self, activity: Activity) -> bool {
        if let Some(existing) = self
            .activities
            .iter_mut()
            .find(|item| item.id == activity.id)
        {
            if *existing == activity {
                return false;
            }
            *existing = activity;
        } else {
            self.activities.push(activity);
        }
        true
    }

    pub fn get(&self, id: u64) -> Option<&Activity> {
        self.activities.iter().find(|item| item.id == id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let previous_len = self.activities.len();
        self.activities.retain(|item| item.id != id);
        self.activities.len() != previous_len
    }

    pub fn current(&self) -> Option<&Activity> {
        let now = Instant::now();
        self.activities
            .iter()
            .filter(|item| item.visible_at(now))
            .max_by_key(|item| (item.priority, item.updated_at, item.id))
    }

    /// Returns the activity `current` would pick if `id` were absent.
    pub fn current_excluding(&self, id: u64) -> Option<&Activity> {
        let now = Instant::now();
        self.activities
            .iter()
            .filter(|item| item.id != id && item.visible_at(now))
            .max_by_key(|item| (item.priority, item.updated_at, item.id))
    }

    pub fn keeps_visible(&self) -> bool {
        let now = Instant::now();
        self.activities
            .iter()
            .any(|item| item.keep_visible && item.visible_at(now))
    }

    pub fn preferred_size(&self) -> (f32, f32) {
        let now = Instant::now();
        self.activities
            .iter()
            .filter(|item| item.visible_at(now))
            .fold((0.0_f32, 0.0_f32), |size, item| {
                (
                    size.0.max(item.preferred_width),
                    size.1.max(item.preferred_height),
                )
            })
    }

    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let previous_len = self.activities.len();
        self.activities
            .retain(|item| item.expires_at.is_none_or(|deadline| deadline > now));
        self.activities.len() != previous_len
    }
}
