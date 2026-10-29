use std::time::{Duration, Instant};

use pollkit::Every;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use winisland_platform::{MediaCommand, MediaContext};

use winisland_core::lyrics::LyricsMode;

use super::properties::{ThumbnailFetcher, TimelineCache, fetch_properties};
use super::session::{auto_allow_new_apps, get_target_session};
use super::{
    LyricsFetchConfig, LyricsFetchRequest, MediaInfo, PlaybackCommand, spawn_lyrics_fetch,
};

pub(super) struct WorkerChannels {
    pub(super) session_bridge: super::sessions::SessionBridge,
    pub(super) info_tx: watch::Sender<MediaInfo>,
    pub(super) enabled_rx: watch::Receiver<bool>,
    pub(super) seek_rx: mpsc::UnboundedReceiver<u64>,
    pub(super) playback_rx: mpsc::UnboundedReceiver<PlaybackCommand>,
    pub(super) lyrics_mode_rx: mpsc::UnboundedReceiver<LyricsMode>,
    pub(super) lyrics_source_rx: mpsc::UnboundedReceiver<String>,
    pub(super) lyrics_local_dir_rx: mpsc::UnboundedReceiver<Option<String>>,
    pub(super) show_secondary_lyrics_rx: mpsc::UnboundedReceiver<bool>,
    pub(super) allowed_apps_rx: mpsc::UnboundedReceiver<Vec<String>>,
    pub(super) wake_rx: std::sync::mpsc::Receiver<()>,
    pub(super) known_apps: Vec<String>,
    pub(super) lyrics_bridge: Option<winisland_plugin_host::lifecycle::LyricsBridge>,
}

pub(super) fn smtc_poll_loop(channels: WorkerChannels, cancel: CancellationToken) {
    let WorkerChannels {
        session_bridge,
        info_tx,
        mut enabled_rx,
        mut seek_rx,
        mut playback_rx,
        mut lyrics_mode_rx,
        mut lyrics_source_rx,
        mut lyrics_local_dir_rx,
        mut show_secondary_lyrics_rx,
        mut allowed_apps_rx,
        wake_rx,
        known_apps,
        lyrics_bridge,
    } = channels;
    let manager = match crate::platform::media().open_context() {
        Ok(manager) => manager,
        Err(error) => {
            crate::platform::update_capabilities(|caps| caps.media_session = false);
            log::error!("SMTC: failed to get session manager: {error}");
            return;
        }
    };
    log::info!("SMTC: session manager created");
    session_bridge.set_ready();
    let mut enabled = *enabled_rx.borrow_and_update();

    let mut current_lyrics_mode = LyricsMode::Online;
    let mut current_lyrics_source = "163".to_string();
    let mut current_lyrics_local_dir: Option<String> = None;
    let mut current_show_secondary_lyrics = false;
    let mut current_allowed_apps: Vec<String> = Vec::new();

    while let Ok(mode) = lyrics_mode_rx.try_recv() {
        current_lyrics_mode = mode;
    }
    while let Ok(src) = lyrics_source_rx.try_recv() {
        current_lyrics_source = src;
    }
    while let Ok(dir) = lyrics_local_dir_rx.try_recv() {
        current_lyrics_local_dir = dir;
    }
    while let Ok(show) = show_secondary_lyrics_rx.try_recv() {
        current_show_secondary_lyrics = show;
    }
    while let Ok(apps) = allowed_apps_rx.try_recv() {
        current_allowed_apps = apps;
    }

    let mut session_worker = super::sessions::SessionWorker::new(session_bridge);
    let mut media_state = MediaUpdateState {
        preferred: None,
        allowed_apps: current_allowed_apps,
        known_apps,
        last_session_seen: Instant::now(),
        last_was_playing: false,
        thumbnail_fetcher: enabled
            .then(|| ThumbnailFetcher::new(info_tx.clone()))
            .flatten(),
        timeline_cache: TimelineCache::default(),
    };

    if enabled {
        for attempt in 0..10 {
            media_state.update(
                manager.as_ref(),
                &info_tx,
                LyricsFetchConfig {
                    mode: current_lyrics_mode,
                    source: &current_lyrics_source,
                    local_dir: current_lyrics_local_dir.as_deref(),
                    show_secondary: current_show_secondary_lyrics,
                    bridge: lyrics_bridge.as_ref(),
                },
                true,
            );
            let info = info_tx.borrow();
            let timeline_ready = info.duration_ms > 0
                || info.position_ms > 0
                || !info.is_playing
                || info.title.is_empty();
            if timeline_ready {
                if attempt > 0 {
                    log::info!("SMTC: initial timeline ready after {} retries", attempt + 1);
                }
                drop(info);
                break;
            }
            drop(info);
            if attempt < 9 {
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }

    let mut regular_update = Every::millis(300);
    let mut regular_poll_count = 0u32;

    while !cancel.is_cancelled() {
        let mut lyrics_settings_changed = false;
        while let Ok(mode) = lyrics_mode_rx.try_recv() {
            if mode != current_lyrics_mode {
                current_lyrics_mode = mode;
                lyrics_settings_changed = true;
            }
        }
        while let Ok(src) = lyrics_source_rx.try_recv() {
            if src != current_lyrics_source {
                current_lyrics_source = src;
                lyrics_settings_changed = true;
            }
        }
        while let Ok(dir) = lyrics_local_dir_rx.try_recv() {
            if dir != current_lyrics_local_dir {
                current_lyrics_local_dir = dir;
                lyrics_settings_changed = true;
            }
        }
        while let Ok(show) = show_secondary_lyrics_rx.try_recv() {
            if show != current_show_secondary_lyrics {
                current_show_secondary_lyrics = show;
                // Without a secondary-text plugin the setting only affects rendering, so the
                // current lyrics stay.
                if lyrics_bridge
                    .as_ref()
                    .is_some_and(|bridge| bridge.has_secondary())
                {
                    lyrics_settings_changed = true;
                }
            }
        }
        if lyrics_settings_changed {
            refresh_current_lyrics(
                &info_tx,
                current_lyrics_mode,
                &current_lyrics_source,
                current_lyrics_local_dir.as_deref(),
                current_show_secondary_lyrics,
                lyrics_bridge.as_ref(),
            );
        }
        while let Ok(apps) = allowed_apps_rx.try_recv() {
            media_state.allowed_apps = apps;
        }

        let next_enabled = *enabled_rx.borrow_and_update();
        if next_enabled != enabled {
            enabled = next_enabled;
            if enabled {
                media_state.thumbnail_fetcher = ThumbnailFetcher::new(info_tx.clone());
                regular_update.expire();
                media_state.last_session_seen = Instant::now();
                media_state.timeline_cache.clear();
                log::info!("SMTC: listener enabled");
            } else {
                media_state.thumbnail_fetcher = None;
                let _ = info_tx.send(MediaInfo::default());
                media_state.last_was_playing = false;
                media_state.timeline_cache.clear();
                log::info!("SMTC: listener disabled");
            }
        }

        let preferred =
            session_worker.refresh(manager.as_ref(), &media_state.allowed_apps, enabled);
        if preferred.as_ref().map(|session| session.identity())
            != media_state
                .preferred
                .as_ref()
                .map(|session| session.identity())
        {
            regular_update.expire();
            media_state.timeline_cache.clear();
        }
        media_state.preferred = preferred;

        if !enabled {
            while seek_rx.try_recv().is_ok() {}
            while playback_rx.try_recv().is_ok() {}
            while manager.poll_events() {}
            let _ = wake_rx.recv_timeout(Duration::from_millis(300));
            continue;
        }

        let mut seek_pos = None;
        while let Ok(v) = seek_rx.try_recv() {
            seek_pos = Some(v);
        }
        if let Some(seek_pos) = seek_pos {
            let seek_pos = seek_pos.min(i64::MAX as u64 / 10_000);
            if !apply_seek_request(
                manager.as_ref(),
                &media_state.allowed_apps,
                &info_tx,
                media_state.preferred.clone(),
                seek_pos,
            ) {
                info_tx.send_if_modified(|info| info.reject_seek(seek_pos));
            }
        }

        while let Ok(cmd) = playback_rx.try_recv() {
            log::info!("SMTC: playback command {cmd:?}");
            if let Some(session) = media_state
                .preferred
                .clone()
                .or_else(|| get_target_session(manager.as_ref(), &media_state.allowed_apps))
            {
                let command = match cmd {
                    PlaybackCommand::Toggle => MediaCommand::Toggle,
                    PlaybackCommand::Next => MediaCommand::Next,
                    PlaybackCommand::Prev => MediaCommand::Previous,
                };
                let _ = session.send(command);
            }
        }

        if manager.poll_events() {
            log::debug!("SMTC: session change event received, updating immediately");
            media_state.update(
                manager.as_ref(),
                &info_tx,
                LyricsFetchConfig {
                    mode: current_lyrics_mode,
                    source: &current_lyrics_source,
                    local_dir: current_lyrics_local_dir.as_deref(),
                    show_secondary: current_show_secondary_lyrics,
                    bridge: lyrics_bridge.as_ref(),
                },
                true,
            );
            regular_update.restart(Instant::now());
        }

        if regular_update.due_now() {
            regular_poll_count += 1;
            let do_auto_allow = regular_poll_count.is_multiple_of(10);
            media_state.update(
                manager.as_ref(),
                &info_tx,
                LyricsFetchConfig {
                    mode: current_lyrics_mode,
                    source: &current_lyrics_source,
                    local_dir: current_lyrics_local_dir.as_deref(),
                    show_secondary: current_show_secondary_lyrics,
                    bridge: lyrics_bridge.as_ref(),
                },
                do_auto_allow,
            );
        }

        let _ = wake_rx.recv_timeout(Duration::from_millis(300));
    }
}

fn apply_seek_request(
    manager: &dyn MediaContext,
    allowed_apps: &[String],
    info_tx: &watch::Sender<MediaInfo>,
    preferred: Option<std::sync::Arc<dyn winisland_platform::MediaSessionHandle>>,
    position_ms: u64,
) -> bool {
    let Some(session) = preferred.or_else(|| get_target_session(manager, allowed_apps)) else {
        log::debug!("SMTC: ignored seek without an active session");
        return false;
    };
    let source_app_id = session.source_app_id().unwrap_or_default();
    log::info!("SMTC: seek to {position_ms}ms");
    match session.send(MediaCommand::Seek(Duration::from_millis(position_ms))) {
        Ok(true) => {}
        Ok(false) => {
            log::warn!("SMTC: media session rejected seek to {position_ms}ms");
            return false;
        }
        Err(error) => {
            log::warn!("SMTC: seek failed: {error}");
            return false;
        }
    }
    info_tx.send_if_modified(|info| {
        if info.source_app_id != source_app_id
            || info.title.is_empty()
            || (info.seek_guard_until.is_some() && info.seek_target_ms != position_ms)
        {
            return false;
        }
        info.apply_seek(position_ms);
        true
    })
}

struct MediaUpdateState {
    preferred: Option<std::sync::Arc<dyn winisland_platform::MediaSessionHandle>>,
    allowed_apps: Vec<String>,
    known_apps: Vec<String>,
    last_session_seen: Instant,
    last_was_playing: bool,
    thumbnail_fetcher: Option<ThumbnailFetcher>,
    timeline_cache: TimelineCache,
}

impl MediaUpdateState {
    fn update(
        &mut self,
        manager: &dyn MediaContext,
        info_tx: &watch::Sender<MediaInfo>,
        lyrics: LyricsFetchConfig<'_>,
        auto_allow: bool,
    ) {
        if auto_allow {
            self.allowed_apps =
                auto_allow_new_apps(manager, &self.allowed_apps, &mut self.known_apps);
        }

        if let Some(session) = self
            .preferred
            .clone()
            .or_else(|| get_target_session(manager, &self.allowed_apps))
        {
            match fetch_properties(
                &session,
                info_tx,
                lyrics,
                self.thumbnail_fetcher.as_ref(),
                &mut self.timeline_cache,
            ) {
                Ok(()) => {
                    self.last_session_seen = Instant::now();
                    self.last_was_playing = info_tx.borrow().is_playing;
                }
                Err(error) => {
                    log::debug!("SMTC: failed to read active session: {error}");
                    if self.last_session_seen.elapsed() > Duration::from_secs(5) {
                        let info = info_tx.borrow();
                        if !info.title.is_empty() {
                            drop(info);
                            let _ = info_tx.send(MediaInfo::default());
                            self.timeline_cache.clear();
                            self.last_was_playing = false;
                            log::warn!("SMTC: active session failed for >5s, cleared media info");
                        }
                    }
                }
            }
        } else if self.last_was_playing {
            let info = info_tx.borrow();
            if !info.title.is_empty() {
                drop(info);
                let _ = info_tx.send(MediaInfo::default());
                self.timeline_cache.clear();
                log::info!("SMTC: app closed while playing, cleared immediately");
            }
            self.last_was_playing = false;
        } else if self.last_session_seen.elapsed() > Duration::from_secs(15) {
            let info = info_tx.borrow();
            if !info.title.is_empty() {
                drop(info);
                let _ = info_tx.send(MediaInfo::default());
                self.timeline_cache.clear();
                log::info!("SMTC: paused session lost for >15s, cleared media info");
            }
        }
    }
}

fn refresh_current_lyrics(
    info_tx: &watch::Sender<MediaInfo>,
    lyrics_mode: LyricsMode,
    lyrics_source: &str,
    local_dir: Option<&str>,
    show_secondary: bool,
    lyrics_bridge: Option<&winisland_plugin_host::lifecycle::LyricsBridge>,
) {
    let mut request = None;
    info_tx.send_if_modified(|info| {
        if info.title.is_empty() {
            return false;
        }
        info.lyrics = None;
        info.lyrics_fetch_id = info.lyrics_fetch_id.wrapping_add(1);
        request = Some((
            info.title.clone(),
            info.artist.clone(),
            info.duration_secs,
            info.lyrics_fetch_id,
            info.track_id,
        ));
        true
    });
    let Some((title, artist, duration_secs, request_id, track_id)) = request else {
        return;
    };
    spawn_lyrics_fetch(
        info_tx,
        LyricsFetchRequest {
            title,
            artist,
            duration_secs,
            mode: lyrics_mode,
            source: lyrics_source.to_string(),
            local_dir: local_dir.map(str::to_string),
            show_secondary,
            request_id,
            track_id,
        },
        lyrics_bridge.cloned(),
    );
}
