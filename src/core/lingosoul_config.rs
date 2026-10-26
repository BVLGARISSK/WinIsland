//! Reads and edits the LingoSoul agent's `config.toml` for the settings page.

use std::path::PathBuf;

use toml::{Table, Value};

/// Agent clients the settings page can switch on without editing the file by hand.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AgentPreset {
    ClaudeCode,
    Codex,
}

impl AgentPreset {
    pub const ALL: [Self; 2] = [Self::ClaudeCode, Self::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    fn timeout_secs(self) -> i64 {
        match self {
            Self::ClaudeCode => 300,
            Self::Codex => 600,
        }
    }

    /// Start and continue arguments; `write` lets the agent change files in its folder.
    fn args(self, write: bool) -> (Vec<&'static str>, Vec<&'static str>) {
        match self {
            Self::ClaudeCode => {
                let tools = if write {
                    "Read,Edit,Bash"
                } else {
                    "Read,Grep,Glob"
                };
                (
                    vec!["-p", "--output-format", "text", "--allowedTools", tools],
                    vec![
                        "-p",
                        "--continue",
                        "--output-format",
                        "text",
                        "--allowedTools",
                        tools,
                    ],
                )
            }
            Self::Codex => {
                let (sandbox, resume) = if write {
                    ("workspace-write", "sandbox_mode=\"workspace-write\"")
                } else {
                    ("read-only", "sandbox_mode=\"read-only\"")
                };
                (
                    vec![
                        "exec",
                        "--skip-git-repo-check",
                        "--sandbox",
                        sandbox,
                        "--color",
                        "never",
                        "-",
                    ],
                    vec![
                        "exec",
                        "resume",
                        "--last",
                        "--skip-git-repo-check",
                        "-c",
                        resume,
                        "-",
                    ],
                )
            }
        }
    }
}

/// Built-in AI services: label key, provider, base URL and a default model.
pub const LLM_PRESETS: [(&str, &str, &str, &str); 5] = [
    (
        "lingosoul_llm_deepseek",
        "openai",
        "https://api.deepseek.com/v1",
        "deepseek-chat",
    ),
    (
        "lingosoul_llm_qwen",
        "openai",
        "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "qwen-plus",
    ),
    (
        "lingosoul_llm_kimi",
        "openai",
        "https://api.moonshot.cn/v1",
        "moonshot-v1-8k",
    ),
    (
        "lingosoul_llm_ollama",
        "openai",
        "http://localhost:11434/v1",
        "",
    ),
    ("lingosoul_llm_claude", "claude", "", ""),
];

pub fn config_path() -> PathBuf {
    std::env::var_os("LINGOSOUL_CONFIG").map_or_else(
        || {
            PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
                .join(".winisland-lingosoul")
                .join("config.toml")
        },
        PathBuf::from,
    )
}

pub struct LingoSoulConfig {
    table: Table,
    /// The file exists but does not parse; saving would throw the user's text away.
    pub broken: bool,
}

impl LingoSoulConfig {
    pub fn load() -> Self {
        match std::fs::read_to_string(config_path()) {
            Ok(text) => match text.parse::<Table>() {
                Ok(table) => Self {
                    table,
                    broken: false,
                },
                Err(error) => {
                    log::warn!("LingoSoul config does not parse: {error}");
                    Self {
                        table: Table::new(),
                        broken: true,
                    }
                }
            },
            Err(_) => Self {
                table: Table::new(),
                broken: false,
            },
        }
    }

    pub fn save(&self) -> bool {
        if self.broken {
            return false;
        }
        let text = match toml::to_string(&self.table) {
            Ok(text) => text,
            Err(error) => {
                log::warn!("Failed to write LingoSoul config: {error}");
                return false;
            }
        };
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text = format!(
            "# LingoSoul (灵言) agent settings, also edited from the WinIsland settings window.\n\n{text}"
        );
        match std::fs::write(&path, text) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("Failed to save LingoSoul config: {error}");
                false
            }
        }
    }

    fn section(&self, section: &str) -> Option<&Table> {
        self.table.get(section).and_then(Value::as_table)
    }

    /// Runs `edit` on a top-level table, creating it when missing.
    fn edit_section(&mut self, section: &str, edit: impl FnOnce(&mut Table)) {
        let mut table = match self.table.remove(section) {
            Some(Value::Table(table)) => table,
            _ => Table::new(),
        };
        edit(&mut table);
        self.table.insert(section.to_owned(), Value::Table(table));
    }

    pub fn text(&self, section: &str, key: &str) -> String {
        self.section(section)
            .and_then(|table| table.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    pub fn flag(&self, section: &str, key: &str, default: bool) -> bool {
        self.section(section)
            .and_then(|table| table.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    pub fn set(&mut self, section: &str, key: &str, value: Value) {
        self.edit_section(section, |table| {
            table.insert(key.to_owned(), value);
        });
    }

    /// Wake words joined for a one-line field.
    pub fn wake_words(&self) -> String {
        self.section("assistant")
            .and_then(|table| table.get("wake_words"))
            .and_then(Value::as_array)
            .map(|words| {
                words
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    pub fn set_wake_words(&mut self, text: &str) {
        let words = text
            .split([',', '，', '、', ';', '；'])
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .map(|word| Value::String(word.to_owned()))
            .collect();
        self.set("assistant", "wake_words", Value::Array(words));
    }

    /// Index of the selected entry in `LLM_PRESETS`, or None for a custom service.
    pub fn llm_preset(&self) -> Option<usize> {
        let provider = self.text("llm", "provider");
        let base_url = self.text("llm", "base_url");
        let base_url = base_url.trim_end_matches('/');
        LLM_PRESETS.iter().position(|(_, preset_provider, url, _)| {
            if *preset_provider == "claude" {
                provider.is_empty() || provider == "claude"
            } else {
                provider == *preset_provider && base_url.eq_ignore_ascii_case(url)
            }
        })
    }

    pub fn apply_llm_preset(&mut self, index: usize) {
        let Some((_, provider, base_url, model)) = LLM_PRESETS.get(index) else {
            return;
        };
        self.edit_section("llm", |llm| {
            llm.insert("provider".into(), Value::String((*provider).into()));
            llm.insert("base_url".into(), Value::String((*base_url).into()));
            llm.insert("model".into(), Value::String((*model).into()));
        });
    }

    /// A custom service is any OpenAI-compatible endpoint; its URL and model stay as typed.
    pub fn apply_llm_custom(&mut self) {
        self.set("llm", "provider", Value::String("openai".into()));
    }

    fn agents(&self) -> &[Value] {
        self.table
            .get("agents")
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }

    fn agent(&self, preset: AgentPreset) -> Option<&Table> {
        self.agents()
            .iter()
            .filter_map(Value::as_table)
            .find(|agent| is_preset(agent, preset))
    }

    pub fn agent_enabled(&self, preset: AgentPreset) -> bool {
        self.agent(preset).is_some_and(|agent| {
            agent
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
    }

    /// Names of every enabled agent, in file order.
    pub fn enabled_agents(&self) -> Vec<String> {
        self.agents()
            .iter()
            .filter_map(Value::as_table)
            .filter(|agent| {
                agent
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            })
            .filter_map(|agent| agent.get("name").and_then(Value::as_str))
            .map(str::to_owned)
            .collect()
    }

    /// Folder the preset agents work in.
    pub fn agent_folder(&self) -> String {
        AgentPreset::ALL
            .iter()
            .find_map(|preset| {
                self.agent(*preset)
                    .and_then(|agent| agent.get("cwd"))
                    .and_then(Value::as_str)
                    .filter(|cwd| !cwd.is_empty())
            })
            .or_else(|| {
                self.section("agent_access")
                    .and_then(|table| table.get("cwd"))
                    .and_then(Value::as_str)
            })
            .unwrap_or_default()
            .to_owned()
    }

    /// Whether the preset agents may change files.
    pub fn agent_can_write(&self) -> bool {
        self.flag("agent_access", "write", true)
    }

    pub fn set_agent_enabled(&mut self, preset: AgentPreset, enabled: bool) {
        let folder = self.agent_folder();
        let write = self.agent_can_write();
        let mut added = false;
        self.edit_agents(|agents| {
            if let Some(agent) = find_preset(agents, preset) {
                agent.insert("enabled".into(), Value::Boolean(enabled));
            } else if enabled {
                let mut agent = Table::new();
                agent.insert("name".into(), Value::String(preset.name().into()));
                agent.insert("command".into(), Value::String(preset.command().into()));
                agent.insert("timeout_secs".into(), Value::Integer(preset.timeout_secs()));
                agent.insert("cwd".into(), Value::String(folder));
                agents.push(Value::Table(agent));
                added = true;
            }
        });
        if added {
            self.write_preset_args(preset, write);
        }
    }

    pub fn set_agent_folder(&mut self, folder: &str) {
        self.set("agent_access", "cwd", Value::String(folder.to_owned()));
        self.edit_agents(|agents| {
            for preset in AgentPreset::ALL {
                if let Some(agent) = find_preset(agents, preset) {
                    agent.insert("cwd".into(), Value::String(folder.to_owned()));
                }
            }
        });
    }

    pub fn set_agent_can_write(&mut self, write: bool) {
        self.set("agent_access", "write", Value::Boolean(write));
        for preset in AgentPreset::ALL {
            self.write_preset_args(preset, write);
        }
    }

    fn write_preset_args(&mut self, preset: AgentPreset, write: bool) {
        let (args, continue_args) = preset.args(write);
        let strings = |list: Vec<&str>| {
            Value::Array(
                list.into_iter()
                    .map(|arg| Value::String(arg.into()))
                    .collect(),
            )
        };
        self.edit_agents(|agents| {
            if let Some(agent) = find_preset(agents, preset) {
                agent.insert("args".into(), strings(args));
                agent.insert("continue_args".into(), strings(continue_args));
            }
        });
    }

    /// Runs `edit` on the `[[agents]]` list, creating it when missing.
    fn edit_agents(&mut self, edit: impl FnOnce(&mut Vec<Value>)) {
        let mut agents = match self.table.remove("agents") {
            Some(Value::Array(agents)) => agents,
            _ => Vec::new(),
        };
        edit(&mut agents);
        self.table.insert("agents".to_owned(), Value::Array(agents));
    }
}

fn is_preset(agent: &Table, preset: AgentPreset) -> bool {
    agent
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.eq_ignore_ascii_case(preset.name()))
}

fn find_preset(agents: &mut [Value], preset: AgentPreset) -> Option<&mut Table> {
    agents
        .iter_mut()
        .filter_map(Value::as_table_mut)
        .find(|agent| is_preset(agent, preset))
}
