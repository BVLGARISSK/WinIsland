use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use winisland_core::config::AppConfig;
use winisland_core::context::ActivityContent;
use winisland_core::physics::Spring;
use winisland_platform::{
    InputState, Key, MouseButton, MouseWheelDelta, PlatformEvent, TouchPhase,
};
use winisland_plugin_api::{COMMAND_ENABLED, MOD_CONTROL};
use winisland_render::{Point, Rect};

use super::App;
use crate::core::assistant::{AssistantCommand, AssistantLink, AssistantRequest, AssistantState};
use crate::core::lingosoul_install::{self, AgentProcess};
use crate::plugin::inventory::InstalledPlugin;
use crate::ui::island::LingoSoulStyle;
use crate::ui::lingosoul_chat::{ChatHit, ChatPanel};

const DROP_HEIGHT: f32 = 2.9;
const DROP_ASPECT: f32 = 1.29;
const DROP_RADIUS: f32 = 0.5;
const DROP_SWELL: f32 = 0.07;
const THINKING_WIDTH: f32 = 1.55;
const GLASS_WAIT: Duration = Duration::from_millis(600);
const WAVE_FADE: f32 = 0.5;
const WAVE_INTRO: f32 = 0.65;
const WAVE_CLOSE: f32 = 0.16;
const WAVE_SPEED: f32 = -2.7;
const PANEL_HEIGHT: f32 = 2.1;
const PANEL_SCREEN_SHARE: f32 = 0.62;
const PLUGIN_CALL_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_PASTE: usize = 4000;
/// A second tap within this window counts as a double tap.
const DOUBLE_TAP: Duration = Duration::from_millis(320);
/// Synthetic command id for the tray item and hotkey that open the chat panel.
pub(super) const CHAT_COMMAND: u64 = u64::MAX;

pub(super) struct LingoSoulView {
    link: AssistantLink,
    /// The assistant's own state; `requested` is what the island shows of it.
    raw: AssistantState,
    expanded: bool,
    /// A reply that arrived while the island was expanded, shown once it collapses.
    deferred_reply: Option<Arc<str>>,
    deferred_until: Option<Instant>,
    pending_tap: Option<Instant>,
    connected: bool,
    retention_sent: Option<String>,
    history_revision: Option<u64>,
    requested: AssistantState,
    state: AssistantState,
    glass_ready: bool,
    warm_frames: u8,
    waiting_since: Option<Instant>,
    level: f32,
    drop: Spring,
    dots_alpha: Spring,
    cover: Spring,
    darken: Spring,
    reply: Spring,
    beam: Spring,
    panel: Spring,
    panel_open: bool,
    chat: ChatPanel,
    window_extended: bool,
    screen_h: f32,
    ctrl: bool,
    plugin_calls: Vec<(u64, u64, Instant)>,
    /// The agent this island started, if any.
    process: AgentProcess,
    autostart_tried: bool,
    status_revision: Option<u64>,
    reply_text: Arc<str>,
    wave_fade: f32,
    wave_intro: f32,
    wave_time: f32,
    phase: f32,
    dots: f32,
    unit: f32,
}

fn is_drop(state: AssistantState) -> bool {
    matches!(state, AssistantState::Listening | AssistantState::Speaking)
}

impl LingoSoulView {
    pub(super) fn new() -> Self {
        Self {
            link: AssistantLink::start(),
            raw: AssistantState::Idle,
            expanded: false,
            deferred_reply: None,
            deferred_until: None,
            pending_tap: None,
            connected: false,
            retention_sent: None,
            history_revision: None,
            requested: AssistantState::Idle,
            state: AssistantState::Idle,
            glass_ready: false,
            warm_frames: 0,
            waiting_since: None,
            level: 0.0,
            drop: Spring::new(0.0),
            dots_alpha: Spring::new(0.0),
            cover: Spring::new(0.0),
            darken: Spring::new(1.0),
            reply: Spring::new(0.0),
            beam: Spring::new(0.0),
            panel: Spring::new(0.0),
            panel_open: false,
            chat: ChatPanel::default(),
            window_extended: false,
            screen_h: 0.0,
            ctrl: false,
            plugin_calls: Vec::new(),
            process: AgentProcess::default(),
            autostart_tried: false,
            status_revision: None,
            reply_text: Arc::from(""),
            wave_fade: 0.0,
            wave_intro: 0.0,
            wave_time: 0.0,
            phase: 0.0,
            dots: 0.0,
            unit: 0.0,
        }
    }

    pub(super) fn wants_glass(&self) -> bool {
        self.requested != AssistantState::Idle
            || self.state != AssistantState::Idle
            || self.drop.value > 0.001
            || self.panel_open
            || self.panel.value > 0.001
    }

    pub(super) fn set_glass_ready(&mut self, ready: bool) {
        if !self.wants_glass() {
            self.glass_ready = false;
            self.warm_frames = 0;
        } else if ready || self.glass_ready {
            self.glass_ready = true;
            self.warm_frames = self.warm_frames.saturating_add(1);
        }
    }

    fn is_warm(&self) -> bool {
        self.warm_frames >= 2
    }

    pub(super) fn set_screen_height(&mut self, height: f32) {
        self.screen_h = height;
    }

    pub(super) fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }

    fn open_panel(&mut self) {
        self.panel_open = true;
        self.chat.scroll_to_end();
        self.link.set_panel_open(true);
    }

    fn close_panel(&mut self) {
        self.panel_open = false;
        self.chat.focused = false;
        self.link.set_panel_open(false);
    }

    /// What the compact island shows: nothing while expanded or while the panel hosts the
    /// exchange, and a held-back reply once the island collapses.
    fn visible_state(&mut self, raw: AssistantState, text: &Arc<str>) -> AssistantState {
        if raw == AssistantState::Replying && self.expanded && !text.is_empty() {
            self.deferred_reply = Some(text.clone());
            self.deferred_until = None;
        } else if !matches!(raw, AssistantState::Idle | AssistantState::Replying) {
            self.deferred_reply = None;
        }
        if self.expanded {
            return AssistantState::Idle;
        }
        if raw == AssistantState::Idle
            && let Some(reply) = self.deferred_reply.clone()
        {
            let until = *self
                .deferred_until
                .get_or_insert_with(|| Instant::now() + reply_hold(&reply));
            if Instant::now() < until {
                self.reply_text = reply;
                return AssistantState::Replying;
            }
            self.deferred_reply = None;
            self.deferred_until = None;
        }
        if self.panel_open && raw != AssistantState::Replying {
            return AssistantState::Idle;
        }
        raw
    }

    pub(super) fn update(&mut self, dt: f32) {
        for command in self.link.take_commands() {
            match command {
                AssistantCommand::Timer(seconds) => {
                    log::info!("LingoSoul: timer {seconds}s");
                    crate::ui::expanded::timer_view::start_for(seconds);
                }
                AssistantCommand::OpenChat => self.open_panel(),
            }
        }
        let snapshot = self.link.snapshot();
        self.raw = snapshot.state;
        if snapshot.state == AssistantState::Replying && !snapshot.text.is_empty() {
            self.reply_text = snapshot.text.clone();
        }
        self.requested = self.visible_state(snapshot.state, &snapshot.text);
        let link = &self.link;
        self.chat.sync(link.chat_revision(), || link.chat());
        self.chat.connected = link.is_connected();
        self.chat.voice = link.voice_available();
        self.chat.thinking = self.panel_open && self.raw == AssistantState::Thinking;
        self.chat.listening = self.panel_open && is_drop(self.raw);
        self.chat.level = if self.chat.listening {
            snapshot.level
        } else {
            0.0
        };
        let blocked = is_drop(self.requested) && !is_drop(self.state) && !self.is_warm();
        if blocked {
            let since = *self.waiting_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= GLASS_WAIT {
                self.state = self.requested;
            }
        } else {
            self.waiting_since = None;
            self.state = self.requested;
        }

        let beam = self.state == AssistantState::Replying && snapshot.listening && !self.panel_open;
        self.beam
            .update_dt(if beam { 1.0 } else { 0.0 }, 0.12, 0.55, dt);
        self.beam.settle(if beam { 1.0 } else { 0.0 }, 0.001, 0.001);
        let target = if is_drop(self.state) || beam {
            snapshot.level
        } else {
            0.0
        };
        let rate: f32 = if target > self.level { 0.45 } else { 0.08 };
        self.level += (target - self.level) * (1.0 - (1.0 - rate).powf(dt));

        let drop = if is_drop(self.state) { 1.0 } else { 0.0 };
        self.drop.update_dt(drop, 0.16, 0.7, dt);
        self.drop.settle(drop, 0.001, 0.001);
        let thinking = self.state == AssistantState::Thinking && !self.panel_open;
        let (dots, stiffness) = if thinking { (1.0, 0.06) } else { (0.0, 0.3) };
        self.dots_alpha.update_dt(dots, stiffness, 0.5, dt);
        self.dots_alpha.settle(dots, 0.001, 0.001);
        let cover = if self.state == AssistantState::Idle && !self.panel_open {
            0.0
        } else {
            1.0
        };
        self.cover.update_dt(cover, 0.2, 0.5, dt);
        self.cover.settle(cover, 0.001, 0.001);
        let darken = if is_drop(self.state) { 0.0 } else { 1.0 };
        self.darken.update_dt(darken, 0.6, 0.4, dt);
        self.darken.settle(darken, 0.001, 0.001);
        let reply = if self.state == AssistantState::Replying && !self.panel_open {
            1.0
        } else {
            0.0
        };
        let reply_stiffness = if reply > 0.0 { 0.035 } else { 0.25 };
        self.reply.update_dt(reply, reply_stiffness, 0.55, dt);
        self.reply.settle(reply, 0.001, 0.001);
        let panel = if self.panel_open { 1.0 } else { 0.0 };
        let panel_stiffness = if self.panel_open { 0.05 } else { 0.3 };
        self.panel.update_dt(panel, panel_stiffness, 0.6, dt);
        self.panel.settle(panel, 0.001, 0.001);

        let seconds = dt / 60.0;
        if is_drop(self.state) {
            if self.drop.value > 0.99 {
                self.wave_fade = (self.wave_fade + seconds).min(WAVE_FADE);
                self.wave_time += seconds * WAVE_SPEED;
                if self.wave_fade >= WAVE_FADE {
                    self.wave_intro = (self.wave_intro + seconds).min(WAVE_INTRO);
                }
            }
        } else {
            self.wave_fade = (self.wave_fade - seconds * WAVE_FADE / WAVE_CLOSE).max(0.0);
            self.wave_intro = 0.0;
            self.wave_time = 0.0;
        }

        if self.is_animating() {
            self.phase += dt / 60.0 * (1.0 + 2.0 * self.level);
            self.dots += dt / 60.0;
        }
    }

    pub(super) fn needs_backdrop(&self) -> bool {
        is_drop(self.requested) || is_drop(self.state) || self.drop.value > 0.001
    }

    pub(super) fn is_animating(&self) -> bool {
        let moving = |spring: &Spring| spring.velocity.abs() > 0.001;
        self.needs_backdrop()
            || self.requested != self.state
            || self.dots_alpha.value > 0.001
            || self.beam.value > 0.001
            || self.wave_fade > 0.0
            || self.chat.thinking
            || self.chat.listening
            || self.pending_tap.is_some()
            || moving(&self.drop)
            || moving(&self.cover)
            || moving(&self.reply)
            || moving(&self.darken)
            || moving(&self.dots_alpha)
            || moving(&self.beam)
            || moving(&self.panel)
    }

    /// Plugin tool calls from the agent still waiting for the plugin's answer.
    pub(super) fn has_pending_requests(&self) -> bool {
        !self.plugin_calls.is_empty()
    }

    fn panel_size(&self, config: &AppConfig) -> (f32, f32) {
        let scale = config.expanded_scale;
        let mut h = config.expanded_height * PANEL_HEIGHT * scale;
        if self.screen_h > 0.0 {
            h = h.min(self.screen_h * PANEL_SCREEN_SHARE);
        }
        (
            config.expanded_width * scale,
            h.max(config.expanded_height * scale),
        )
    }

    /// Extra window height the open chat panel needs.
    pub(super) fn panel_extent(&self, config: &AppConfig) -> Option<f32> {
        (self.panel_open || self.panel.value > 0.001).then(|| self.panel_size(config).1)
    }

    pub(super) fn target_size(
        &mut self,
        config: &AppConfig,
        compact_h: f32,
    ) -> Option<(f32, f32, f32)> {
        self.unit = compact_h;
        if self.panel_open {
            let (w, h) = self.panel_size(config);
            self.chat.prepare(w, h, config.expanded_scale);
            let radius = crate::ui::lingosoul_chat::PANEL_RADIUS * config.expanded_scale;
            return Some((w, h, radius.min(h * 0.36)));
        }
        match self.state {
            AssistantState::Idle => None,
            AssistantState::Thinking => Some((
                config.base_width * config.compact_scale * THINKING_WIDTH,
                compact_h,
                compact_h / 2.0,
            )),
            AssistantState::Replying => {
                let scale = config.expanded_scale;
                let w = config.expanded_width * scale;
                let layout = crate::ui::island::lingosoul_reply_layout(&self.reply_text, w, scale);
                let h = layout.height.max(compact_h * 2.6);
                Some((w, h, lingosoul_card_radius(w, h, scale)))
            }
            AssistantState::Listening | AssistantState::Speaking => {
                let h = compact_h * (DROP_HEIGHT + DROP_SWELL * self.level);
                Some((h * DROP_ASPECT, h, h * DROP_RADIUS))
            }
        }
    }

    pub(super) fn capture_extent(&self, config: &AppConfig) -> (f32, f32) {
        let unit = self.unit.max(1.0);
        let drop_h = unit * (DROP_HEIGHT + DROP_SWELL);
        let scale = config.expanded_scale;
        let card_w = config.expanded_width * scale;
        let card_h = crate::ui::island::lingosoul_reply_layout(&self.reply_text, card_w, scale)
            .height
            .max(unit * 2.6);
        let panel_h = self.panel_extent(config).unwrap_or(0.0);
        (
            (drop_h * DROP_ASPECT).max(card_w),
            drop_h.max(card_h).max(panel_h),
        )
    }

    pub(super) fn reply_text(&self) -> &str {
        &self.reply_text
    }

    pub(super) fn chat(&self) -> &ChatPanel {
        &self.chat
    }

    pub(super) fn style(&self) -> LingoSoulStyle {
        LingoSoulStyle {
            drop: self.drop.value.clamp(0.0, 1.0),
            level: self.level,
            phase: self.phase,
            dots: self.dots,
            dots_alpha: self.dots_alpha.value.clamp(0.0, 1.0),
            cover: self.cover.value.clamp(0.0, 1.0),
            unit: self.unit,
            darken: self.darken.value.clamp(0.0, 1.0),
            warmup: self.glass_ready && !self.is_warm(),
            reply: self.reply.value.clamp(0.0, 1.0),
            beam: self.beam.value.clamp(0.0, 1.0),
            panel: self.panel.value.clamp(0.0, 1.0),
            wave_fade: smooth_ease(self.wave_fade / WAVE_FADE),
            wave_intro: self.wave_intro / WAVE_INTRO,
            wave_time: self.wave_time,
        }
    }
}

impl App {
    /// Answers the agent's plugin requests: the plugin list, plugin commands and plugin tools.
    pub(super) fn serve_assistant_requests(&mut self) {
        for AssistantRequest { id, body } in self.lingosoul.link.take_requests() {
            match body["request"].as_str() {
                Some("plugins") => {
                    let snapshot = self.plugin_snapshot(self.plugin_mgr.installed_plugins());
                    self.lingosoul.link.reply(id, snapshot);
                }
                Some("plugin_command") => {
                    let command = body["command"].as_str().unwrap_or_default();
                    let result = self
                        .plugin_host
                        .as_ref()
                        .map(|host| host.runtime().extensions.invoke(command, &[]));
                    let reply = match result {
                        Some(Ok(())) => json!({"ok": true, "text": "已执行"}),
                        Some(Err(_)) => {
                            json!({"ok": false, "text": format!("没有可用的命令 {command}")})
                        }
                        None => json!({"ok": false, "text": "插件系统没有启用"}),
                    };
                    self.lingosoul.link.reply(id, reply);
                }
                Some("plugin_call") => {
                    let tool = body["tool"].as_str().unwrap_or_default();
                    let args = body
                        .get("args")
                        .map_or_else(|| "{}".to_owned(), Value::to_string);
                    match self
                        .plugin_host
                        .as_ref()
                        .map(|host| host.runtime().extensions.call_assistant(tool, &args))
                    {
                        Some(Ok(sequence)) => self.lingosoul.plugin_calls.push((
                            id,
                            sequence,
                            Instant::now() + PLUGIN_CALL_TIMEOUT,
                        )),
                        _ => self.lingosoul.link.reply(
                            id,
                            json!({"ok": false, "text": format!("插件工具 {tool} 不可用")}),
                        ),
                    }
                }
                _ => self
                    .lingosoul
                    .link
                    .reply(id, json!({"ok": false, "text": "unknown request"})),
            }
        }
        if !self.lingosoul.plugin_calls.is_empty() {
            let now = Instant::now();
            let Some(host) = self.plugin_host.clone() else {
                return;
            };
            let extensions = &host.runtime().extensions;
            let link = &self.lingosoul.link;
            self.lingosoul
                .plugin_calls
                .retain(|&(id, sequence, deadline)| {
                    if let Some((ok, text)) = extensions.take_assistant_result(sequence) {
                        link.reply(id, json!({"ok": ok, "text": text}));
                        false
                    } else if now >= deadline {
                        extensions.cancel_assistant(sequence);
                        link.reply(id, json!({"ok": false, "text": "插件没有及时响应"}));
                        false
                    } else {
                        true
                    }
                });
        }
    }

    fn plugin_snapshot(&self, installed: Vec<InstalledPlugin>) -> Value {
        let plugins: Vec<Value> = installed
            .iter()
            .map(|plugin| {
                json!({
                    "id": plugin.id,
                    "name": plugin.display_name(),
                    "version": plugin.version,
                    "description": plugin.display_description(),
                    "enabled": plugin.enabled,
                })
            })
            .collect();
        let name_of = |id: &str| {
            installed.iter().find(|plugin| plugin.id == id).map_or_else(
                || id.to_owned(),
                |plugin| plugin.display_name().into_owned(),
            )
        };
        let (mut tools, mut commands, mut activities) = (Vec::new(), Vec::new(), Vec::new());
        if let Some(host) = &self.plugin_host {
            let extensions = &host.runtime().extensions;
            tools = extensions
                .assistant_tools()
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.id,
                        "plugin": tool.plugin,
                        "plugin_name": name_of(&tool.plugin),
                        "description": tool.description,
                        "schema": serde_json::from_str::<Value>(&tool.schema).unwrap_or(Value::Null),
                    })
                })
                .collect();
            commands = extensions
                .commands()
                .into_iter()
                .filter(|command| command.flags & COMMAND_ENABLED != 0)
                .map(|command| json!({"id": command.id, "title": command.title}))
                .collect();
            if let Some((_, items)) = host.activities_snapshot(u64::MAX) {
                activities = items
                    .into_iter()
                    .filter_map(|activity| match activity.content {
                        ActivityContent::Text(text) => {
                            Some(json!({"title": text.title, "body": text.body}))
                        }
                        _ => None,
                    })
                    .collect();
            }
        }
        json!({
            "ok": true,
            "plugins": plugins,
            "tools": tools,
            "commands": commands,
            "activities": activities,
        })
    }

    fn chat_island_rect(&self) -> Rect {
        self.island_rect(&self.compute_island_layout())
    }

    fn set_chat_focus(&mut self, focused: bool) {
        self.lingosoul.chat.focused = focused;
        let Some(window) = self.window else { return };
        crate::platform::window().set_plugin_input(window.id(), false, focused);
        let area = focused.then(|| {
            let rect = self.chat_island_rect();
            let (w, h) = self.lingosoul.panel_size(&self.config);
            let input = self
                .lingosoul
                .chat
                .input_rect(w, h, self.config.expanded_scale);
            let left = rect.left + (rect.width() - w) * 0.5 + input.left;
            winisland_platform::Rect {
                left: left as i32,
                top: (rect.top + input.top) as i32,
                right: (left + input.width()) as i32,
                bottom: (rect.top + input.bottom) as i32,
            }
        });
        crate::platform::window().set_text_input(window.id(), area);
    }

    pub(super) fn chat_command(&self) -> Option<winisland_platform::PluginCommand> {
        self.lingosoul
            .link
            .is_connected()
            .then(|| winisland_platform::PluginCommand {
                id: CHAT_COMMAND,
                title: winisland_core::i18n::tr("lingosoul_chat_open"),
                enabled: true,
                menu: true,
                hotkey: Some(winisland_platform::Hotkey {
                    ctrl: true,
                    alt: true,
                    shift: false,
                    win: false,
                    key: 'Y',
                }),
            })
    }

    pub(super) fn open_chat_panel(&mut self) {
        if !self.lingosoul.panel_open {
            self.toggle_chat_panel(true);
        }
        self.set_chat_focus(true);
    }

    fn toggle_chat_panel(&mut self, open: bool) {
        if open {
            self.lingosoul.open_panel();
        } else {
            if self.lingosoul.chat.focused {
                self.set_chat_focus(false);
            }
            self.lingosoul.close_panel();
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Stops the agent this island started, then downloads the latest release.
    pub(super) fn install_lingosoul(&mut self) {
        self.stop_lingosoul();
        lingosoul_install::install_latest();
        self.refresh_history_page();
    }

    pub(super) fn start_lingosoul(&mut self) {
        if self.lingosoul.link.is_connected() {
            return;
        }
        if let Err(error) = self.lingosoul.process.start() {
            log::warn!("Failed to start LingoSoul: {error}");
        }
        self.refresh_history_page();
    }

    /// Asks any connected agent to exit, and ends the one this island started.
    pub(super) fn stop_lingosoul(&mut self) {
        self.lingosoul.link.quit();
        self.lingosoul.process.stop();
        self.refresh_history_page();
    }

    /// Starts the agent after an install, or once at launch when no agent is running.
    fn manage_agent_process(&mut self) {
        let revision = crate::core::assistant::status_revision();
        if self.lingosoul.status_revision != Some(revision) {
            self.lingosoul.status_revision = Some(revision);
            self.refresh_history_page();
        }
        if lingosoul_install::take_installed().is_some() {
            self.lingosoul.autostart_tried = true;
            self.start_lingosoul();
        }
        if !self.lingosoul.autostart_tried
            && self.config.lingosoul_autostart
            && crate::core::assistant::agent_absent()
        {
            self.lingosoul.autostart_tried = true;
            self.start_lingosoul();
        }
    }

    /// Tells the assistant that the settings page rewrote its config.
    pub(super) fn reload_lingosoul(&mut self) {
        self.lingosoul.link.reload();
    }

    /// Swiping down on the compact island opens the chat while an assistant is running.
    pub(super) fn open_chat_by_swipe(&mut self) -> bool {
        if !self.lingosoul.link.is_connected() {
            return false;
        }
        self.open_chat_panel();
        true
    }

    /// Clears the chat history from settings; directly on disk when no assistant runs.
    pub(super) fn clear_lingosoul_history(&mut self) {
        if !self.lingosoul.link.clear() {
            crate::core::assistant::clear_history_file();
            self.refresh_history_page();
        }
    }

    fn refresh_history_page(&mut self) {
        if let Some(settings) = self.settings.as_mut() {
            settings.mark_items_dirty();
            settings.request_redraw();
        }
    }

    /// Keeps keyboard focus and the window size in step with the panel.
    pub(super) fn sync_chat_focus(&mut self) {
        self.manage_agent_process();
        let revision = self.lingosoul.link.chat_revision();
        if self.lingosoul.history_revision != Some(revision) {
            self.lingosoul.history_revision = Some(revision);
            self.refresh_history_page();
        }
        let extended = self.lingosoul.panel_extent(&self.config).is_some();
        if extended != self.lingosoul.window_extended {
            self.lingosoul.window_extended = extended;
            self.resize_activity_window();
        }
        let wants = self.lingosoul.chat.focused && self.lingosoul.panel_open;
        if !wants && self.lingosoul.chat.focused {
            self.set_chat_focus(false);
        }
        if let Some(at) = self.lingosoul.pending_tap
            && at.elapsed() >= DOUBLE_TAP
        {
            self.lingosoul.pending_tap = None;
            self.lingosoul.link.cancel();
        }
        let connected = self.lingosoul.link.is_connected();
        if connected && !self.lingosoul.connected {
            self.lingosoul
                .link
                .set_panel_open(self.lingosoul.panel_open);
            self.lingosoul.retention_sent = None;
        }
        self.lingosoul.connected = connected;
        let retention = &self.config.lingosoul_history_retention;
        if connected && self.lingosoul.retention_sent.as_ref() != Some(retention) {
            let days = retention.parse().unwrap_or(0);
            if self.lingosoul.link.set_retention(days) {
                self.lingosoul.retention_sent = Some(retention.clone());
            }
        }
    }

    fn chat_press(&mut self, x: f32, y: f32) -> bool {
        let rect = self.chat_island_rect();
        if !rect.contains(Point::new(x, y)) {
            if self.lingosoul.chat.focused {
                self.set_chat_focus(false);
            }
            return false;
        }
        if !self.lingosoul.panel_open {
            self.toggle_chat_panel(true);
            return true;
        }
        let (w, h) = self.lingosoul.panel_size(&self.config);
        let left = rect.left + (rect.width() - w) * 0.5;
        let hit = self
            .lingosoul
            .chat
            .hit(x - left, y - rect.top, w, h, self.config.expanded_scale);
        match hit {
            ChatHit::Close => self.toggle_chat_panel(false),
            ChatHit::Clear => {
                self.lingosoul.link.clear();
            }
            ChatHit::Input => self.set_chat_focus(true),
            ChatHit::Mic => {
                if self.lingosoul.chat.listening {
                    self.lingosoul.link.cancel();
                } else {
                    self.lingosoul.link.listen();
                }
            }
            ChatHit::Panel => {}
        }
        true
    }

    fn chat_key(&mut self, key: &Key, text: Option<&str>) {
        let chat = &mut self.lingosoul.chat;
        match key {
            Key::Enter => {
                if !chat.is_empty_input() {
                    let message = chat.take_input();
                    chat.scroll_to_end();
                    if !self.lingosoul.link.ask(&message) {
                        log::warn!("LingoSoul: no assistant to send the message to");
                    }
                }
            }
            Key::Escape => self.toggle_chat_panel(false),
            Key::Backspace => chat.backspace(),
            Key::Delete => chat.delete(),
            Key::ArrowLeft => chat.move_caret(false),
            Key::ArrowRight => chat.move_caret(true),
            Key::ArrowUp => chat.scroll_by(1.0),
            Key::ArrowDown => chat.scroll_by(-1.0),
            Key::Character(value) if self.lingosoul.ctrl => match value.to_lowercase().as_str() {
                "v" => {
                    if let Some(pasted) = crate::platform::shell().clipboard_text(MAX_PASTE) {
                        chat.insert(&pasted.replace(['\r', '\n'], " "));
                    }
                }
                "a" => chat.caret_to(true),
                _ => {}
            },
            Key::Character(value) => chat.insert(text.unwrap_or(value)),
            _ => {
                if let Some(text) = text {
                    chat.insert(text);
                }
            }
        }
    }

    /// Taps on the island while it listens: one tap stops the exchange, two switch to typing.
    /// Neither expands the island.
    fn voice_tap(&mut self, event: &PlatformEvent) -> Option<bool> {
        let (pressed, point) = match event {
            PlatformEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (px, py) = crate::utils::mouse::get_global_cursor_pos();
                (
                    *state == InputState::Pressed,
                    Point::new((px - self.geom.win_x) as f32, (py - self.geom.win_y) as f32),
                )
            }
            PlatformEvent::Touch {
                phase, position, ..
            } => (
                *phase == TouchPhase::Started,
                Point::new(position.x as f32, position.y as f32),
            ),
            _ => return None,
        };
        if !self.chat_island_rect().contains(point) {
            return None;
        }
        if pressed {
            match self.lingosoul.pending_tap.take() {
                Some(at) if at.elapsed() < DOUBLE_TAP => {
                    self.lingosoul.link.cancel();
                    self.toggle_chat_panel(true);
                    self.set_chat_focus(true);
                }
                _ => self.lingosoul.pending_tap = Some(Instant::now()),
            }
        }
        Some(true)
    }

    /// Routes island input to the reply card and the chat panel; true when consumed.
    pub(super) fn route_lingosoul_input(&mut self, event: &PlatformEvent) -> bool {
        let voice_visible = !self.lingosoul.panel_open
            && !self.expanded
            && (is_drop(self.lingosoul.state) || is_drop(self.lingosoul.requested));
        if voice_visible && let Some(consumed) = self.voice_tap(event) {
            return consumed;
        }
        let reply_visible =
            self.lingosoul.state == AssistantState::Replying && self.lingosoul.reply.value > 0.5;
        if !self.lingosoul.panel_open && !reply_visible {
            return false;
        }
        let local = || {
            let (px, py) = crate::utils::mouse::get_global_cursor_pos();
            ((px - self.geom.win_x) as f32, (py - self.geom.win_y) as f32)
        };
        let focused = self.lingosoul.chat.focused && self.lingosoul.panel_open;
        let consumed = match event {
            PlatformEvent::MouseInput {
                state: InputState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = local();
                self.chat_press(x, y)
            }
            PlatformEvent::MouseInput { .. } => {
                let (x, y) = local();
                self.chat_island_rect().contains(Point::new(x, y))
            }
            PlatformEvent::Touch {
                phase, position, ..
            } => {
                let (x, y) = (position.x as f32, position.y as f32);
                if *phase == TouchPhase::Started {
                    self.chat_press(x, y)
                } else {
                    self.chat_island_rect().contains(Point::new(x, y))
                }
            }
            PlatformEvent::MouseWheel { delta, .. } if self.lingosoul.panel_open => {
                let (x, y) = local();
                if !self.chat_island_rect().contains(Point::new(x, y)) {
                    return false;
                }
                let lines = match delta {
                    MouseWheelDelta::Lines { y, .. } => *y,
                    MouseWheelDelta::Pixels { y, .. } => {
                        *y as f32 / (48.0 * self.config.expanded_scale)
                    }
                };
                self.lingosoul.chat.scroll_by(lines);
                true
            }
            PlatformEvent::ModifiersChanged { modifiers, .. } => {
                self.lingosoul.ctrl = modifiers & MOD_CONTROL != 0;
                false
            }
            PlatformEvent::KeyInput {
                key, state, text, ..
            } if focused => {
                if *state == InputState::Pressed {
                    self.chat_key(key, text.as_deref());
                }
                true
            }
            PlatformEvent::ImeCommit { text, .. } if focused => {
                self.lingosoul.chat.insert(text);
                true
            }
            PlatformEvent::ImeComposition { text, .. } if focused => {
                self.lingosoul.chat.set_preedit(text);
                true
            }
            PlatformEvent::Focused { focused: false, .. } if focused => {
                self.set_chat_focus(false);
                false
            }
            _ => false,
        };
        if consumed && let Some(window) = &self.window {
            window.request_redraw();
        }
        consumed
    }
}

pub(super) fn lingosoul_card_radius(w: f32, h: f32, scale: f32) -> f32 {
    crate::utils::shape::expanded_island_radius(w, h, scale).min(h * 0.36)
}

/// How long a held-back reply card stays up, matching the agent's own timing.
fn reply_hold(text: &str) -> Duration {
    Duration::from_secs_f32((3.5 + 0.12 * text.chars().count() as f32).min(12.0))
}

fn smooth_ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
