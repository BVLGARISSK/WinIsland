use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use winisland_core::lyrics::LyricLine;
use winisland_plugin_api::abi::PluginStatus;
use winisland_plugin_api::types::v2::lyrics::{LYRICS_TEXT_FLAG_WORD_SYNCED, LyricsTextV2};
use winisland_plugin_api::types::v2::{PluginToken, ResourceId, Utf8Slice};

use super::{PluginCallGuard, PluginIdentity, PluginInstance, WorkerEvent};
use crate::resources::ResourceKind;
use crate::runtime::HostRuntime;

const PRIMARY_TIMEOUT: Duration = Duration::from_secs(2);
/// Secondary transformers run after the original lyrics are shown, and one may process the whole
/// song on its first line, so it gets far longer than a primary transformer.
const SECONDARY_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Default)]
pub struct LyricsBridge {
    workers: Arc<Mutex<Vec<LyricsWorker>>>,
}

#[derive(Clone)]
struct LyricsWorker {
    id: String,
    token: PluginToken,
    sender: Sender<WorkerEvent>,
    thread: std::thread::Thread,
}

pub(super) struct TransformRequest {
    pub(super) secondary: bool,
    pub(super) time_ms: u64,
    pub(super) word_synced: bool,
    pub(super) text: String,
    pub(super) index: u32,
    pub(super) count: u32,
    pub(super) song: Arc<str>,
}

impl LyricsWorker {
    fn transform(
        &self,
        request: TransformRequest,
        timeout: Duration,
    ) -> Result<String, PluginStatus> {
        let (response, receive) = mpsc::channel();
        self.sender
            .send(WorkerEvent::LyricsTransform {
                token: self.token,
                request,
                response,
            })
            .map_err(|_| PluginStatus::Internal)?;
        self.thread.unpark();
        receive
            .recv_timeout(timeout)
            .map_err(|_| PluginStatus::Internal)?
    }
}

impl LyricsBridge {
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
            });
        }
    }

    pub(crate) fn detach(&self, plugin_id: &str) {
        if let Ok(mut workers) = self.workers.lock() {
            workers.retain(|worker| worker.id != plugin_id);
        }
    }

    fn active_workers(&self) -> Vec<(LyricsWorker, bool)> {
        self.workers
            .lock()
            .map(|workers| {
                workers
                    .iter()
                    .cloned()
                    .map(|worker| (worker, true))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Runs the transformers that replace line text.
    pub fn apply(&self, lyrics: Arc<Vec<LyricLine>>) -> Arc<Vec<LyricLine>> {
        self.run(lyrics, false, &AtomicBool::new(false))
    }

    /// Runs the transformers that write each line's secondary text, such as a translation.
    ///
    /// Stops early once `cancelled` is set; the partly annotated lyrics are then stale and should
    /// be discarded by the caller.
    pub fn annotate(
        &self,
        lyrics: Arc<Vec<LyricLine>>,
        cancelled: &AtomicBool,
    ) -> Arc<Vec<LyricLine>> {
        self.run(lyrics, true, cancelled)
    }

    fn run(
        &self,
        mut lyrics: Arc<Vec<LyricLine>>,
        secondary: bool,
        cancelled: &AtomicBool,
    ) -> Arc<Vec<LyricLine>> {
        let mut workers = self.active_workers();
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
        for (index, line) in (0..count).zip(Arc::make_mut(&mut lyrics).iter_mut()) {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
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
                };
                match worker.transform(request, timeout) {
                    Ok(text) if secondary => {
                        if !text.trim().is_empty() && text != line.text {
                            line.secondary_text = Some(text);
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
                    Err(status) => {
                        *active = false;
                        if reported.insert(worker.id.clone()) {
                            log::warn!("Lyrics transformer '{}' failed: {status:?}", worker.id);
                        }
                    }
                }
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
