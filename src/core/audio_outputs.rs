use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use winisland_platform::AudioOutputDevice;

#[derive(Default)]
struct OutputState {
    devices: Vec<AudioOutputDevice>,
    revision: u64,
}

/// Playback devices listed and switched on worker threads, read from the UI thread.
#[derive(Default)]
pub struct AudioOutputs {
    state: Arc<Mutex<OutputState>>,
    busy: Arc<AtomicBool>,
}

impl AudioOutputs {
    pub fn refresh(&self) {
        self.spawn(None);
    }

    pub fn select(&self, id: &str) {
        let mut state = self.state.lock();
        for device in &mut state.devices {
            device.is_default = device.id == id;
        }
        state.revision += 1;
        drop(state);
        self.spawn(Some(id.to_string()));
    }

    pub fn revision(&self) -> u64 {
        self.state.lock().revision
    }

    pub fn devices(&self) -> Vec<AudioOutputDevice> {
        self.state.lock().devices.clone()
    }

    fn spawn(&self, select: Option<String>) {
        if self.busy.swap(true, Ordering::AcqRel) && select.is_none() {
            return;
        }
        let state = self.state.clone();
        let busy = self.busy.clone();
        let spawned = std::thread::Builder::new()
            .name("audio-outputs".to_string())
            .spawn(move || {
                if let Some(id) = select
                    && let Err(error) = crate::platform::audio().set_default_output(&id)
                {
                    log::warn!("Audio outputs: could not switch to {id}: {error}");
                }
                match crate::platform::audio().output_devices() {
                    Ok(devices) => {
                        let mut state = state.lock();
                        if state.devices != devices {
                            state.devices = devices;
                            state.revision += 1;
                        }
                    }
                    Err(error) => log::warn!("Audio outputs: could not list devices: {error}"),
                }
                busy.store(false, Ordering::Release);
            });
        if let Err(error) = spawned {
            self.busy.store(false, Ordering::Release);
            log::warn!("Audio outputs: worker could not start: {error}");
        }
    }
}
