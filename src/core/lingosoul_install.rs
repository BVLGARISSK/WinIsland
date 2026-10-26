//! Installs the LingoSoul agent from its GitHub releases and runs it beside the island.

use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const RELEASE_URL: &str = "https://api.github.com/repos/WinIslandProject/LingoSoul/releases/latest";
const ASSET: &str = "lingosoul-agent-windows-x64.zip";
const EXE: &str = "lingosoul-agent.exe";
const VERSION_FILE: &str = "version.txt";
const LOG_FILE: &str = "agent.log";
const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
/// Keeps the agent from opening a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Default, PartialEq)]
pub enum InstallState {
    #[default]
    Idle,
    Working,
    Failed(String),
    /// Installed this version; taken once by the island so it can start it.
    Done(String),
}

static STATE: Mutex<InstallState> = Mutex::new(InstallState::Idle);

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

pub fn install_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("LingoSoul"))
}

pub fn agent_exe() -> Option<PathBuf> {
    install_dir()
        .map(|dir| dir.join(EXE))
        .filter(|path| path.is_file())
}

pub fn installed_version() -> Option<String> {
    let text = std::fs::read_to_string(install_dir()?.join(VERSION_FILE)).ok()?;
    let version = text.trim();
    (!version.is_empty()).then(|| version.to_owned())
}

pub fn state() -> InstallState {
    STATE.lock().map(|state| state.clone()).unwrap_or_default()
}

fn set_state(next: InstallState) {
    if let Ok(mut state) = STATE.lock() {
        *state = next;
    }
    crate::platform::wake();
}

/// The version installed since the last call, so the island starts it once.
pub fn take_installed() -> Option<String> {
    let mut state = STATE.lock().ok()?;
    let InstallState::Done(version) = state.clone() else {
        return None;
    };
    *state = InstallState::Idle;
    Some(version)
}

/// Downloads and unpacks the latest release in the background.
pub fn install_latest() {
    if state() == InstallState::Working {
        return;
    }
    set_state(InstallState::Working);
    let spawned = std::thread::Builder::new()
        .name("lingosoul-install".into())
        .spawn(|| {
            let next = match install() {
                Ok(version) => {
                    log::info!("Installed LingoSoul {version}");
                    InstallState::Done(version)
                }
                Err(error) => {
                    log::warn!("LingoSoul install failed: {error}");
                    InstallState::Failed(error)
                }
            };
            set_state(next);
        });
    if let Err(error) = spawned {
        set_state(InstallState::Failed(error.to_string()));
    }
}

fn install() -> Result<String, String> {
    let dir = install_dir().ok_or("no local app data folder")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let (version, bytes) = runtime.block_on(download())?;
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    unpack(&bytes, &dir)?;
    std::fs::write(dir.join(VERSION_FILE), &version)
        .map_err(|error| format!("{VERSION_FILE}: {error}"))?;
    Ok(version)
}

async fn download() -> Result<(String, Vec<u8>), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("WinIsland-LingoSoul")
        .build()
        .map_err(|error| error.to_string())?;
    let release: Release = client
        .get(RELEASE_URL)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == ASSET)
        .ok_or_else(|| format!("{} has no {ASSET}", release.tag_name))?;
    let expected = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase)
        .ok_or("the release does not list a checksum")?;
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PACKAGE_BYTES)
    {
        return Err("the package is too large".into());
    }
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != expected {
        return Err("the package checksum does not match".into());
    }
    let version = release.tag_name.trim_start_matches('v').to_owned();
    Ok((version, bytes.to_vec()))
}

/// Writes the package's top-level files into `dir`; nothing outside it is touched.
fn unpack(bytes: &[u8], dir: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("invalid package: {error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("invalid package: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = entry
            .enclosed_name()
            .and_then(|path| path.file_name().map(PathBuf::from))
        else {
            continue;
        };
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|error| format!("{}: {error}", name.display()))?;
        let target = dir.join(&name);
        // A running agent keeps its exe locked; it can still be renamed out of the way.
        if target.exists() && std::fs::write(&target, &data).is_ok() {
            continue;
        }
        if target.exists() {
            let old = target.with_extension("old");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&target, old);
        }
        std::fs::write(&target, &data).map_err(|error| format!("{}: {error}", name.display()))?;
    }
    Ok(())
}

/// The agent process this island started; other agents are left alone.
#[derive(Default)]
pub struct AgentProcess {
    child: Option<Child>,
}

impl AgentProcess {
    pub fn running(&mut self) -> bool {
        match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            Some(_) => {
                self.child = None;
                false
            }
            None => false,
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.running() {
            return Ok(());
        }
        let exe = agent_exe().ok_or("LingoSoul is not installed")?;
        let dir = exe.parent().ok_or("invalid install folder")?;
        let log = std::fs::File::create(dir.join(LOG_FILE)).map_err(|error| error.to_string())?;
        let errors = log.try_clone().map_err(|error| error.to_string())?;
        let child = Command::new(&exe)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(errors)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("{}: {error}", exe.display()))?;
        log::info!("Started LingoSoul agent, pid {}", child.id());
        self.child = Some(child);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
