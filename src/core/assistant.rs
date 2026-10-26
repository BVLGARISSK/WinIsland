use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

const PIPE_NAME: &str = r"\\.\pipe\winisland-lingosoul";
const RECONNECT_DELAY: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AssistantState {
    #[default]
    Idle,
    Listening,
    Thinking,
    Speaking,
    Replying,
}

impl AssistantState {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "idle" => Self::Idle,
            "listening" => Self::Listening,
            "thinking" => Self::Thinking,
            "speaking" => Self::Speaking,
            "replying" => Self::Replying,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssistantSnapshot {
    pub state: AssistantState,
    pub level: f32,
    pub text: Arc<str>,
    pub listening: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistantCommand {
    /// Start the countdown timer; zero cancels it.
    Timer(u64),
    /// Open the chat panel.
    OpenChat,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub from_user: bool,
    pub text: Arc<str>,
    pub at_ms: u64,
}

/// Something the agent asked the island to look up or do; answered with `AssistantLink::reply`.
pub struct AssistantRequest {
    pub id: u64,
    pub body: Value,
}

#[derive(Default)]
struct Chat {
    revision: u64,
    messages: Vec<ChatMessage>,
}

#[derive(Default)]
struct Shared {
    snapshot: Mutex<AssistantSnapshot>,
    commands: Mutex<Vec<AssistantCommand>>,
    requests: Mutex<Vec<AssistantRequest>>,
    chat: Mutex<Chat>,
    writer: Mutex<Option<std::fs::File>>,
    voice: std::sync::atomic::AtomicBool,
}

pub struct AssistantLink {
    shared: Arc<Shared>,
}

impl AssistantLink {
    pub fn start() -> Self {
        let shared = Arc::new(Shared::default());
        let sink = shared.clone();
        let _ = std::thread::Builder::new()
            .name("assistant-link".into())
            .spawn(move || run(&sink));
        Self { shared }
    }

    pub fn snapshot(&self) -> AssistantSnapshot {
        self.shared
            .snapshot
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn take_commands(&self) -> Vec<AssistantCommand> {
        self.shared
            .commands
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    pub fn take_requests(&self) -> Vec<AssistantRequest> {
        self.shared
            .requests
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    pub fn is_connected(&self) -> bool {
        self.shared
            .writer
            .lock()
            .is_ok_and(|writer| writer.is_some())
    }

    pub fn chat_revision(&self) -> u64 {
        self.shared.chat.lock().map_or(0, |chat| chat.revision)
    }

    pub fn chat(&self) -> Vec<ChatMessage> {
        self.shared
            .chat
            .lock()
            .map(|chat| chat.messages.clone())
            .unwrap_or_default()
    }

    /// Sends a typed message to the assistant; false when no assistant is running.
    pub fn ask(&self, text: &str) -> bool {
        self.send(&serde_json::json!({"type": "ask", "text": text}))
    }

    pub fn clear(&self) -> bool {
        self.send(&serde_json::json!({"type": "clear"}))
    }

    /// Whether the assistant can listen; false for a chat-only assistant.
    pub fn voice_available(&self) -> bool {
        self.shared.voice.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Starts listening without a wake word.
    pub fn listen(&self) -> bool {
        self.send(&serde_json::json!({"type": "listen"}))
    }

    /// Stops listening and ends the voice exchange.
    pub fn cancel(&self) -> bool {
        self.send(&serde_json::json!({"type": "cancel"}))
    }

    pub fn set_panel_open(&self, open: bool) -> bool {
        self.send(&serde_json::json!({"type": "panel", "open": open}))
    }

    /// Days to keep chat history; zero keeps it forever.
    pub fn set_retention(&self, days: u64) -> bool {
        self.send(&serde_json::json!({"type": "retention", "days": days}))
    }

    pub fn reply(&self, id: u64, mut body: Value) {
        body["type"] = "reply".into();
        body["id"] = id.into();
        self.send(&body);
    }

    fn send(&self, value: &Value) -> bool {
        let Ok(mut writer) = self.shared.writer.lock() else {
            return false;
        };
        let Some(file) = writer.as_mut() else {
            return false;
        };
        if writeln!(file, "{value}")
            .and_then(|()| file.flush())
            .is_err()
        {
            *writer = None;
            return false;
        }
        true
    }
}

fn run(shared: &Shared) {
    let name = std::env::var("LINGOSOUL_PIPE").unwrap_or_else(|_| PIPE_NAME.into());
    loop {
        if let Ok(pipe) = std::fs::File::open(&name) {
            let inbound = (0..20).find_map(|_| {
                let pipe = std::fs::OpenOptions::new()
                    .write(true)
                    .open(format!("{name}-in"))
                    .ok();
                if pipe.is_none() {
                    std::thread::sleep(Duration::from_millis(100));
                }
                pipe
            });
            if let Ok(mut writer) = shared.writer.lock() {
                *writer = inbound;
            }
            for line in BufReader::new(pipe).lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                handle(shared, &value);
            }
            shared
                .voice
                .store(false, std::sync::atomic::Ordering::Release);
            if let Ok(mut writer) = shared.writer.lock() {
                *writer = None;
            }
            publish(shared, AssistantSnapshot::default());
        }
        std::thread::sleep(RECONNECT_DELAY);
    }
}

fn handle(shared: &Shared, value: &Value) {
    if let Some(voice) = value.pointer("/hello/voice").and_then(Value::as_bool) {
        shared
            .voice
            .store(voice, std::sync::atomic::Ordering::Release);
        crate::platform::wake();
        return;
    }
    if let Some(command) = parse_command(value) {
        if let Ok(mut queue) = shared.commands.lock() {
            queue.push(command);
        }
        crate::platform::wake();
    } else if let Some(history) = value.get("history").and_then(Value::as_array) {
        if let Ok(mut chat) = shared.chat.lock() {
            chat.messages = history.iter().filter_map(parse_message).collect();
            chat.revision = chat.revision.wrapping_add(1);
        }
        crate::platform::wake();
    } else if let Some(message) = value.get("message").and_then(parse_message) {
        if let Ok(mut chat) = shared.chat.lock() {
            chat.messages.push(message);
            chat.revision = chat.revision.wrapping_add(1);
        }
        crate::platform::wake();
    } else if value.get("request").is_some() {
        let id = value.get("id").and_then(Value::as_u64).unwrap_or(0);
        if let Ok(mut queue) = shared.requests.lock() {
            queue.push(AssistantRequest {
                id,
                body: value.clone(),
            });
        }
        crate::platform::wake();
    } else if let Some(next) = parse(value) {
        publish(shared, next);
    }
}

fn publish(shared: &Shared, next: AssistantSnapshot) {
    let changed = shared.snapshot.lock().is_ok_and(|mut current| {
        let changed = *current != next;
        *current = next;
        changed
    });
    if changed {
        crate::platform::wake();
    }
}

fn parse_message(value: &Value) -> Option<ChatMessage> {
    Some(ChatMessage {
        from_user: value.get("role")?.as_str()? == "user",
        text: Arc::from(value.get("text")?.as_str()?),
        at_ms: value.get("at").and_then(Value::as_u64).unwrap_or(0),
    })
}

fn parse_command(value: &Value) -> Option<AssistantCommand> {
    match value.get("command")?.as_str()? {
        "timer" => Some(AssistantCommand::Timer(
            value.get("seconds")?.as_u64()?.min(24 * 3600),
        )),
        "open_chat" => Some(AssistantCommand::OpenChat),
        _ => None,
    }
}

fn parse(value: &Value) -> Option<AssistantSnapshot> {
    let state = AssistantState::parse(value.get("state")?.as_str()?)?;
    let level = value.get("level").and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(AssistantSnapshot {
        state,
        level: if level.is_finite() {
            level.clamp(0.0, 1.0)
        } else {
            0.0
        },
        text: Arc::from(text),
        listening: value
            .get("listening")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// The assistant's chat history file, next to its config.
pub fn history_path() -> std::path::PathBuf {
    let config = std::env::var_os("LINGOSOUL_CONFIG").map_or_else(
        || {
            std::path::PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
                .join(".winisland-lingosoul")
                .join("config.toml")
        },
        std::path::PathBuf::from,
    );
    config.with_file_name("history.jsonl")
}

pub fn read_history() -> Vec<ChatMessage> {
    std::fs::read_to_string(history_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|line| parse_message(&serde_json::from_str(line).ok()?))
        .collect()
}

/// Empties the history file; used when no assistant is running to do it.
pub fn clear_history_file() {
    let _ = std::fs::write(history_path(), "");
}

/// Local (year, month, day, hour, minute) of a Unix timestamp in milliseconds.
pub fn local_parts(at_ms: u64) -> (i64, u32, u32, u32, u32) {
    let now = crate::platform::shell().local_datetime();
    let utc_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let local_now = days_from_civil(
        i64::from(now.year),
        u32::from(now.month),
        u32::from(now.day),
    ) * 86_400
        + i64::from(now.hour) * 3600
        + i64::from(now.minute) * 60
        + i64::from(now.second);
    let offset = ((local_now - utc_now) as f64 / 900.0).round() as i64 * 900;
    let seconds = i64::try_from(at_ms / 1000).unwrap_or(i64::MAX - offset.max(0)) + offset;
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    (
        year,
        month,
        day,
        (time / 3600) as u32,
        (time % 3600 / 60) as u32,
    )
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
