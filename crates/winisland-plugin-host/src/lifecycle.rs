use std::mem::ManuallyDrop;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use winisland_plugin_api::abi::{ABI_VERSION_2, PluginCreateInfoV2, PluginHostV2, PluginStatus};
use winisland_plugin_api::types::v2::ResourceId;
use winisland_plugin_api::types::v2::context::{
    HostStateV2, MEDIA_COMMAND_NEXT, MEDIA_COMMAND_PREVIOUS, MEDIA_COMMAND_SEEK,
    MEDIA_COMMAND_TOGGLE_PLAY, MEDIA_CONTROL_NEXT, MEDIA_CONTROL_PREVIOUS, MEDIA_CONTROL_SEEK,
    MEDIA_CONTROL_TOGGLE_PLAY, MediaCommandV2,
};
use winisland_plugin_api::types::v2::settings::SettingsChangeV2;
use winisland_plugin_api::types::v2::{PluginToken, WidgetId};

use crate::PluginHostError;
use crate::fault::{PluginCallGuard, PluginIdentity};
use crate::loader::PluginLibrary;
use crate::runtime::HostRuntime;

mod lyrics;
pub use lyrics::LyricsBridge;

/// How long shutdown waits for the plugin worker's current call, matching the limit the host
/// gives other plugin calls.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

enum WorkerEvent {
    LyricsTransform {
        token: PluginToken,
        request: lyrics::TransformRequest,
        response: Sender<Result<String, PluginStatus>>,
    },
    MediaCommand {
        id: u64,
        command: u32,
        position_ms: u64,
        completion: Option<(PluginToken, u64)>,
    },
    HostState {
        id: u64,
        snapshot: Box<HostStateV2>,
    },
    SettingsChange {
        id: u64,
        key: String,
        value: String,
        response: Sender<PluginStatus>,
    },
}

pub struct PluginInstance {
    library: ManuallyDrop<PluginLibrary>,
    handle: usize,
    token: PluginToken,
    identity: Arc<PluginIdentity>,
    stopped: bool,
    stop_worker: Arc<AtomicBool>,
    tick_widgets: Arc<Mutex<Vec<WidgetId>>>,
    tick_failure: Arc<Mutex<Option<PluginStatus>>>,
    worker: Option<JoinHandle<()>>,
    event_tx: Option<Sender<WorkerEvent>>,
}

impl PluginInstance {
    /// # Safety
    /// `host_api` must remain allocated through the plugin's shutdown and destroy calls.
    pub unsafe fn create(
        library: PluginLibrary,
        token: PluginToken,
        host_api: *const PluginHostV2,
        marker_dir: PathBuf,
    ) -> Result<Self, PluginHostError> {
        if host_api.is_null() || token == PluginToken::INVALID {
            return Err(PluginHostError::Invalid(
                "invalid host table or plugin token".into(),
            ));
        }
        let create = library.descriptor.create.ok_or_else(|| {
            PluginHostError::Invalid("plugin descriptor has no create callback".into())
        })?;
        let identity = Arc::new(PluginIdentity {
            id: library.metadata().id.clone(),
            version: library.metadata().version.clone(),
            marker_dir,
        });
        let info = PluginCreateInfoV2 {
            struct_size: std::mem::size_of::<PluginCreateInfoV2>() as u32,
            abi_version: ABI_VERSION_2,
            plugin_token: token,
            host_api,
        };
        let mut handle = std::ptr::null_mut();
        // SAFETY: The descriptor and host table satisfy the ABI v2 contract.
        let result = {
            let _guard = PluginCallGuard::enter(&identity);
            unsafe { create(&info, &mut handle) }
        };
        if result != PluginStatus::Ok || handle.is_null() {
            if !handle.is_null() {
                let can_unload = library.descriptor.shutdown.is_some_and(|shutdown| {
                    // SAFETY: A non-null partial handle was returned by the plugin.
                    let _guard = PluginCallGuard::enter(&identity);
                    unsafe { shutdown(handle) == PluginStatus::Ok }
                });
                if can_unload {
                    if let Some(destroy) = library.descriptor.destroy {
                        // SAFETY: Successful shutdown permits destroying the partial instance.
                        let _guard = PluginCallGuard::enter(&identity);
                        unsafe { destroy(handle) };
                    }
                } else {
                    std::mem::forget(library);
                    return Err(PluginHostError::RetainedDll(format!(
                        "plugin create failed ({result:?}); shutdown failed, DLL kept loaded"
                    )));
                }
            }
            return Err(PluginHostError::Execution(format!(
                "plugin create failed ({result:?}) or returned a null handle"
            )));
        }
        Ok(Self {
            library: ManuallyDrop::new(library),
            handle: handle as usize,
            token,
            identity,
            stopped: false,
            stop_worker: Arc::new(AtomicBool::new(false)),
            tick_widgets: Arc::new(Mutex::new(Vec::new())),
            tick_failure: Arc::new(Mutex::new(None)),
            worker: None,
            event_tx: None,
        })
    }

    pub fn token(&self) -> PluginToken {
        self.token
    }

    pub fn library(&self) -> &PluginLibrary {
        &self.library
    }

    pub fn set_tick_widgets(&self, widgets: Vec<WidgetId>) -> Result<(), PluginHostError> {
        let mut current = self.tick_widgets.lock().map_err(|_| {
            PluginHostError::Worker("plugin tick widget list lock is poisoned".into())
        })?;
        *current = widgets;
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        Ok(())
    }

    pub fn tick_failure(&self) -> Option<PluginStatus> {
        self.tick_failure.lock().ok().and_then(|failure| *failure)
    }

    pub fn start_tick_worker(
        &mut self,
        period: Duration,
        runtime: &HostRuntime,
    ) -> Result<(), PluginHostError> {
        if self.stopped || self.worker.is_some() || period.is_zero() {
            return Err(PluginHostError::Worker("tick worker cannot start".into()));
        }
        let tick = self.library.descriptor.on_tick;
        let (event_tx, event_rx) = mpsc::channel();
        let stop = Arc::clone(&self.stop_worker);
        let widgets = Arc::clone(&self.tick_widgets);
        let failure = Arc::clone(&self.tick_failure);
        let handle = self.handle;
        let token = self.token;
        let identity = Arc::clone(&self.identity);
        let runtime_address = runtime as *const HostRuntime as usize;
        self.worker = Some(
            thread::Builder::new()
                .name(format!("plugin-tick-{}", self.library.metadata().id))
                .spawn(move || {
                    // SAFETY: The pinned runtime outlives this joined worker.
                    let runtime = unsafe { &*(runtime_address as *const HostRuntime) };
                    runtime.extensions.attach_worker(token, thread::current());
                    let mut previous = Instant::now();
                    let mut next_tick = previous;
                    while !stop.load(Ordering::Acquire) {
                        for _ in 0..256 {
                            // Events queued behind a long lyrics call are dropped once shutdown
                            // starts instead of delaying it.
                            if stop.load(Ordering::Acquire) {
                                break;
                            }
                            let Ok(event) = event_rx.try_recv() else {
                                break;
                            };
                            // SAFETY: The pinned runtime outlives this joined worker.
                            let runtime = unsafe { &*(runtime_address as *const HostRuntime) };
                            dispatch_event(runtime, &identity, event);
                        }
                        runtime.extensions.dispatch(token, &identity);
                        let frame_start = Instant::now();
                        let dt = frame_start.duration_since(previous).as_secs_f64();
                        let due = frame_start >= next_tick;
                        let mut ticking = false;
                        if let Some(tick) = tick {
                            let snapshot = match widgets.lock() {
                                Ok(guard) => guard.clone(),
                                Err(_) => break,
                            };
                            for widget in snapshot {
                                let surface = runtime
                                    .state
                                    .lock()
                                    .ok()
                                    .and_then(|state| {
                                        state
                                            .widgets
                                            .get(&widget.get())
                                            .map(|r| r.surface.is_some())
                                    })
                                    .unwrap_or(false);
                                if !runtime.extensions.should_tick(widget.get(), surface) {
                                    continue;
                                }
                                ticking = true;
                                if !due {
                                    continue;
                                }
                                if stop.load(Ordering::Acquire) {
                                    break;
                                }
                                // SAFETY: The instance remains alive until this worker is joined.
                                let status = {
                                    let _guard = PluginCallGuard::enter(&identity);
                                    unsafe { tick(handle as *mut _, widget, dt) }
                                };
                                if status != PluginStatus::Ok {
                                    if let Ok(mut failure) = failure.lock() {
                                        *failure = Some(status);
                                    }
                                    stop.store(true, Ordering::Release);
                                    break;
                                }
                            }
                        }
                        if ticking && due {
                            previous = frame_start;
                            next_tick = Instant::now() + period;
                        }
                        let remaining = runtime.extensions.wait_duration(
                            token,
                            ticking,
                            next_tick.saturating_duration_since(Instant::now()),
                        );
                        if !remaining.is_zero() {
                            thread::park_timeout(remaining);
                        }
                    }
                    while let Ok(event) = event_rx.try_recv() {
                        if let WorkerEvent::MediaCommand {
                            completion: Some((caller, sequence)),
                            ..
                        } = event
                        {
                            runtime.extensions.complete(
                                caller,
                                sequence,
                                PluginStatus::StaleHandle,
                            );
                        }
                    }
                })
                .map_err(|error| PluginHostError::Worker(error.to_string()))?,
        );
        if let Some(worker) = &self.worker {
            runtime
                .extensions
                .attach_worker(self.token, worker.thread().clone());
        }
        self.event_tx = Some(event_tx);
        Ok(())
    }

    pub fn queue_media_command(
        &self,
        id: u64,
        command: u32,
        position_ms: u64,
        completion: Option<(PluginToken, u64)>,
    ) -> Result<(), PluginHostError> {
        if self.stopped || self.stop_worker.load(Ordering::Acquire) {
            return Err(PluginHostError::Worker("plugin worker has stopped".into()));
        }
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        self.event_tx
            .as_ref()
            .ok_or_else(|| PluginHostError::Worker("plugin worker is unavailable".into()))?
            .send(WorkerEvent::MediaCommand {
                id,
                command,
                position_ms,
                completion,
            })
            .map_err(|_| PluginHostError::Worker("plugin worker has exited".into()))
            .inspect(|_| {
                if let Some(worker) = &self.worker {
                    worker.thread().unpark();
                }
            })
    }

    pub fn queue_host_state(&self, id: u64, snapshot: HostStateV2) -> Result<(), PluginHostError> {
        if self.stopped || self.stop_worker.load(Ordering::Acquire) {
            return Err(PluginHostError::Worker("plugin worker has stopped".into()));
        }
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        self.event_tx
            .as_ref()
            .ok_or_else(|| PluginHostError::Worker("plugin worker is unavailable".into()))?
            .send(WorkerEvent::HostState {
                id,
                snapshot: Box::new(snapshot),
            })
            .map_err(|_| PluginHostError::Worker("plugin worker has exited".into()))
            .inspect(|_| {
                if let Some(worker) = &self.worker {
                    worker.thread().unpark();
                }
            })
    }

    pub fn call_settings_change(
        &self,
        id: u64,
        key: &str,
        value: &str,
    ) -> Result<PluginStatus, PluginHostError> {
        if self.stopped || self.stop_worker.load(Ordering::Acquire) {
            return Err(PluginHostError::Worker("plugin worker has stopped".into()));
        }
        let (response, receive) = mpsc::channel();
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        self.event_tx
            .as_ref()
            .ok_or_else(|| PluginHostError::Worker("plugin worker is unavailable".into()))?
            .send(WorkerEvent::SettingsChange {
                id,
                key: key.to_string(),
                value: value.to_string(),
                response,
            })
            .map_err(|_| PluginHostError::Worker("plugin worker has exited".into()))
            .inspect(|_| {
                if let Some(worker) = &self.worker {
                    worker.thread().unpark();
                }
            })?;
        receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| {
                PluginHostError::Worker(format!("settings callback timed out: {error}"))
            })
    }

    pub fn shutdown(&mut self) -> Result<(), PluginHostError> {
        if self.stopped {
            return Ok(());
        }
        self.stop_worker.store(true, Ordering::Release);
        self.event_tx.take();
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
            // A secondary lyrics call may run for minutes and cannot be interrupted. Shutdown
            // usually runs on the UI thread, so it fails instead of freezing the window; the
            // plugin stays loaded and a later shutdown succeeds once the call has returned.
            let deadline = Instant::now() + SHUTDOWN_WAIT;
            while !worker.is_finished() {
                if Instant::now() >= deadline {
                    return Err(PluginHostError::Worker(
                        "plugin is still running a call; try again once it returns".into(),
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| {
                PluginHostError::Worker("plugin tick worker panicked; DLL kept loaded".into())
            })?;
        }
        let callback = self.library.descriptor.shutdown.ok_or_else(|| {
            PluginHostError::Invalid("plugin descriptor has no shutdown callback".into())
        })?;
        // SAFETY: The tick worker has joined and the plugin still owns its handle.
        let status = {
            let _guard = PluginCallGuard::enter(&self.identity);
            unsafe { callback(self.handle as *mut _) }
        };
        if status != PluginStatus::Ok {
            return Err(PluginHostError::Execution(format!(
                "plugin shutdown failed ({status:?}); DLL kept loaded"
            )));
        }
        self.stopped = true;
        Ok(())
    }
}

fn dispatch_event(runtime: &HostRuntime, identity: &Arc<PluginIdentity>, event: WorkerEvent) {
    match event {
        WorkerEvent::LyricsTransform {
            token,
            request,
            response,
        } => {
            let _ = response.send(lyrics::dispatch_transform(
                runtime, identity, token, &request,
            ));
        }
        WorkerEvent::MediaCommand {
            id,
            command,
            position_ms,
            completion,
        } => {
            let status = dispatch_media_command(runtime, identity, id, command, position_ms);
            if let Some((token, sequence)) = completion {
                runtime.extensions.complete(token, sequence, status);
            }
        }
        WorkerEvent::HostState { id, snapshot } => {
            dispatch_host_state(runtime, identity, id, *snapshot)
        }
        WorkerEvent::SettingsChange {
            id,
            key,
            value,
            response,
        } => {
            let _ = response.send(dispatch_settings_change(
                runtime, identity, id, &key, &value,
            ));
        }
    }
}

fn dispatch_settings_change(
    runtime: &HostRuntime,
    identity: &Arc<PluginIdentity>,
    id: u64,
    key: &str,
    value: &str,
) -> PluginStatus {
    if key.is_empty() || key.len() >= 64 || value.len() >= 256 {
        return PluginStatus::InvalidArgument;
    }
    let callback = {
        let Ok(mut state) = runtime.state.lock() else {
            return PluginStatus::Internal;
        };
        let Some(page) = state.settings.get_mut(&id) else {
            return PluginStatus::StaleHandle;
        };
        if !page
            .items
            .iter()
            .any(|item| item.raw.key.split(|byte| *byte == 0).next() == Some(key.as_bytes()))
        {
            return PluginStatus::InvalidArgument;
        }
        let Some(callback) = page.on_change else {
            return PluginStatus::InvalidArgument;
        };
        page.in_flight = page.in_flight.saturating_add(1);
        (callback, page.callback_data)
    };
    let mut change = SettingsChangeV2 {
        struct_size: std::mem::size_of::<SettingsChangeV2>() as u32,
        key: [0; 64],
        value: [0; 256],
    };
    change.key[..key.len()].copy_from_slice(key.as_bytes());
    change.value[..value.len()].copy_from_slice(value.as_bytes());
    // SAFETY: The settings page remains registered and leased for the callback.
    let status = {
        let _guard = PluginCallGuard::enter(identity);
        unsafe { (callback.0)(callback.1 as *mut _, ResourceId::from_raw(id), &change) }
    };
    if let Ok(mut state) = runtime.state.lock()
        && let Some(page) = state.settings.get_mut(&id)
    {
        page.in_flight = page.in_flight.saturating_sub(1);
    }
    status
}

fn dispatch_host_state(
    runtime: &HostRuntime,
    identity: &Arc<PluginIdentity>,
    id: u64,
    snapshot: HostStateV2,
) {
    let callback = {
        let Ok(mut state) = runtime.state.lock() else {
            return;
        };
        let Some(record) = state.subscriptions.get_mut(&id) else {
            return;
        };
        record.in_flight = record.in_flight.saturating_add(1);
        (record.callback, record.callback_data)
    };
    // SAFETY: The registered callback is leased until this worker decrements `in_flight`.
    let status = {
        let _guard = PluginCallGuard::enter(identity);
        unsafe { (callback.0)(callback.1 as *mut _, &snapshot) }
    };
    if status != PluginStatus::Ok {
        log::warn!(
            "Plugin '{}' host-state callback returned {status:?}",
            identity.id
        );
    }
    if let Ok(mut state) = runtime.state.lock()
        && let Some(record) = state.subscriptions.get_mut(&id)
    {
        record.in_flight = record.in_flight.saturating_sub(1);
    }
}

fn dispatch_media_command(
    runtime: &HostRuntime,
    identity: &Arc<PluginIdentity>,
    id: u64,
    command: u32,
    position_ms: u64,
) -> PluginStatus {
    let required_control = match command {
        MEDIA_COMMAND_TOGGLE_PLAY => MEDIA_CONTROL_TOGGLE_PLAY,
        MEDIA_COMMAND_PREVIOUS => MEDIA_CONTROL_PREVIOUS,
        MEDIA_COMMAND_NEXT => MEDIA_CONTROL_NEXT,
        MEDIA_COMMAND_SEEK => MEDIA_CONTROL_SEEK,
        _ => return PluginStatus::InvalidArgument,
    };
    let callback = {
        let Ok(mut state) = runtime.state.lock() else {
            return PluginStatus::Internal;
        };
        let Some(media) = state.media.get_mut(&id) else {
            return PluginStatus::StaleHandle;
        };
        if media.available_controls & required_control == 0 {
            return PluginStatus::InvalidArgument;
        }
        let Some(callback) = media.on_command else {
            return PluginStatus::InvalidArgument;
        };
        media.in_flight = media.in_flight.saturating_add(1);
        (callback, media.callback_data)
    };
    let event = MediaCommandV2 {
        struct_size: std::mem::size_of::<MediaCommandV2>() as u32,
        command,
        position_ms,
    };
    // SAFETY: The registered callback is leased until this worker releases `in_flight`.
    let _guard = PluginCallGuard::enter(identity);
    unsafe { (callback.0)(callback.1 as *mut _, ResourceId::from_raw(id), &event) };
    if let Ok(mut state) = runtime.state.lock()
        && let Some(media) = state.media.get_mut(&id)
    {
        media.in_flight = media.in_flight.saturating_sub(1);
    }
    PluginStatus::Ok
}

impl Drop for PluginInstance {
    fn drop(&mut self) {
        if self.shutdown().is_err() {
            return;
        }
        if let Some(destroy) = self.library.descriptor.destroy {
            // SAFETY: Shutdown joined all host workers and the plugin reports its own threads joined.
            let _guard = PluginCallGuard::enter(&self.identity);
            unsafe { destroy(self.handle as *mut _) };
        }
        // SAFETY: No plugin callbacks remain after shutdown and destroy.
        unsafe { ManuallyDrop::drop(&mut self.library) };
    }
}
