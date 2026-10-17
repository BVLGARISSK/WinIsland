use std::time::{Duration, Instant};

use crate::anim::{Curve, Tween};

const SEPARATE_DURATION: Duration = Duration::from_millis(540);
const RESEPARATE_DURATION: Duration = Duration::from_millis(320);
const MERGE_DURATION: Duration = Duration::from_millis(360);
const EXPAND_MERGE_DURATION: Duration = Duration::from_millis(230);
const CONTENT_DURATION: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Task<T> {
    pub id: TaskId,
    pub content: T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presentation {
    Compact,
    Expanded,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaskFrame<T> {
    pub task: Task<T>,
    pub separation: f32,
    pub content_opacity: f32,
}

pub struct Multitask<T> {
    requested: Option<Task<T>>,
    current: Option<Task<T>>,
    separation: Tween,
    content: Tween,
    detached: Option<TaskId>,
}

impl<T: Copy + PartialEq> Default for Multitask<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + PartialEq> Multitask<T> {
    pub fn new() -> Self {
        Self {
            requested: None,
            current: None,
            separation: Tween::new(0.0),
            content: Tween::new(0.0),
            detached: None,
        }
    }

    pub fn set_secondary(&mut self, task: Option<Task<T>>) {
        self.requested = task;
    }

    pub fn update(
        &mut self,
        now: Instant,
        presentation: Presentation,
        compact_settled: bool,
    ) -> bool {
        let before = self.frame();
        self.separation.update(now);
        self.content.update(now);
        if let (Some(current), Some(requested)) = (&mut self.current, self.requested)
            && current.id == requested.id
        {
            *current = requested;
        }
        let replacing = self.current.map(|task| task.id) != self.requested.map(|task| task.id);
        let detach = presentation == Presentation::Compact
            && (compact_settled || self.separation.value() > 0.0)
            && !replacing
            && self.current.is_some();
        let returning =
            self.detached.is_some() && self.detached == self.current.map(|task| task.id);
        let duration = if detach && returning {
            RESEPARATE_DURATION
        } else if detach {
            SEPARATE_DURATION
        } else if presentation == Presentation::Expanded {
            EXPAND_MERGE_DURATION
        } else {
            MERGE_DURATION
        };
        self.separation.retarget(
            if detach { 1.0 } else { 0.0 },
            duration,
            if detach {
                Curve::EaseInRebound
            } else {
                Curve::Rebound
            },
            now,
        );
        if self.separation.value() == 0.0 && !self.separation.is_animating() {
            self.current = self.requested;
            if presentation == Presentation::Compact && compact_settled && self.current.is_some() {
                self.separation
                    .retarget(1.0, SEPARATE_DURATION, Curve::EaseInRebound, now);
            }
        }
        if self.current.is_none() {
            self.detached = None;
        } else if self.separation.value() >= 0.99 {
            self.detached = self.current.map(|task| task.id);
        }
        let show_content = self.current.is_some() && self.separation.value() >= 0.55 && detach;
        self.content.retarget(
            if show_content { 1.0 } else { 0.0 },
            CONTENT_DURATION,
            Curve::Ease,
            now,
        );
        before != self.frame()
    }

    pub fn frame(&self) -> Option<TaskFrame<T>> {
        (self.separation.value() != 0.0 || self.separation.is_animating()).then_some(TaskFrame {
            task: self.current?,
            separation: self.separation.value(),
            content_opacity: self.content.value(),
        })
    }

    pub fn is_animating(&self) -> bool {
        self.separation.is_animating() || self.content.is_animating()
    }
}
