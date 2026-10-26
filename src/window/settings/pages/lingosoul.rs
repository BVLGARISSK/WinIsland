use crate::core::assistant::{PROTOCOL, agent_status, local_parts, read_history};
use crate::core::lingosoul_config::{AgentPreset, LLM_PRESETS, LingoSoulConfig};
use crate::core::lingosoul_install::{self, InstallState};
use crate::utils::settings_ui::ClickResult;
use crate::utils::settings_ui::items::SettingsItem;
use toml::Value;
use winisland_core::config::AppConfigField;
use winisland_core::i18n::tr;

use super::super::{PluginSettingsRequest, PopupState, SettingsApp};
use super::{PageInput, SettingsPage};

const PREVIEW_CHARS: usize = 80;
const MODE_AGENT: &str = "agent";
const LLM_CUSTOM: &str = "custom";

/// A text value in the agent config.
#[derive(Clone, Copy)]
pub(crate) enum LingoSoulField {
    Name,
    WakeWords,
    ApiKey,
    Model,
    BaseUrl,
    Proxy,
    City,
}

/// A choice in the agent config, picked from a popup.
#[derive(Clone, Copy)]
pub(crate) enum LingoSoulChoice {
    Mode,
    Agent,
    Service,
    Permission,
}

#[derive(Clone, Copy)]
pub(crate) enum LingoSoulAction {
    Clear,
    Text(LingoSoulField),
    Choice(LingoSoulChoice),
    Tools,
    Agent(AgentPreset),
    Folder,
    Install,
    Process,
}

impl LingoSoulField {
    fn key(self) -> (&'static str, &'static str) {
        match self {
            Self::Name => ("assistant", "name"),
            Self::WakeWords => ("assistant", "wake_words"),
            Self::ApiKey => ("llm", "api_key"),
            Self::Model => ("llm", "model"),
            Self::BaseUrl => ("llm", "base_url"),
            Self::Proxy => ("llm", "proxy"),
            Self::City => ("tools", "city"),
        }
    }

    fn value(self, config: &LingoSoulConfig) -> String {
        match self {
            Self::WakeWords => config.wake_words(),
            _ => {
                let (section, key) = self.key();
                config.text(section, key)
            }
        }
    }
}

/// Options of a popup as (label, value) pairs and the selected value.
fn choice_options(
    choice: LingoSoulChoice,
    config: &LingoSoulConfig,
) -> (Vec<(String, String)>, String) {
    match choice {
        LingoSoulChoice::Mode => (
            vec![
                (tr("lingosoul_mode_api"), "api".to_owned()),
                (tr("lingosoul_mode_agent"), MODE_AGENT.to_owned()),
            ],
            if config.text("assistant", "mode") == MODE_AGENT {
                MODE_AGENT.to_owned()
            } else {
                "api".to_owned()
            },
        ),
        LingoSoulChoice::Agent => {
            let agents = config.enabled_agents();
            let current = config.text("assistant", "agent");
            let selected = agents
                .iter()
                .find(|name| name.eq_ignore_ascii_case(current.trim()))
                .or_else(|| agents.first())
                .cloned()
                .unwrap_or_default();
            (
                agents
                    .into_iter()
                    .map(|name| (name.clone(), name))
                    .collect(),
                selected,
            )
        }
        LingoSoulChoice::Service => (
            LLM_PRESETS
                .iter()
                .enumerate()
                .map(|(index, (label, ..))| (tr(label), index.to_string()))
                .chain([(tr("lingosoul_llm_custom"), LLM_CUSTOM.to_owned())])
                .collect(),
            config
                .llm_preset()
                .map_or_else(|| LLM_CUSTOM.to_owned(), |index| index.to_string()),
        ),
        LingoSoulChoice::Permission => (
            vec![
                (tr("lingosoul_agent_write"), "write".to_owned()),
                (tr("lingosoul_agent_read"), "read".to_owned()),
            ],
            if config.agent_can_write() {
                "write".to_owned()
            } else {
                "read".to_owned()
            },
        ),
    }
}

/// One line of a message for the list: no Markdown marks, cut to a preview.
fn preview(text: &str) -> String {
    let flat = text
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '-', '*', '>', '|', ' '])
        })
        .filter(|line| !line.is_empty() && !line.starts_with("```"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['*', '`', '|'], "");
    if flat.chars().count() <= PREVIEW_CHARS {
        flat
    } else {
        flat.chars().take(PREVIEW_CHARS).collect::<String>() + "…"
    }
}

fn row_choice(
    page: &mut SettingsPage<LingoSoulAction>,
    config: &LingoSoulConfig,
    label: &str,
    choice: LingoSoulChoice,
) {
    let (options, selected) = choice_options(choice, config);
    page.row_source(
        tr(label),
        options
            .into_iter()
            .map(|(label, value)| (label, value == selected))
            .collect(),
        true,
        LingoSoulAction::Choice(choice),
    );
}

fn row_field(
    page: &mut SettingsPage<LingoSoulAction>,
    config: &LingoSoulConfig,
    label: &str,
    placeholder: &str,
    field: LingoSoulField,
) {
    page.row_text(
        tr(label),
        field.value(config),
        tr(placeholder),
        matches!(field, LingoSoulField::ApiKey),
        LingoSoulAction::Text(field),
    );
}

fn build_config_sections(page: &mut SettingsPage<LingoSoulAction>, config: &LingoSoulConfig) {
    if config.broken {
        page.section(tr("lingosoul_assistant_title"));
        page.group_start();
        page.row_label(tr("lingosoul_config_broken"));
        page.row_label(
            crate::core::lingosoul_config::config_path()
                .display()
                .to_string(),
        );
        page.group_end();
        return;
    }
    let agent_mode = config.text("assistant", "mode") == MODE_AGENT;
    page.section(tr("lingosoul_assistant_title"));
    page.group_start();
    row_field(
        page,
        config,
        "lingosoul_name",
        "lingosoul_name_hint",
        LingoSoulField::Name,
    );
    row_field(
        page,
        config,
        "lingosoul_wake_words",
        "lingosoul_wake_words_hint",
        LingoSoulField::WakeWords,
    );
    row_choice(page, config, "lingosoul_mode", LingoSoulChoice::Mode);
    if agent_mode {
        if config.enabled_agents().is_empty() {
            page.row_label(tr("lingosoul_agent_none"));
        } else {
            row_choice(
                page,
                config,
                "lingosoul_mode_agent_pick",
                LingoSoulChoice::Agent,
            );
        }
    }
    page.group_end();

    if !agent_mode {
        page.section(tr("lingosoul_llm_title"));
        page.group_start();
        row_choice(
            page,
            config,
            "lingosoul_llm_service",
            LingoSoulChoice::Service,
        );
        row_field(
            page,
            config,
            "lingosoul_llm_key",
            "lingosoul_llm_key_hint",
            LingoSoulField::ApiKey,
        );
        row_field(
            page,
            config,
            "lingosoul_llm_model",
            "lingosoul_llm_model_hint",
            LingoSoulField::Model,
        );
        if config.llm_preset().is_none() {
            row_field(
                page,
                config,
                "lingosoul_llm_url",
                "lingosoul_llm_url_hint",
                LingoSoulField::BaseUrl,
            );
        }
        row_field(
            page,
            config,
            "lingosoul_llm_proxy",
            "lingosoul_llm_proxy_hint",
            LingoSoulField::Proxy,
        );
        page.group_end();

        page.section(tr("lingosoul_tools_title"));
        page.group_start();
        let tools = config.flag("tools", "enabled", true);
        page.row_switch(
            tr("lingosoul_tools_enabled"),
            tools,
            true,
            LingoSoulAction::Tools,
        );
        if tools {
            row_field(
                page,
                config,
                "lingosoul_tools_city",
                "lingosoul_tools_city_hint",
                LingoSoulField::City,
            );
        }
        page.group_end();
    }

    page.section(tr("lingosoul_agents_title"));
    page.group_start();
    for preset in AgentPreset::ALL {
        page.row_switch(
            preset.name().to_owned(),
            config.agent_enabled(preset),
            true,
            LingoSoulAction::Agent(preset),
        );
    }
    let folder = config.agent_folder();
    page.row_folder(
        tr("lingosoul_agent_folder"),
        tr("folder_select"),
        (!folder.is_empty()).then(|| tr("folder_clear")),
        (!folder.is_empty()).then_some(folder),
        true,
        LingoSoulAction::Folder,
    );
    row_choice(
        page,
        config,
        "lingosoul_agent_permission",
        LingoSoulChoice::Permission,
    );
    page.row_label(tr("lingosoul_agents_note"));
    page.group_end();
}

/// Writes the edited agent config and asks a running agent to load it.
fn save_config(app: &mut SettingsApp, config: &LingoSoulConfig) {
    if config.save() {
        app.plugin_request = Some(PluginSettingsRequest::ReloadLingoSoul);
    }
    app.mark_items_dirty();
    app.request_redraw();
}

fn commit_lingosoul_text(app: &mut SettingsApp, text: &str) {
    let Some(field) = app.lingosoul_field.take() else {
        return;
    };
    let mut config = LingoSoulConfig::load();
    let text = text.trim();
    if field.value(&config) == text {
        return;
    }
    match field {
        LingoSoulField::WakeWords => config.set_wake_words(text),
        _ => {
            let (section, key) = field.key();
            config.set(section, key, Value::String(text.to_owned()));
        }
    }
    save_config(app, &config);
}

fn select_lingosoul_choice(app: &mut SettingsApp, value: &str) {
    let Some(choice) = app.lingosoul_choice.take() else {
        return;
    };
    let mut config = LingoSoulConfig::load();
    match choice {
        LingoSoulChoice::Mode => config.set("assistant", "mode", Value::String(value.to_owned())),
        LingoSoulChoice::Agent => config.set("assistant", "agent", Value::String(value.to_owned())),
        LingoSoulChoice::Service => match value.parse::<usize>() {
            Ok(index) => config.apply_llm_preset(index),
            Err(_) => config.apply_llm_custom(),
        },
        LingoSoulChoice::Permission => config.set_agent_can_write(value == "write"),
    }
    save_config(app, &config);
}

/// One line on whether the agent is installed, running, or setting itself up.
fn program_status(installed: Option<&str>) -> String {
    let status = agent_status();
    if let InstallState::Failed(error) = lingosoul_install::state() {
        return tr("lingosoul_install_failed").replace("{}", &error);
    }
    if lingosoul_install::state() == InstallState::Working {
        return tr("lingosoul_installing");
    }
    if status.connected {
        if status.protocol.is_some_and(|protocol| protocol != PROTOCOL) {
            return tr("lingosoul_outdated");
        }
        return match &status.setup {
            Some(Ok(progress)) if *progress < 1.0 => tr("lingosoul_models_downloading")
                .replace("{}", &format!("{:.0}", progress * 100.0)),
            Some(Err(error)) => tr("lingosoul_models_failed").replace("{}", error),
            _ => {
                tr("lingosoul_running").replace("{}", status.version.as_deref().unwrap_or_default())
            }
        };
    }
    match installed {
        Some(version) => tr("lingosoul_stopped").replace("{}", version),
        None => tr("lingosoul_not_installed"),
    }
}

impl SettingsApp {
    pub(crate) fn build_lingosoul_page(&self, with_history: bool) -> SettingsPage<LingoSoulAction> {
        let mut page = SettingsPage::new();
        self.build_program_section(&mut page);
        build_config_sections(&mut page, &LingoSoulConfig::load());
        let history = if with_history {
            read_history()
        } else {
            Vec::new()
        };
        page.section(tr("lingosoul_history_title"));
        page.group_start();
        page.setting(
            &self.config,
            AppConfigField::LingosoulHistoryRetention,
            true,
        );
        page.row_button(
            tr("lingosoul_history_clear"),
            tr("lingosoul_chat_clear"),
            !with_history || !history.is_empty(),
            LingoSoulAction::Clear,
        );
        page.group_end();
        if !with_history {
            return page;
        }
        if history.is_empty() {
            page.group_start();
            page.row_label(tr("lingosoul_history_empty"));
            page.group_end();
            return page;
        }
        let you = tr("lingosoul_history_you");
        let assistant = tr("lingosoul_chat_title");
        let mut days: Vec<((i64, u32, u32), Vec<String>)> = Vec::new();
        for message in &history {
            let (year, month, day, hour, minute) = local_parts(message.at_ms);
            let row = format!(
                "{hour:02}:{minute:02}   {}：{}",
                if message.from_user { &you } else { &assistant },
                preview(&message.text)
            );
            match days.last_mut() {
                Some((key, rows)) if *key == (year, month, day) => rows.push(row),
                _ => days.push(((year, month, day), vec![row])),
            }
        }
        for ((year, month, day), rows) in days.into_iter().rev() {
            page.section(format!("{year}-{month:02}-{day:02}"));
            page.group_start();
            for row in rows {
                page.row_label(row);
            }
            page.group_end();
        }
        page
    }

    fn build_program_section(&self, page: &mut SettingsPage<LingoSoulAction>) {
        let installed = lingosoul_install::agent_exe().map(|_| {
            lingosoul_install::installed_version()
                .unwrap_or_else(|| tr("lingosoul_version_unknown"))
        });
        let running = agent_status().connected;
        let working = lingosoul_install::state() == InstallState::Working;
        page.section(tr("lingosoul_program_title"));
        page.group_start();
        page.row_label(program_status(installed.as_deref()));
        page.row_button(
            tr(if installed.is_some() {
                "lingosoul_update"
            } else {
                "lingosoul_install"
            }),
            tr(if installed.is_some() {
                "lingosoul_update_button"
            } else {
                "lingosoul_install_button"
            }),
            !working,
            LingoSoulAction::Install,
        );
        page.row_button(
            tr("lingosoul_process"),
            tr(if running {
                "lingosoul_stop_button"
            } else {
                "lingosoul_start_button"
            }),
            !working && (running || installed.is_some()),
            LingoSoulAction::Process,
        );
        page.setting(&self.config, AppConfigField::LingosoulAutostart, true);
        page.group_end();
    }

    pub(crate) fn build_lingosoul_items(&self) -> Vec<SettingsItem> {
        self.build_lingosoul_page(true).into_items()
    }

    pub(crate) fn handle_lingosoul_click(&mut self, input: PageInput) {
        let page = self.build_lingosoul_page(true);
        let result = input.hit_test(&page);
        if self.handle_setting_click(&page, &result, input) {
            return;
        }
        let Some(action) = page.action(&result).copied() else {
            return;
        };
        match (action, &result) {
            (LingoSoulAction::Clear, ClickResult::RowButton(_)) => {
                self.plugin_request = Some(PluginSettingsRequest::ClearLingoSoulHistory);
            }
            (LingoSoulAction::Install, ClickResult::RowButton(_)) => {
                // Installing means wanting it: start it with the island from now on.
                if !self.config.lingosoul_autostart {
                    self.config.lingosoul_autostart = true;
                    self.persist_settings_change();
                }
                self.plugin_request = Some(PluginSettingsRequest::InstallLingoSoul);
            }
            (LingoSoulAction::Process, ClickResult::RowButton(_)) => {
                self.plugin_request = Some(if agent_status().connected {
                    PluginSettingsRequest::StopLingoSoul
                } else {
                    PluginSettingsRequest::StartLingoSoul
                });
            }
            (LingoSoulAction::Text(field), ClickResult::TextField(index)) => {
                let config = LingoSoulConfig::load();
                self.lingosoul_field = Some(field);
                let rect = input.text_field_rect(&page, *index, self.scroll_y);
                self.begin_text_input(rect, field.value(&config), commit_lingosoul_text);
            }
            (LingoSoulAction::Choice(choice), ClickResult::SourceButton(index)) => {
                let (options, selected) = choice_options(choice, &LingoSoulConfig::load());
                if options.is_empty() {
                    return;
                }
                self.lingosoul_choice = Some(choice);
                let rect = input.popup_button_rect(&page, *index, self.scroll_y);
                let (width, height) = self.logical_window_size();
                let selected = options
                    .iter()
                    .position(|(_, value)| *value == selected)
                    .unwrap_or_default();
                let (labels, values) = options.into_iter().unzip();
                self.show_popup(PopupState::new(
                    select_lingosoul_choice,
                    rect,
                    labels,
                    values,
                    selected,
                    width,
                    height,
                ));
            }
            (LingoSoulAction::Tools, ClickResult::Switch(_)) => {
                let mut config = LingoSoulConfig::load();
                let on = config.flag("tools", "enabled", true);
                config.set("tools", "enabled", Value::Boolean(!on));
                save_config(self, &config);
            }
            (LingoSoulAction::Agent(preset), ClickResult::Switch(_)) => {
                let mut config = LingoSoulConfig::load();
                let on = config.agent_enabled(preset);
                config.set_agent_enabled(preset, !on);
                save_config(self, &config);
            }
            (LingoSoulAction::Folder, ClickResult::FolderSelect(_)) => {
                let Some(path) = rfd::FileDialog::new().pick_folder() else {
                    return;
                };
                let mut config = LingoSoulConfig::load();
                config.set_agent_folder(&path.to_string_lossy());
                save_config(self, &config);
            }
            (LingoSoulAction::Folder, ClickResult::FolderClear(_)) => {
                let mut config = LingoSoulConfig::load();
                config.set_agent_folder("");
                save_config(self, &config);
            }
            _ => {}
        }
    }
}
