use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winisland_core::lyrics::LyricLine;
use winisland_plugin_api::abi::PluginStatus;
use winisland_plugin_api::types::v2::lyrics::{LYRICS_TEXT_FLAG_WORD_SYNCED, LyricsTextV2};
use winisland_plugin_api::types::v2::{PluginToken, ResourceId, Utf8Slice};

use super::{PluginCallGuard, PluginIdentity, PluginInstance, WorkerEvent};
use crate::resources::ResourceKind;
use crate::runtime::{HostRuntime, LyricsKinds};

const PRIMARY_TIMEOUT: Duration = Duration::from_secs(2);
/// Secondary transformers run after the original lyrics are shown, and one may process the whole
/// song on its first line, so it gets far longer than a primary transformer.
const SECONDARY_TIMEOUT: Duration = Duration::from_secs(180);
const CANCEL_POLL: Duration = Duration::from_millis(100);
const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct LyricsBridge {
    workers: Arc<Mutex<Vec<LyricsWorker>>>,
    kinds: LyricsKinds,
}

#[derive(Clone)]
struct LyricsWorker {
    id: String,
    token: PluginToken,
    sender: Sender<WorkerEvent>,
    thread: std::thread::Thread,
    load: Arc<WorkerLoad>,
}

/// Tracked apart from any waiting request: a waiter gives up when its song changes, so only the
/// age of the plugin's running call shows that the plugin has hung.
#[derive(Default)]
struct WorkerLoad {
    pending: AtomicUsize,
    running_since: Mutex<Option<Instant>>,
}

pub(super) struct TransformRequest {
    pub(super) secondary: bool,
    pub(super) time_ms: u64,
    pub(super) word_synced: bool,
    pub(super) text: String,
    pub(super) index: u32,
    pub(super) count: u32,
    pub(super) song: Arc<str>,
    pub(super) cancelled: Arc<AtomicBool>,
    pending: PendingRequest,
}

impl TransformRequest {
    pub(super) fn start_call(&self) -> RunningCall {
        if let Ok(mut running_since) = self.pending.load.running_since.lock() {
            *running_since = Some(Instant::now());
        }
        RunningCall(self.pending.load.clone())
    }
}

/// Dropping a request, whether handled or discarded by a stopping worker, ends its pending state.
struct PendingRequest {
    load: Arc<WorkerLoad>,
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.load.pending.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct RunningCall(Arc<WorkerLoad>);

impl Drop for RunningCall {
    fn drop(&mut self) {
        if let Ok(mut running_since) = self.0.running_since.lock() {
            *running_since = None;
        }
    }
}

impl LyricsWorker {
    fn pending_request(&self) -> PendingRequest {
        self.load.pending.fetch_add(1, Ordering::AcqRel);
        PendingRequest {
            load: self.load.clone(),
        }
    }

    fn busy(&self) -> bool {
        self.load.pending.load(Ordering::Acquire) > 0
    }

    fn stalled(&self) -> bool {
        self.load
            .running_since
            .lock()
            .ok()
            .and_then(|running_since| *running_since)
            .is_some_and(|start| start.elapsed() >= SECONDARY_TIMEOUT)
    }

    fn transform(
        &self,
        request: TransformRequest,
        timeout: Duration,
    ) -> Result<String, PluginStatus> {
        let cancelled = request.cancelled.clone();
        let (response, receive) = mpsc::channel();
        self.sender
            .send(WorkerEvent::LyricsTransform {
                token: self.token,
                request,
                response,
            })
            .map_err(|_| PluginStatus::Internal)?;
        self.thread.unpark();
        let deadline = Instant::now() + timeout;
        loop {
            // Waiting in slices lets a cancelled annotation give up its thread without waiting for
            // the plugin to return.
            let slice = CANCEL_POLL.min(deadline.saturating_duration_since(Instant::now()));
            match receive.recv_timeout(slice) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Disconnected) => return Err(PluginStatus::Internal),
                Err(RecvTimeoutError::Timeout) if cancelled.load(Ordering::Relaxed) => {
                    return Err(PluginStatus::StaleHandle);
                }
                Err(RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                    return Err(PluginStatus::Internal);
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

impl LyricsBridge {
    pub(crate) fn new(kinds: LyricsKinds) -> Self {
        Self {
            workers: Arc::default(),
            kinds,
        }
    }

    pub(crate) fn attach(&self, instance: &PluginInstance) {
        if let Some(thread) = &instance.worker
            && let Some(sender) = &instance.event_tx
            && let Ok(mut workers) = self.workers.lock()
        {
            workers.push(LyricsWorker {
                id: instance.library().metadata().id.clone(),
                token: instance.token(),
                sender: sender.clone(),
                thread: thread.thread().clone(),
                load: Arc::default(),
            });
        }
    }

    pub(crate) fn detach(&self, plugin_id: &str) {
        if let Ok(mut workers) = self.workers.lock() {
            workers.retain(|worker| worker.id != plugin_id);
        }
    }

    /// A plugin's calls run one at a time on its worker, so a request it has no transformer for
    /// would wait behind its other work and time out. Primary runs also skip a busy plugin so
    /// they never hold back the lyrics; secondary runs queue behind it, because the earlier call
    /// is usually the previous song's, whose remaining requests the plugin skips.
    fn active_workers(&self, secondary: bool) -> Vec<(LyricsWorker, bool)> {
        let Ok(kinds) = self.kinds.lock() else {
            return Vec::new();
        };
        self.workers
            .lock()
            .map(|workers| {
                workers
                    .iter()
                    .filter(|worker| {
                        kinds
                            .values()
                            .any(|&(token, kind)| token == worker.token && kind == secondary)
                            && !worker.stalled()
                            && (secondary || !worker.busy())
                    })
                    .cloned()
                    .map(|worker| (worker, true))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether a plugin that is not stalled has a transformer that writes secondary text.
    pub fn has_secondary(&self) -> bool {
        !self.active_workers(true).is_empty()
    }

    pub fn apply(&self, lyrics: Arc<Vec<LyricLine>>) -> Arc<Vec<LyricLine>> {
        self.run(lyrics, false, &Arc::default(), |_| {})
    }

    /// `progress` receives partial results at most once per [`PROGRESS_INTERVAL`]. Once
    /// `cancelled` is set the run stops and its result is stale.
    pub fn annotate(
        &self,
        lyrics: Arc<Vec<LyricLine>>,
        cancelled: &Arc<AtomicBool>,
        progress: impl FnMut(&Arc<Vec<LyricLine>>),
    ) -> Arc<Vec<LyricLine>> {
        self.run(lyrics, true, cancelled, progress)
    }

    fn run(
        &self,
        mut lyrics: Arc<Vec<LyricLine>>,
        secondary: bool,
        cancelled: &Arc<AtomicBool>,
        mut progress: impl FnMut(&Arc<Vec<LyricLine>>),
    ) -> Arc<Vec<LyricLine>> {
        let mut workers = self.active_workers(secondary);
        if workers.is_empty() {
            return lyrics;
        }
        let Ok(count) = u32::try_from(lyrics.len()) else {
            return lyrics;
        };
        let song: Arc<str> = lyrics
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .into();
        let timeout = if secondary {
            SECONDARY_TIMEOUT
        } else {
            PRIMARY_TIMEOUT
        };
        let mut reported = HashSet::new();
        let mut published = Instant::now();
        let mut unpublished = false;
        for (index, position) in (0..count).zip(0..lyrics.len()) {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let line = &mut Arc::make_mut(&mut lyrics)[position];
            for (worker, active) in &mut workers {
                if !*active {
                    continue;
                }
                let request = TransformRequest {
                    secondary,
                    time_ms: line.time_ms,
                    word_synced: line.is_word_synced(),
                    text: line.text.clone(),
                    index,
                    count,
                    song: song.clone(),
                    cancelled: cancelled.clone(),
                    pending: worker.pending_request(),
                };
                match worker.transform(request, timeout) {
                    Ok(text) if secondary => {
                        if !text.trim().is_empty() && text != line.text {
                            line.secondary_text = Some(text);
                            unpublished = true;
                            break;
                        }
                    }
                    Ok(text) => {
                        if !line.replace_text_preserving_timings(text)
                            && reported.insert(worker.id.clone())
                        {
                            log::warn!(
                                "Lyrics transformer '{}' changed word timing boundaries",
                                worker.id
                            );
                        }
                    }
                    Err(PluginStatus::StaleHandle) if cancelled.load(Ordering::Relaxed) => break,
                    Err(status) => {
                        *active = false;
                        if reported.insert(worker.id.clone()) {
                            if worker.stalled() {
                                log::warn!(
                                    "Lyrics transformer '{}' has run one call for over \
                                     {SECONDARY_TIMEOUT:?}; skipping it until that call returns",
                                    worker.id
                                );
                            } else {
                                log::warn!("Lyrics transformer '{}' failed: {status:?}", worker.id);
                            }
                        }
                    }
                }
            }
            if unpublished && published.elapsed() >= PROGRESS_INTERVAL {
                progress(&lyrics);
                published = Instant::now();
                unpublished = false;
            }
        }
        lyrics
    }
}

pub(super) fn dispatch_transform(
    runtime: &HostRuntime,
    identity: &Arc<PluginIdentity>,
    token: PluginToken,
    request: &TransformRequest,
) -> Result<String, PluginStatus> {
    if request.cancelled.load(Ordering::Relaxed) {
        return Err(PluginStatus::StaleHandle);
    }
    let _call = request.start_call();
    let mut ids = runtime.resources.list(token, ResourceKind::Lyrics)?;
    ids.sort_unstable();
    let mut text = request.text.clone();
    for id in ids {
        let (callback, callback_data) = {
            let mut state = runtime.state.lock().map_err(|_| PluginStatus::Internal)?;
            let record = state.lyrics.get_mut(&id).ok_or(PluginStatus::StaleHandle)?;
            if record.secondary != request.secondary {
                continue;
            }
            record.in_flight = record.in_flight.saturating_add(1);
            (record.on_transform, record.callback_data)
        };
        let input = if request.secondary {
            &request.text
        } else {
            &text
        };
        let result = transform_one(identity, callback, callback_data, id, request, input);
        if let Ok(mut state) = runtime.state.lock()
            && let Some(record) = state.lyrics.get_mut(&id)
        {
            record.in_flight = record.in_flight.saturating_sub(1);
        }
        let output = result?;
        if !request.secondary {
            text = output;
        } else if !output.trim().is_empty() {
            return Ok(output);
        }
    }
    if request.secondary {
        Ok(String::new())
    } else {
        Ok(text)
    }
}

fn transform_one(
    identity: &Arc<PluginIdentity>,
    callback: winisland_plugin_api::types::v2::lyrics::LyricsTransformFnV2,
    callback_data: usize,
    id: u64,
    request: &TransformRequest,
    text: &str,
) -> Result<String, PluginStatus> {
    let input = LyricsTextV2 {
        struct_size: std::mem::size_of::<LyricsTextV2>() as u32,
        flags: if request.word_synced {
            LYRICS_TEXT_FLAG_WORD_SYNCED
        } else {
            0
        },
        line_time_ms: request.time_ms,
        text: Utf8Slice::borrowed(text),
        line_index: request.index,
        line_count: request.count,
        song: Utf8Slice::borrowed(&request.song),
    };
    let id = unsafe { ResourceId::from_raw(id) };
    let mut required = 0u32;
    let _guard = PluginCallGuard::enter(identity);
    // SAFETY: The callback is leased and the input remains live for the size query.
    let status = unsafe {
        callback(
            callback_data as *mut _,
            id,
            &input,
            std::ptr::null_mut(),
            0,
            &mut required,
        )
    };
    if status != PluginStatus::Ok {
        return Err(status);
    }
    if required > 256 * 1024 {
        return Err(PluginStatus::LimitExceeded);
    }
    if required == 0 {
        return Ok(String::new());
    }
    let mut output = vec![0u8; required as usize];
    let mut written = required;
    // SAFETY: The callback is leased and the output buffer has the requested capacity.
    let status = unsafe {
        callback(
            callback_data as *mut _,
            id,
            &input,
            output.as_mut_ptr(),
            required,
            &mut written,
        )
    };
    if status != PluginStatus::Ok {
        return Err(status);
    }
    if written > required {
        return Err(PluginStatus::InvalidArgument);
    }
    output.truncate(written as usize);
    String::from_utf8(output).map_err(|_| PluginStatus::InvalidArgument)
}
