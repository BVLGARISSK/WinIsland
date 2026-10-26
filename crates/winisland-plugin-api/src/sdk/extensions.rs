use std::ffi::c_void;
use std::sync::Mutex;

use crate::*;

use super::{Error, Host, Widget, success};

#[derive(Clone)]
pub struct Events(Host);
#[derive(Clone)]
pub struct Commands(Host);
#[derive(Clone)]
pub struct Input(Host);
#[derive(Clone)]
pub struct Surfaces(Host);
#[derive(Clone)]
pub struct MediaSessions(Host);
#[derive(Clone)]
pub struct Assistant(Host);

impl Host {
    pub fn supports(&self, interface: u32) -> bool {
        self.query::<TablePrefix>(interface).is_ok()
    }
    pub fn events(&self) -> Result<Events, Error> {
        self.query::<EventsApiV2>(IFACE_EVENTS)?;
        Ok(Events(self.clone()))
    }
    pub fn commands(&self) -> Result<Commands, Error> {
        self.query::<CommandApiV2>(IFACE_COMMAND)?;
        Ok(Commands(self.clone()))
    }
    pub fn input(&self) -> Result<Input, Error> {
        self.query::<InputApiV2>(IFACE_INPUT)?;
        Ok(Input(self.clone()))
    }
    pub fn surfaces(&self) -> Result<Surfaces, Error> {
        self.query::<SurfaceApiV2>(IFACE_SURFACE)?;
        Ok(Surfaces(self.clone()))
    }
    pub fn media_sessions(&self) -> Result<MediaSessions, Error> {
        self.query::<MediaSessionApiV2>(IFACE_MEDIA_SESSION)?;
        Ok(MediaSessions(self.clone()))
    }
    pub fn assistant(&self) -> Result<Assistant, Error> {
        self.query::<AssistantApiV2>(IFACE_ASSISTANT)?;
        Ok(Assistant(self.clone()))
    }
}

#[derive(Clone)]
pub struct Event {
    pub kind: u64,
    pub detail: u32,
    pub target: WidgetId,
    pub resource: ResourceId,
    pub sequence: u64,
    pub time_seconds: f64,
    pub position: (f32, f32),
    pub delta: (f32, f32),
    pub modifiers: u32,
    pub code: u32,
    pub data: Vec<u8>,
}

type Handler = Mutex<Box<dyn FnMut(Event) + Send>>;
type ReleaseFn = unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus;

unsafe extern "C" fn event_callback(data: *mut c_void, raw: *const PluginEventV2) {
    // SAFETY: CallbackResource retains this boxed handler until the host finishes callbacks.
    let handler = unsafe { &*data.cast::<Handler>() };
    // SAFETY: The host supplies a complete event and borrowed payload through this call.
    let raw = unsafe { &*raw };
    let data = if raw.data.len == 0 {
        Vec::new()
    } else {
        // SAFETY: Event payload bytes remain readable until this synchronous callback returns.
        unsafe { std::slice::from_raw_parts(raw.data.ptr, raw.data.len as usize) }.to_vec()
    };
    if let Ok(mut handler) = handler.lock() {
        handler(Event {
            kind: raw.kind,
            detail: raw.detail,
            target: raw.target,
            resource: raw.resource,
            sequence: raw.sequence,
            time_seconds: raw.time_seconds,
            position: (raw.x, raw.y),
            delta: (raw.delta_x, raw.delta_y),
            modifiers: raw.modifiers,
            code: raw.code,
            data,
        });
    }
}

pub struct CallbackResource {
    host: Host,
    id: ResourceId,
    context: usize,
    release: ReleaseFn,
    handler: Option<Box<Handler>>,
}

impl CallbackResource {
    pub fn id(&self) -> ResourceId {
        self.id
    }

    pub fn cancel(&mut self) -> Result<(), Error> {
        if self.id == ResourceId::INVALID {
            return Ok(());
        }
        // SAFETY: The host and callback storage remain live until release confirms completion.
        let status = unsafe { (self.release)(self.context as *mut _, self.host.token, self.id) };
        if status != PluginStatus::StaleHandle {
            success(status)?;
        }
        self.id = ResourceId::INVALID;
        self.handler.take();
        Ok(())
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), Error> {
        let table = self.host.query::<CommandApiV2>(IFACE_COMMAND)?;
        let function = table
            .set_enabled
            .ok_or(Error::MissingFunction("command.set_enabled"))?;
        // SAFETY: The host validates that this resource is a command owned by the caller.
        success(unsafe {
            function(
                table.prefix.context,
                self.host.token,
                self.id,
                u8::from(enabled),
            )
        })
    }
}

impl Drop for CallbackResource {
    fn drop(&mut self) {
        if self.cancel().is_err()
            && let Some(handler) = self.handler.take()
        {
            std::mem::forget(handler);
        }
    }
}

impl Events {
    pub fn subscribe<F>(
        &self,
        events: u64,
        target: WidgetId,
        handler: F,
    ) -> Result<CallbackResource, Error>
    where
        F: FnMut(Event) + Send + 'static,
    {
        let table = self.0.query::<EventsApiV2>(IFACE_EVENTS)?;
        let function = table
            .subscribe
            .ok_or(Error::MissingFunction("events.subscribe"))?;
        let release = table
            .release
            .ok_or(Error::MissingFunction("events.release"))?;
        let mut handler: Box<Handler> = Box::new(Mutex::new(Box::new(handler)));
        let spec = EventSubscriptionV2 {
            events,
            target,
            callback: Some(event_callback),
            callback_data: (&mut *handler as *mut Handler).cast(),
            ..Default::default()
        };
        let mut id = ResourceId::INVALID;
        // SAFETY: The descriptor and output remain valid; the boxed handler has a stable address.
        success(unsafe { function(table.prefix.context, self.0.token, &spec, &mut id) })?;
        Ok(CallbackResource {
            host: self.0.clone(),
            id,
            context: table.prefix.context as usize,
            release,
            handler: Some(handler),
        })
    }

    pub fn timer<F>(&self, mut spec: TimerSpecV2, handler: F) -> Result<CallbackResource, Error>
    where
        F: FnMut(Event) + Send + 'static,
    {
        let table = self.0.query::<EventsApiV2>(IFACE_EVENTS)?;
        let function = table
            .create_timer
            .ok_or(Error::MissingFunction("events.create_timer"))?;
        let release = table
            .release
            .ok_or(Error::MissingFunction("events.release"))?;
        let mut handler: Box<Handler> = Box::new(Mutex::new(Box::new(handler)));
        spec.callback = Some(event_callback);
        spec.callback_data = (&mut *handler as *mut Handler).cast();
        let mut id = ResourceId::INVALID;
        // SAFETY: The descriptor and output remain valid; the boxed handler has a stable address.
        success(unsafe { function(table.prefix.context, self.0.token, &spec, &mut id) })?;
        Ok(CallbackResource {
            host: self.0.clone(),
            id,
            context: table.prefix.context as usize,
            release,
            handler: Some(handler),
        })
    }

    pub fn island_state(&self) -> Result<IslandStateV2, Error> {
        let table = self.0.query::<EventsApiV2>(IFACE_EVENTS)?;
        let function = table
            .island_state
            .ok_or(Error::MissingFunction("events.island_state"))?;
        let mut state = IslandStateV2 {
            struct_size: std::mem::size_of::<IslandStateV2>() as u32,
            ..Default::default()
        };
        // SAFETY: The output is initialized with its supported ABI size.
        success(unsafe { function(table.prefix.context, self.0.token, &mut state) })?;
        Ok(state)
    }

    pub fn set_animation(&self, target: WidgetId, enabled: bool) -> Result<(), Error> {
        let table = self.0.query::<EventsApiV2>(IFACE_EVENTS)?;
        let function = table
            .set_animation
            .ok_or(Error::MissingFunction("events.set_animation"))?;
        // SAFETY: The host validates resource ownership and the boolean value.
        success(unsafe {
            function(
                table.prefix.context,
                self.0.token,
                target,
                u8::from(enabled),
            )
        })
    }
}

impl Input {
    pub fn set_regions(&self, target: WidgetId, regions: &[InputRegionV2]) -> Result<(), Error> {
        let table = self.0.query::<InputApiV2>(IFACE_INPUT)?;
        let function = table
            .set_regions
            .ok_or(Error::MissingFunction("input.set_regions"))?;
        let count = u32::try_from(regions.len()).map_err(|_| Error::Length)?;
        // SAFETY: The region slice remains readable until the host copies it.
        success(unsafe {
            function(
                table.prefix.context,
                self.0.token,
                target,
                regions.as_ptr(),
                count,
            )
        })
    }

    pub fn release_capture(&self, target: WidgetId) -> Result<(), Error> {
        let table = self.0.query::<InputApiV2>(IFACE_INPUT)?;
        let function = table
            .release_capture
            .ok_or(Error::MissingFunction("input.release_capture"))?;
        // SAFETY: The host validates the target and caller token.
        success(unsafe { function(table.prefix.context, self.0.token, target) })
    }
}

impl Commands {
    pub fn register<F>(
        &self,
        mut spec: CommandSpecV2,
        handler: F,
    ) -> Result<CallbackResource, Error>
    where
        F: FnMut(Event) + Send + 'static,
    {
        let table = self.0.query::<CommandApiV2>(IFACE_COMMAND)?;
        let function = table
            .register
            .ok_or(Error::MissingFunction("command.register"))?;
        let release = table
            .release
            .ok_or(Error::MissingFunction("command.release"))?;
        let mut handler: Box<Handler> = Box::new(Mutex::new(Box::new(handler)));
        spec.callback = Some(event_callback);
        spec.callback_data = (&mut *handler as *mut Handler).cast();
        let mut id = ResourceId::INVALID;
        // SAFETY: The descriptor and output remain valid; the boxed handler has a stable address.
        success(unsafe { function(table.prefix.context, self.0.token, &spec, &mut id) })?;
        Ok(CallbackResource {
            host: self.0.clone(),
            id,
            context: table.prefix.context as usize,
            release,
            handler: Some(handler),
        })
    }

    pub fn list(&self) -> Result<Vec<CommandInfoV2>, Error> {
        let table = self.0.query::<CommandApiV2>(IFACE_COMMAND)?;
        let function = table.list.ok_or(Error::MissingFunction("command.list"))?;
        read_list(|out, count, required| {
            // SAFETY: read_list provides writable arrays of count elements and a count output.
            unsafe { function(table.prefix.context, self.0.token, out, count, required) }
        })
    }

    pub fn execute(&self, id: &str, data: &[u8]) -> Result<u64, Error> {
        let table = self.0.query::<CommandApiV2>(IFACE_COMMAND)?;
        let function = table
            .execute
            .ok_or(Error::MissingFunction("command.execute"))?;
        let mut sequence = 0;
        // SAFETY: All slices and the sequence output live through the synchronous call.
        success(unsafe {
            function(
                table.prefix.context,
                self.0.token,
                Utf8Slice::borrowed(id),
                ByteSlice::borrowed(data),
                &mut sequence,
            )
        })?;
        Ok(sequence)
    }
}

impl Assistant {
    /// Registers a tool the voice assistant may call. `handler` receives the JSON arguments and
    /// returns the text the assistant sees, or an error message.
    pub fn register<F>(
        &self,
        key: &str,
        description: &str,
        schema: &str,
        mut handler: F,
    ) -> Result<CallbackResource, Error>
    where
        F: FnMut(&str) -> Result<String, String> + Send + 'static,
    {
        let table = self.0.query::<AssistantApiV2>(IFACE_ASSISTANT)?;
        let function = table
            .register
            .ok_or(Error::MissingFunction("assistant.register"))?;
        let respond = table
            .respond
            .ok_or(Error::MissingFunction("assistant.respond"))?;
        let release = table
            .release
            .ok_or(Error::MissingFunction("assistant.release"))?;
        let context = table.prefix.context as usize;
        let token = self.0.token;
        let mut handler: Box<Handler> = Box::new(Mutex::new(Box::new(move |event: Event| {
            if event.kind != EVENT_ASSISTANT {
                return;
            }
            let args = String::from_utf8_lossy(&event.data);
            let (status, text) = match handler(&args) {
                Ok(text) => (ASSISTANT_RESULT_OK, text),
                Err(text) => (ASSISTANT_RESULT_ERROR, text),
            };
            // SAFETY: The host keeps the table context alive while plugin callbacks run.
            let _ = unsafe {
                respond(
                    context as *mut _,
                    token,
                    event.sequence,
                    status,
                    Utf8Slice::borrowed(&text),
                )
            };
        })));
        let spec = AssistantToolSpecV2 {
            flags: ASSISTANT_TOOL_ENABLED,
            key: str_to_fixed(key),
            description: str_to_fixed(description),
            schema: Utf8Slice::borrowed(schema),
            callback: Some(event_callback),
            callback_data: (&mut *handler as *mut Handler).cast(),
            ..Default::default()
        };
        let mut id = ResourceId::INVALID;
        // SAFETY: The descriptor, schema and output remain valid; the boxed handler has a stable address.
        success(unsafe { function(table.prefix.context, self.0.token, &spec, &mut id) })?;
        Ok(CallbackResource {
            host: self.0.clone(),
            id,
            context: table.prefix.context as usize,
            release,
            handler: Some(handler),
        })
    }
}

pub struct Surface {
    widget: Widget,
}

impl std::ops::Deref for Surface {
    type Target = Widget;
    fn deref(&self) -> &Widget {
        &self.widget
    }
}

impl Surfaces {
    pub fn create(&self, spec: SurfaceSpecV2) -> Result<Surface, Error> {
        let table = self.0.query::<SurfaceApiV2>(IFACE_SURFACE)?;
        let function = table
            .create
            .ok_or(Error::MissingFunction("surface.create"))?;
        let mut id = WidgetId::INVALID;
        // SAFETY: The sized descriptor and output remain live through registration.
        success(unsafe { function(table.prefix.context, self.0.token, &spec, &mut id) })?;
        Ok(Surface {
            widget: Widget {
                host: self.0.clone(),
                id,
            },
        })
    }
}

impl Surface {
    pub fn update(&self, spec: SurfaceSpecV2) -> Result<(), Error> {
        let table = self.widget.host.query::<SurfaceApiV2>(IFACE_SURFACE)?;
        let function = table
            .update
            .ok_or(Error::MissingFunction("surface.update"))?;
        // SAFETY: The descriptor remains readable until return and this surface owns its ID.
        success(unsafe {
            function(
                table.prefix.context,
                self.widget.host.token,
                self.widget.id,
                &spec,
            )
        })
    }

    pub fn show(&self) -> Result<(), Error> {
        let table = self.widget.host.query::<SurfaceApiV2>(IFACE_SURFACE)?;
        let function = table
            .show_page
            .ok_or(Error::MissingFunction("surface.show_page"))?;
        // SAFETY: The host validates surface ownership and its registered kind.
        success(unsafe { function(table.prefix.context, self.widget.host.token, self.widget.id) })
    }
}

impl MediaSessions {
    pub fn list(&self) -> Result<Vec<MediaSessionV2>, Error> {
        let table = self.0.query::<MediaSessionApiV2>(IFACE_MEDIA_SESSION)?;
        let function = table
            .list
            .ok_or(Error::MissingFunction("media_session.list"))?;
        read_list(|out, count, required| {
            // SAFETY: read_list provides writable arrays of count elements and a count output.
            unsafe { function(table.prefix.context, self.0.token, out, count, required) }
        })
    }

    pub fn send(&self, session: u64, command: u32, position_ms: u64) -> Result<u64, Error> {
        let table = self.0.query::<MediaSessionApiV2>(IFACE_MEDIA_SESSION)?;
        let function = table
            .send
            .ok_or(Error::MissingFunction("media_session.send"))?;
        let mut sequence = 0;
        // SAFETY: The sequence output is writable and the host validates all request values.
        success(unsafe {
            function(
                table.prefix.context,
                self.0.token,
                session,
                command,
                position_ms,
                &mut sequence,
            )
        })?;
        Ok(sequence)
    }
}

fn read_list<T: Copy + Default>(
    read: impl Fn(*mut T, u32, *mut u32) -> PluginStatus,
) -> Result<Vec<T>, Error> {
    for _ in 0..3 {
        let mut count = 0;
        success(read(std::ptr::null_mut(), 0, &mut count))?;
        if count > 16384 {
            return Err(Error::Length);
        }
        let mut values = vec![T::default(); count as usize];
        let status = read(values.as_mut_ptr(), count, &mut count);
        if status == PluginStatus::LimitExceeded {
            continue;
        }
        success(status)?;
        if count as usize > values.len() {
            return Err(Error::Length);
        }
        values.truncate(count as usize);
        return Ok(values);
    }
    Err(Error::LimitExceeded)
}
