mod input;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::Thread;
use std::time::{Duration, Instant};

use crate::fault::{PluginCallGuard, PluginIdentity};
use winisland_plugin_api::*;

pub use input::InputOutcome;

pub(crate) const MAX_PENDING: usize = 256;

#[derive(Clone, Default)]
pub struct Event {
    pub kind: u64,
    pub detail: u32,
    pub target: u64,
    pub sequence: u64,
    pub x: f32,
    pub y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub modifiers: u32,
    pub code: u32,
    pub data: Vec<u8>,
    pub completion: Option<(PluginToken, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presentation {
    pub target: u64,
    pub bounds: [f32; 4],
    pub logical: [f32; 2],
    pub interactive: bool,
}

#[derive(Clone)]
pub struct Command {
    pub resource: u64,
    pub id: String,
    pub title: String,
    pub flags: u32,
    pub modifiers: u32,
    pub key: u32,
}

#[derive(Clone)]
pub struct AssistantTool {
    pub resource: u64,
    pub id: String,
    pub plugin: String,
    pub description: String,
    pub schema: String,
}

pub enum HostRequest {
    Command {
        name: String,
        data: Vec<u8>,
    },
    ShowPage(u64),
    Media {
        session: u64,
        command: u32,
        position_ms: u64,
    },
}

pub struct Request {
    pub token: PluginToken,
    pub sequence: u64,
    pub action: HostRequest,
}

pub(crate) struct Callback {
    pub token: PluginToken,
    pub target: u64,
    pub mask: u64,
    pub function: PluginEventFnV2,
    pub data: usize,
    pub in_flight: bool,
    pub timer: Option<Timer>,
}

pub(crate) struct Timer {
    pub deadline: Option<Instant>,
    pub interval: Duration,
    pub visible_only: bool,
}

#[derive(Default)]
pub(crate) struct State {
    pub callbacks: HashMap<u64, Callback>,
    pub commands: HashMap<u64, Command>,
    pub assistant_tools: HashMap<u64, AssistantTool>,
    pub assistant_calls: HashMap<u64, PluginToken>,
    pub assistant_results: HashMap<u64, (bool, String)>,
    pub pending: HashMap<PluginToken, VecDeque<(u64, Event)>>,
    pub requests: VecDeque<Request>,
    pub workers: HashMap<PluginToken, Thread>,
    pub regions: HashMap<u64, Vec<InputRegionV2>>,
    pub presentations: Vec<Presentation>,
    pub next_presentations: Vec<Presentation>,
    pub animations: HashMap<u64, bool>,
    pub capture: Option<(u64, u64)>,
    pub focus: Option<(u64, u64)>,
    pub hover: Option<(u64, u64)>,
    pub capture_button: u32,
    pub scrolled_at: Option<Instant>,
    pub sessions: Vec<MediaSessionV2>,
    pub island: IslandStateV2,
    pub sequence: u64,
}

pub struct Extensions {
    pub(crate) state: Mutex<State>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    changed: AtomicBool,
    started: Instant,
}

impl Default for Extensions {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            wake: Mutex::new(None),
            changed: AtomicBool::new(false),
            started: Instant::now(),
        }
    }
}

impl Extensions {
    pub fn set_wake(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut callback) = self.wake.lock() {
            *callback = Some(wake);
        }
    }

    pub fn changed(&self) {
        self.changed.store(true, Ordering::Release);
        let callback = self.wake.lock().ok().and_then(|wake| wake.clone());
        if let Some(callback) = callback {
            callback();
        }
    }

    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }

    pub fn time_seconds(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn commands(&self) -> Vec<Command> {
        let mut commands = self
            .state
            .lock()
            .map(|state| state.commands.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        commands.sort_by(|a, b| a.id.cmp(&b.id));
        commands
    }

    pub fn assistant_tools(&self) -> Vec<AssistantTool> {
        let mut tools = self
            .state
            .lock()
            .map(|state| state.assistant_tools.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        tools.sort_by(|a, b| a.id.cmp(&b.id));
        tools
    }

    /// Queues an assistant call to the tool `id`; the plugin answers through `respond`.
    pub fn call_assistant(&self, id: &str, args: &str) -> Result<u64, PluginStatus> {
        let mut state = self.state.lock().map_err(|_| PluginStatus::Internal)?;
        let (resource, token) = state
            .assistant_tools
            .values()
            .find(|tool| tool.id == id)
            .and_then(|tool| Some((tool.resource, state.callbacks.get(&tool.resource)?.token)))
            .ok_or(PluginStatus::InvalidArgument)?;
        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or(PluginStatus::LimitExceeded)?;
        let sequence = state.sequence;
        state.enqueue(
            resource,
            Event {
                kind: EVENT_ASSISTANT,
                sequence,
                data: args.as_bytes().to_vec(),
                ..Default::default()
            },
        )?;
        state.assistant_calls.insert(sequence, token);
        Ok(sequence)
    }

    pub fn take_assistant_result(&self, sequence: u64) -> Option<(bool, String)> {
        self.state.lock().ok()?.assistant_results.remove(&sequence)
    }

    pub fn cancel_assistant(&self, sequence: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.assistant_calls.remove(&sequence);
            state.assistant_results.remove(&sequence);
        }
    }

    pub fn drain_requests(&self) -> Vec<Request> {
        self.state
            .lock()
            .map(|mut state| state.requests.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn complete(&self, token: PluginToken, sequence: u64, status: PluginStatus) {
        if let Ok(mut state) = self.state.lock() {
            state.publish(
                Some(token),
                Event {
                    kind: EVENT_RESULT,
                    sequence,
                    code: status.code() as u32,
                    ..Default::default()
                },
            );
        }
    }

    pub fn set_island(&self, snapshot: IslandStateV2) {
        if let Ok(mut state) = self.state.lock()
            && state.island != snapshot
        {
            state.island = snapshot;
            state.publish(
                None,
                Event {
                    kind: EVENT_HOST,
                    ..Default::default()
                },
            );
        }
    }

    pub fn set_sessions(&self, sessions: Vec<MediaSessionV2>) {
        if let Ok(mut state) = self.state.lock() {
            let changed = state.sessions.len() != sessions.len()
                || state.sessions.iter().zip(&sessions).any(|(old, next)| {
                    let mut old = *old;
                    old.sampled_at_seconds = next.sampled_at_seconds;
                    old != *next
                });
            if !changed {
                return;
            }
            state.sessions = sessions;
            state.publish(
                None,
                Event {
                    kind: EVENT_MEDIA,
                    ..Default::default()
                },
            );
        }
    }

    pub fn invoke(&self, id: &str, data: &[u8]) -> Result<(), PluginStatus> {
        let mut state = self.state.lock().map_err(|_| PluginStatus::Internal)?;
        let resource = state
            .commands
            .values()
            .find(|command| command.id == id && command.flags & COMMAND_ENABLED != 0)
            .map(|command| command.resource)
            .ok_or(PluginStatus::InvalidArgument)?;
        state.enqueue(
            resource,
            Event {
                kind: EVENT_COMMAND,
                data: data.to_vec(),
                ..Default::default()
            },
        )
    }

    pub fn begin_frame(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.next_presentations.clear();
        }
    }

    pub fn present(&self, presentation: Presentation) {
        if let Ok(mut state) = self.state.lock() {
            state.next_presentations.push(presentation);
        }
    }

    pub fn finish_frame(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let next = std::mem::take(&mut state.next_presentations);
        let old = std::mem::replace(&mut state.presentations, next);
        let old_ids: HashSet<_> = old.iter().map(|p| p.target).collect();
        let current = state.presentations.clone();
        let new_ids: HashSet<_> = current.iter().map(|p| p.target).collect();
        for target in old_ids.symmetric_difference(&new_ids) {
            state.publish(
                None,
                Event {
                    kind: EVENT_VISIBILITY,
                    target: *target,
                    code: u32::from(new_ids.contains(target)),
                    ..Default::default()
                },
            );
        }
        for p in &current {
            if old
                .iter()
                .find(|old| old.target == p.target)
                .is_none_or(|old| old.logical != p.logical)
            {
                state.publish(
                    None,
                    Event {
                        kind: EVENT_RESIZE,
                        target: p.target,
                        x: p.logical[0],
                        y: p.logical[1],
                        ..Default::default()
                    },
                );
            }
        }
        let active: HashSet<_> = current
            .iter()
            .filter(|p| p.interactive)
            .map(|p| p.target)
            .collect();
        state.cancel_missing(&active);
        if old_ids != new_ids {
            for worker in state.workers.values() {
                worker.unpark();
            }
        }
    }

    pub fn clear_presentations(&self) {
        self.begin_frame();
        self.finish_frame();
    }

    pub fn should_tick(&self, target: u64, surface: bool) -> bool {
        self.state.lock().is_ok_and(|state| {
            state.animations.get(&target).copied().unwrap_or(true)
                && (!surface || state.presentations.iter().any(|p| p.target == target))
        })
    }

    pub(crate) fn attach_worker(&self, token: PluginToken, worker: Thread) {
        if let Ok(mut state) = self.state.lock() {
            state.workers.insert(token, worker);
        }
    }

    pub(crate) fn dispatch(&self, token: PluginToken, identity: &Arc<PluginIdentity>) {
        let now = Instant::now();
        if let Ok(mut state) = self.state.lock() {
            let visible: HashSet<_> = state.presentations.iter().map(|p| p.target).collect();
            let mut due = Vec::new();
            for (id, callback) in &mut state.callbacks {
                if callback.token != token {
                    continue;
                }
                let Some(timer) = callback.timer.as_mut() else {
                    continue;
                };
                if timer.deadline.is_some_and(|deadline| deadline <= now)
                    && (!timer.visible_only || visible.contains(&callback.target))
                {
                    due.push((*id, callback.target));
                    timer.deadline = (!timer.interval.is_zero()).then(|| now + timer.interval);
                }
            }
            for (resource, target) in due {
                let _ = state.enqueue(
                    resource,
                    Event {
                        kind: EVENT_TIMER,
                        target,
                        ..Default::default()
                    },
                );
            }
        }
        for _ in 0..MAX_PENDING {
            let delivery = {
                let Ok(mut state) = self.state.lock() else {
                    return;
                };
                let Some((id, event)) = state.pending.get_mut(&token).and_then(VecDeque::pop_front)
                else {
                    break;
                };
                state.callbacks.get_mut(&id).map(|callback| {
                    callback.in_flight = true;
                    (id, event, callback.function, callback.data)
                })
            };
            let Some((id, event, callback, data)) = delivery else {
                continue;
            };
            let raw = PluginEventV2 {
                kind: event.kind,
                detail: event.detail,
                // SAFETY: These identifiers originated from the host resource registry, or are the zero wildcard.
                target: unsafe { WidgetId::from_raw(event.target) },
                // SAFETY: This callback resource remains allocated until the callback returns.
                resource: unsafe { ResourceId::from_raw(id) },
                sequence: event.sequence,
                time_seconds: self.time_seconds(),
                x: event.x,
                y: event.y,
                delta_x: event.delta_x,
                delta_y: event.delta_y,
                modifiers: event.modifiers,
                code: event.code,
                data: ByteSlice::borrowed(&event.data),
                ..Default::default()
            };
            // SAFETY: Release refuses in-flight callbacks and the owned payload lives through this call.
            {
                let _guard = PluginCallGuard::enter(identity);
                unsafe { callback(data as *mut _, &raw) };
            }
            if let Some((caller, sequence)) = event.completion {
                self.complete(caller, sequence, PluginStatus::Ok);
            }
            if let Ok(mut state) = self.state.lock()
                && let Some(callback) = state.callbacks.get_mut(&id)
            {
                callback.in_flight = false;
            }
        }
    }

    pub(crate) fn wait_duration(
        &self,
        token: PluginToken,
        tick: bool,
        period: Duration,
    ) -> Duration {
        let Ok(state) = self.state.lock() else {
            return period;
        };
        if state
            .pending
            .get(&token)
            .is_some_and(|queue| !queue.is_empty())
        {
            return Duration::ZERO;
        }
        let mut wait = if tick {
            period
        } else {
            Duration::from_secs(3600)
        };
        for callback in state
            .callbacks
            .values()
            .filter(|callback| callback.token == token)
        {
            if callback
                .timer
                .as_ref()
                .is_some_and(|timer| timer.visible_only)
                && !state
                    .presentations
                    .iter()
                    .any(|p| p.target == callback.target)
            {
                continue;
            }
            if let Some(deadline) = callback.timer.as_ref().and_then(|timer| timer.deadline) {
                wait = wait.min(deadline.saturating_duration_since(Instant::now()));
            }
        }
        wait
    }

    pub(crate) fn revoke(&self, token: PluginToken) {
        if let Ok(mut state) = self.state.lock() {
            let removed: HashSet<_> = state
                .callbacks
                .iter()
                .filter_map(|(id, c)| (c.token == token).then_some(*id))
                .collect();
            state
                .callbacks
                .retain(|_, callback| callback.token != token);
            state.commands.retain(|id, _| !removed.contains(id));
            state.assistant_tools.retain(|id, _| !removed.contains(id));
            let abandoned: Vec<u64> = state
                .assistant_calls
                .iter()
                .filter_map(|(sequence, owner)| (*owner == token).then_some(*sequence))
                .collect();
            for sequence in abandoned {
                state.assistant_calls.remove(&sequence);
                state
                    .assistant_results
                    .insert(sequence, (false, "plugin unloaded".into()));
            }
            if let Some(queue) = state.pending.remove(&token) {
                for (_, event) in queue {
                    if let Some((caller, sequence)) = event.completion {
                        state.publish(
                            Some(caller),
                            Event {
                                kind: EVENT_RESULT,
                                sequence,
                                code: PluginStatus::StaleHandle.code() as u32,
                                ..Default::default()
                            },
                        );
                    }
                }
            }
            state.workers.remove(&token);
            state.requests.retain(|request| request.token != token);
        }
        self.changed();
    }

    pub(crate) fn remove_target(&self, target: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.presentations.retain(|p| p.target != target);
            state.next_presentations.retain(|p| p.target != target);
            state.regions.remove(&target);
            state.animations.remove(&target);
            for callback in state
                .callbacks
                .values_mut()
                .filter(|callback| callback.target == target)
            {
                if let Some(timer) = &mut callback.timer {
                    timer.deadline = None;
                }
            }
            let active = state.presentations.iter().map(|p| p.target).collect();
            state.cancel_missing(&active);
        }
        self.changed();
    }
}

impl State {
    pub(crate) fn enqueue(&mut self, id: u64, event: Event) -> Result<(), PluginStatus> {
        let callback = self.callbacks.get(&id).ok_or(PluginStatus::StaleHandle)?;
        let token = callback.token;
        let queue = self.pending.entry(token).or_default();
        let coalesced = matches!(
            event.kind,
            EVENT_HOST | EVENT_MEDIA | EVENT_RESIZE | EVENT_VISIBILITY | EVENT_TIMER
        ) || (event.kind == EVENT_INPUT && event.detail == INPUT_MOVE);
        if coalesced
            && let Some((_, old)) = queue.back_mut().filter(|(resource, old)| {
                *resource == id
                    && old.kind == event.kind
                    && old.target == event.target
                    && old.detail == event.detail
            })
        {
            *old = event;
        } else {
            if queue.len() >= MAX_PENDING {
                return Err(PluginStatus::LimitExceeded);
            }
            queue.push_back((id, event));
        }
        if let Some(worker) = self.workers.get(&token) {
            worker.unpark();
        }
        Ok(())
    }

    pub(crate) fn publish(&mut self, token: Option<PluginToken>, event: Event) {
        let ids: Vec<_> = self
            .callbacks
            .iter()
            .filter_map(|(id, callback)| {
                (callback.timer.is_none()
                    && callback.mask & event.kind != 0
                    && token.is_none_or(|token| callback.token == token)
                    && (callback.target == 0 || callback.target == event.target))
                    .then_some(*id)
            })
            .collect();
        for id in ids {
            let _ = self.enqueue(id, event.clone());
        }
    }

    pub(crate) fn request(
        &mut self,
        token: PluginToken,
        action: HostRequest,
    ) -> Result<u64, PluginStatus> {
        if self.requests.len() >= MAX_PENDING {
            return Err(PluginStatus::LimitExceeded);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(PluginStatus::LimitExceeded)?;
        let sequence = self.sequence;
        self.requests.push_back(Request {
            token,
            sequence,
            action,
        });
        Ok(sequence)
    }
}
