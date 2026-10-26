use std::ffi::c_void;
use std::time::{Duration, Instant};

use winisland_plugin_api::types::v2::widget::WidgetSpecV2;
use winisland_plugin_api::*;

use super::{read_bytes, read_struct, read_utf8, runtime};
use crate::extensions::{AssistantTool, Callback, Command, Event, HostRequest, Timer};
use crate::resources::ResourceKind;
use crate::runtime::HostRuntime;

pub(crate) const BUILTINS: &[(&str, &str)] = &[
    ("winisland.expand", "Expand island"),
    ("winisland.collapse", "Collapse island"),
    ("winisland.settings", "Open settings"),
    ("winisland.media.toggle", "Play / pause"),
    ("winisland.media.next", "Next track"),
    ("winisland.media.previous", "Previous track"),
];

fn status(result: Result<(), PluginStatus>) -> PluginStatus {
    result.err().unwrap_or(PluginStatus::Ok)
}

unsafe fn access<'a>(
    context: *mut c_void,
    token: PluginToken,
    cap: u64,
) -> Result<&'a HostRuntime, PluginStatus> {
    // SAFETY: The ABI caller keeps its host context alive through this call.
    let host = unsafe { runtime(context)? };
    host.registry.require(token, cap)?;
    Ok(host)
}

fn target(host: &HostRuntime, token: PluginToken, id: WidgetId) -> Result<(), PluginStatus> {
    host.resources
        .require(token, ResourceKind::Widget, id.get())
}

fn text(bytes: &[u8]) -> Result<&str, PluginStatus> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(PluginStatus::InvalidArgument)?;
    std::str::from_utf8(&bytes[..end]).map_err(|_| PluginStatus::InvalidArgument)
}

fn key(bytes: &[u8]) -> Result<&str, PluginStatus> {
    let value = text(bytes)?;
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(PluginStatus::InvalidArgument);
    }
    Ok(value)
}

unsafe fn output_list<T: Copy>(
    values: &[T],
    out: *mut T,
    capacity: u32,
    required: *mut u32,
) -> Result<(), PluginStatus> {
    if required.is_null() || (capacity > 0 && out.is_null()) {
        return Err(PluginStatus::InvalidArgument);
    }
    // SAFETY: The caller provides the writable count and capacity-sized output allocation.
    unsafe {
        *required = values.len() as u32;
    }
    if out.is_null() && capacity == 0 {
        return Ok(());
    }
    if (capacity as usize) < values.len() {
        return Err(PluginStatus::LimitExceeded);
    }
    if !values.is_empty() {
        // SAFETY: The output allocation has room for all copied ABI values and cannot alias this snapshot.
        unsafe {
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, values.len());
        }
    }
    Ok(())
}

pub unsafe extern "C" fn set_regions(
    context: *mut c_void,
    token: PluginToken,
    id: WidgetId,
    regions: *const InputRegionV2,
    count: u32,
) -> PluginStatus {
    status((|| {
        // SAFETY: The context and region array remain readable through this ABI call.
        let host = unsafe { access(context, token, CAP_INPUT)? };
        target(host, token, id)?;
        if count > 128 || (count > 0 && regions.is_null()) {
            return Err(PluginStatus::InvalidArgument);
        }
        let regions = if count == 0 {
            Vec::new()
        } else {
            // SAFETY: The caller supplies count readable, properly aligned region values.
            unsafe { std::slice::from_raw_parts(regions, count as usize) }.to_vec()
        };
        let mut ids = std::collections::HashSet::new();
        if regions.iter().any(|r| {
            !ids.insert(r.id)
                || r.reserved != 0
                || r.flags & !INPUT_FLAGS != 0
                || ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
                || r.width <= 0.0
                || r.height <= 0.0
        }) {
            return Err(PluginStatus::InvalidArgument);
        }
        host.extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?
            .regions
            .insert(id.get(), regions);
        host.extensions.changed();
        Ok(())
    })())
}

pub unsafe extern "C" fn release_capture(
    context: *mut c_void,
    token: PluginToken,
    id: WidgetId,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller provides a live host context.
        let host = unsafe { access(context, token, CAP_INPUT)? };
        target(host, token, id)?;
        host.extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?
            .release_capture(id.get());
        host.extensions.changed();
        Ok(())
    })())
}

pub unsafe extern "C" fn subscribe(
    context: *mut c_void,
    token: PluginToken,
    spec: *const EventSubscriptionV2,
    out: *mut ResourceId,
) -> PluginStatus {
    status((|| {
        if out.is_null() {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: The ABI caller supplies a live context and a readable sized descriptor.
        let (host, spec) = unsafe { (access(context, token, CAP_EVENTS)?, read_struct(spec)?) };
        if spec.events == 0
            || spec.events & !(EVENT_ALL & !(EVENT_TIMER | EVENT_COMMAND)) != 0
            || spec.reserved != 0
        {
            return Err(PluginStatus::InvalidArgument);
        }
        if spec.target != WidgetId::INVALID {
            target(host, token, spec.target)?;
        }
        if spec.events & (EVENT_INPUT | EVENT_VISIBILITY | EVENT_RESIZE) != 0
            && spec.target == WidgetId::INVALID
        {
            return Err(PluginStatus::InvalidArgument);
        }
        let function = spec.callback.ok_or(PluginStatus::InvalidArgument)?;
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        let id = host.resources.allocate(token, ResourceKind::Event, 0)?;
        state.callbacks.insert(
            id,
            Callback {
                token,
                target: spec.target.get(),
                mask: spec.events,
                function,
                data: spec.callback_data as usize,
                in_flight: false,
                timer: None,
            },
        );
        // SAFETY: This newly allocated callback ID belongs to the caller and out is writable.
        unsafe {
            *out = ResourceId::from_raw(id);
        }
        Ok(())
    })())
}

pub unsafe extern "C" fn create_timer(
    context: *mut c_void,
    token: PluginToken,
    spec: *const TimerSpecV2,
    out: *mut ResourceId,
) -> PluginStatus {
    status((|| {
        if out.is_null() {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: The ABI caller supplies a live context and readable sized timer descriptor.
        let (host, spec) = unsafe { (access(context, token, CAP_EVENTS)?, read_struct(spec)?) };
        if spec.flags & !TIMER_VISIBLE_ONLY != 0
            || spec.delay_ms > 604_800_000
            || spec.interval_ms > 604_800_000
            || (spec.interval_ms != 0 && spec.interval_ms < 4)
            || (spec.flags != 0 && spec.target == WidgetId::INVALID)
        {
            return Err(PluginStatus::InvalidArgument);
        }
        if spec.target != WidgetId::INVALID {
            target(host, token, spec.target)?;
        }
        let function = spec.callback.ok_or(PluginStatus::InvalidArgument)?;
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        let id = host.resources.allocate(token, ResourceKind::Event, 0)?;
        state.callbacks.insert(
            id,
            Callback {
                token,
                target: spec.target.get(),
                mask: 0,
                function,
                data: spec.callback_data as usize,
                in_flight: false,
                timer: Some(Timer {
                    deadline: Some(Instant::now() + Duration::from_millis(spec.delay_ms)),
                    interval: Duration::from_millis(spec.interval_ms),
                    visible_only: spec.flags & TIMER_VISIBLE_ONLY != 0,
                }),
            },
        );
        if let Some(worker) = state.workers.get(&token) {
            worker.unpark();
        }
        // SAFETY: This newly allocated callback ID belongs to the caller and out is writable.
        unsafe {
            *out = ResourceId::from_raw(id);
        }
        Ok(())
    })())
}

unsafe fn release_callback(
    context: *mut c_void,
    token: PluginToken,
    id: ResourceId,
    cap: u64,
    kind: ResourceKind,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller provides its live instance context.
        let host = unsafe { runtime(context)? };
        if host.registry.get(token)?.capabilities & cap == 0 {
            return Err(PluginStatus::CapabilityMissing);
        }
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        host.resources.require(token, kind, id.get())?;
        if state.callbacks.get(&id.get()).is_some_and(|c| c.in_flight) {
            return Err(PluginStatus::LimitExceeded);
        }
        host.resources.release(token, kind, id.get())?;
        state.callbacks.remove(&id.get());
        state.commands.remove(&id.get());
        state.assistant_tools.remove(&id.get());
        let mut cancelled = Vec::new();
        if let Some(queue) = state.pending.get_mut(&token) {
            queue.retain(|(resource, event)| {
                if *resource != id.get() {
                    return true;
                }
                if let Some(completion) = event.completion {
                    cancelled.push(completion);
                }
                false
            });
        }
        if let Some(worker) = state.workers.get(&token) {
            worker.unpark();
        }
        drop(state);
        for (caller, sequence) in cancelled {
            host.extensions
                .complete(caller, sequence, PluginStatus::StaleHandle);
        }
        host.extensions.changed();
        Ok(())
    })())
}

pub unsafe extern "C" fn release_event(
    context: *mut c_void,
    token: PluginToken,
    id: ResourceId,
) -> PluginStatus {
    // SAFETY: Arguments obey the event resource release ABI.
    unsafe { release_callback(context, token, id, CAP_EVENTS, ResourceKind::Event) }
}

pub unsafe extern "C" fn release_command(
    context: *mut c_void,
    token: PluginToken,
    id: ResourceId,
) -> PluginStatus {
    // SAFETY: Arguments obey the command resource release ABI.
    unsafe { release_callback(context, token, id, CAP_COMMAND, ResourceKind::Command) }
}

pub unsafe extern "C" fn island_state(
    context: *mut c_void,
    token: PluginToken,
    out: *mut IslandStateV2,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies a live context and initialized output with its structure size.
        let host = unsafe { access(context, token, CAP_EVENTS)? };
        // SAFETY: The ABI requires a writable, initialized sized IslandStateV2 output.
        unsafe {
            read_struct(out)?;
        }
        let state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        // SAFETY: The sized output was checked and remains exclusively writable.
        unsafe {
            *out = state.island;
        }
        Ok(())
    })())
}

pub unsafe extern "C" fn set_animation(
    context: *mut c_void,
    token: PluginToken,
    id: WidgetId,
    enabled: u8,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies its live host context.
        let host = unsafe { access(context, token, CAP_EVENTS)? };
        target(host, token, id)?;
        if enabled > 1 {
            return Err(PluginStatus::InvalidArgument);
        }
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        state.animations.insert(id.get(), enabled != 0);
        if let Some(worker) = state.workers.get(&token) {
            worker.unpark();
        }
        Ok(())
    })())
}

pub unsafe extern "C" fn register_command(
    context: *mut c_void,
    token: PluginToken,
    spec: *const CommandSpecV2,
    out: *mut ResourceId,
) -> PluginStatus {
    status((|| {
        if out.is_null() {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: The ABI caller supplies a live context and a readable sized descriptor.
        let (host, spec) = unsafe { (access(context, token, CAP_COMMAND)?, read_struct(spec)?) };
        let name = format!("{}.{}", host.registry.get(token)?.id, key(&spec.key)?);
        let title = text(&spec.title)?.to_owned();
        let function = spec.callback.ok_or(PluginStatus::InvalidArgument)?;
        if title.is_empty()
            || spec.flags & !(COMMAND_ENABLED | COMMAND_TRAY) != 0
            || spec.hotkey_modifiers & !15 != 0
            || (spec.hotkey_key != 0
                && !(spec.hotkey_key <= 127 && (spec.hotkey_key as u8).is_ascii_alphanumeric()))
        {
            return Err(PluginStatus::InvalidArgument);
        }
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        if BUILTINS.iter().any(|(id, _)| *id == name)
            || state.commands.values().any(|c| c.id == name)
        {
            return Err(PluginStatus::InvalidArgument);
        }
        let id = host.resources.allocate(token, ResourceKind::Command, 0)?;
        state.callbacks.insert(
            id,
            Callback {
                token,
                target: 0,
                mask: 0,
                function,
                data: spec.callback_data as usize,
                in_flight: false,
                timer: None,
            },
        );
        state.commands.insert(
            id,
            Command {
                resource: id,
                id: name,
                title,
                flags: spec.flags,
                modifiers: spec.hotkey_modifiers,
                key: spec.hotkey_key,
            },
        );
        drop(state);
        host.extensions.changed();
        // SAFETY: The output points to writable caller-owned storage for the newly allocated ID.
        unsafe {
            *out = ResourceId::from_raw(id);
        }
        Ok(())
    })())
}

pub unsafe extern "C" fn register_assistant_tool(
    context: *mut c_void,
    token: PluginToken,
    spec: *const AssistantToolSpecV2,
    out: *mut ResourceId,
) -> PluginStatus {
    status((|| {
        if out.is_null() {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: The ABI caller supplies a live context and a readable sized descriptor.
        let (host, spec) = unsafe { (access(context, token, CAP_ASSISTANT)?, read_struct(spec)?) };
        let plugin = host.registry.get(token)?.id;
        let id = format!("{plugin}.{}", key(&spec.key)?);
        let description = text(&spec.description)?.to_owned();
        // SAFETY: The schema slice stays readable through this synchronous call.
        let schema = unsafe { read_utf8(spec.schema, 16 * 1024)? };
        let function = spec.callback.ok_or(PluginStatus::InvalidArgument)?;
        if description.is_empty() || spec.flags & !ASSISTANT_TOOL_ENABLED != 0 {
            return Err(PluginStatus::InvalidArgument);
        }
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        if state.assistant_tools.values().any(|tool| tool.id == id) {
            return Err(PluginStatus::InvalidArgument);
        }
        let resource = host
            .resources
            .allocate(token, ResourceKind::AssistantTool, 0)?;
        state.callbacks.insert(
            resource,
            Callback {
                token,
                target: 0,
                mask: 0,
                function,
                data: spec.callback_data as usize,
                in_flight: false,
                timer: None,
            },
        );
        state.assistant_tools.insert(
            resource,
            AssistantTool {
                resource,
                id,
                plugin,
                description,
                schema,
            },
        );
        drop(state);
        host.extensions.changed();
        // SAFETY: The output points to writable caller-owned storage for the newly allocated ID.
        unsafe {
            *out = ResourceId::from_raw(resource);
        }
        Ok(())
    })())
}

pub unsafe extern "C" fn respond_assistant(
    context: *mut c_void,
    token: PluginToken,
    sequence: u64,
    result: u32,
    text: Utf8Slice,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies a live context and a text slice readable through this call.
        let (host, text) = unsafe {
            (
                access(context, token, CAP_ASSISTANT)?,
                read_utf8(text, 64 * 1024)?,
            )
        };
        if result > ASSISTANT_RESULT_ERROR {
            return Err(PluginStatus::InvalidArgument);
        }
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        if state.assistant_calls.get(&sequence) != Some(&token) {
            return Err(PluginStatus::StaleHandle);
        }
        state.assistant_calls.remove(&sequence);
        state
            .assistant_results
            .insert(sequence, (result == ASSISTANT_RESULT_OK, text));
        drop(state);
        host.extensions.changed();
        Ok(())
    })())
}

pub unsafe extern "C" fn release_assistant_tool(
    context: *mut c_void,
    token: PluginToken,
    id: ResourceId,
) -> PluginStatus {
    // SAFETY: Arguments obey the assistant tool resource release ABI.
    unsafe {
        release_callback(
            context,
            token,
            id,
            CAP_ASSISTANT,
            ResourceKind::AssistantTool,
        )
    }
}

pub unsafe extern "C" fn command_enabled(
    context: *mut c_void,
    token: PluginToken,
    id: ResourceId,
    enabled: u8,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies a live context.
        let host = unsafe { access(context, token, CAP_COMMAND)? };
        host.resources
            .require(token, ResourceKind::Command, id.get())?;
        if enabled > 1 {
            return Err(PluginStatus::InvalidArgument);
        }
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        let command = state
            .commands
            .get_mut(&id.get())
            .ok_or(PluginStatus::StaleHandle)?;
        command.flags = (command.flags & !COMMAND_ENABLED) | u32::from(enabled);
        drop(state);
        host.extensions.changed();
        Ok(())
    })())
}

pub unsafe extern "C" fn list_commands(
    context: *mut c_void,
    token: PluginToken,
    out: *mut CommandInfoV2,
    capacity: u32,
    required: *mut u32,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies a live host context.
        let host = unsafe { access(context, token, CAP_COMMAND)? };
        let mut values: Vec<_> = BUILTINS
            .iter()
            .map(|(id, title)| CommandInfoV2 {
                id: str_to_fixed(id),
                title: str_to_fixed(title),
                flags: COMMAND_ENABLED,
                ..Default::default()
            })
            .collect();
        values.extend(host.extensions.commands().iter().map(|c| CommandInfoV2 {
            id: str_to_fixed(&c.id),
            title: str_to_fixed(&c.title),
            flags: c.flags,
            ..Default::default()
        }));
        // SAFETY: The ABI caller supplies capacity output elements and a writable required count.
        unsafe { output_list(&values, out, capacity, required) }
    })())
}

pub unsafe extern "C" fn execute_command(
    context: *mut c_void,
    token: PluginToken,
    name: Utf8Slice,
    data: ByteSlice,
    out: *mut u64,
) -> PluginStatus {
    status((|| {
        if out.is_null() {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: All borrowed inputs remain readable through the synchronous call.
        let (host, name, data) = unsafe {
            (
                access(context, token, CAP_COMMAND)?,
                read_utf8(name, 160)?,
                read_bytes(data, 64 * 1024)?,
            )
        };
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        let sequence = if BUILTINS.iter().any(|(id, _)| *id == name) {
            state.request(token, HostRequest::Command { name, data })?
        } else {
            let id = state
                .commands
                .values()
                .find(|c| c.id == name && c.flags & COMMAND_ENABLED != 0)
                .map(|c| c.resource)
                .ok_or(PluginStatus::InvalidArgument)?;
            state.sequence = state
                .sequence
                .checked_add(1)
                .ok_or(PluginStatus::LimitExceeded)?;
            let sequence = state.sequence;
            state.enqueue(
                id,
                Event {
                    kind: EVENT_COMMAND,
                    sequence,
                    data,
                    completion: Some((token, sequence)),
                    ..Default::default()
                },
            )?;
            sequence
        };
        drop(state);
        host.extensions.changed();
        // SAFETY: The caller supplies a writable request sequence output.
        unsafe {
            *out = sequence;
        }
        Ok(())
    })())
}

fn valid_surface(spec: &SurfaceSpecV2) -> Result<(), PluginStatus> {
    key(&spec.key)?;
    text(&spec.title)?;
    if !(SURFACE_PAGE..=SURFACE_STATUS).contains(&spec.kind)
        || spec.flags & !SURFACE_ENABLED != 0
        || !spec.width.is_finite()
        || !spec.height.is_finite()
        || spec.width < 0.0
        || spec.height < 0.0
        || spec.width > 2048.0
        || spec.height > 2048.0
    {
        return Err(PluginStatus::InvalidArgument);
    }
    Ok(())
}

pub unsafe extern "C" fn create_surface(
    context: *mut c_void,
    token: PluginToken,
    spec: *const SurfaceSpecV2,
    out: *mut WidgetId,
) -> PluginStatus {
    status((|| {
        if out.is_null() {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: The caller supplies a live context and readable sized surface descriptor.
        let (host, spec) = unsafe { (access(context, token, CAP_SURFACE)?, read_struct(spec)?) };
        valid_surface(&spec)?;
        let ids = host.resources.list(token, ResourceKind::Widget)?;
        let mut state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        if ids
            .iter()
            .filter_map(|id| state.widgets.get(id))
            .any(|r| r.surface.is_some_and(|s| s.key == spec.key))
        {
            return Err(PluginStatus::InvalidArgument);
        }
        let id = host.resources.allocate(token, ResourceKind::Widget, 0)?;
        state.widgets.insert(
            id,
            crate::runtime::WidgetRecord {
                spec: WidgetSpecV2 {
                    key: spec.key,
                    title: spec.title,
                    ..Default::default()
                },
                surface: Some(spec),
                draw_list: std::sync::Arc::from([]),
                logical_width: spec.width.max(1.0),
                logical_height: spec.height.max(1.0),
                redraw: true,
                failures: 0,
                disabled: false,
            },
        );
        drop(state);
        host.extensions.changed();
        // SAFETY: The ID belongs to the caller and the output is writable.
        unsafe {
            *out = WidgetId::from_raw(id);
        }
        Ok(())
    })())
}

pub unsafe extern "C" fn update_surface(
    context: *mut c_void,
    token: PluginToken,
    id: WidgetId,
    spec: *const SurfaceSpecV2,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies a live context and readable sized surface descriptor.
        let (host, spec) = unsafe { (access(context, token, CAP_SURFACE)?, read_struct(spec)?) };
        valid_surface(&spec)?;
        target(host, token, id)?;
        let mut state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        let record = state
            .widgets
            .get_mut(&id.get())
            .ok_or(PluginStatus::StaleHandle)?;
        let old = record.surface.ok_or(PluginStatus::StaleHandle)?;
        if old.key != spec.key || old.kind != spec.kind {
            return Err(PluginStatus::InvalidArgument);
        }
        record.surface = Some(spec);
        record.redraw = true;
        state.activity_revision = state.activity_revision.wrapping_add(1);
        drop(state);
        host.extensions.changed();
        Ok(())
    })())
}

pub unsafe extern "C" fn show_page(
    context: *mut c_void,
    token: PluginToken,
    id: WidgetId,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies a live context.
        let host = unsafe { access(context, token, CAP_SURFACE)? };
        target(host, token, id)?;
        let state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        if !state
            .widgets
            .get(&id.get())
            .and_then(|r| r.surface)
            .is_some_and(|s| s.kind == SURFACE_PAGE && s.flags & SURFACE_ENABLED != 0)
        {
            return Err(PluginStatus::InvalidArgument);
        }
        drop(state);
        host.extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?
            .request(token, HostRequest::ShowPage(id.get()))?;
        host.extensions.changed();
        Ok(())
    })())
}

pub unsafe extern "C" fn list_sessions(
    context: *mut c_void,
    token: PluginToken,
    out: *mut MediaSessionV2,
    capacity: u32,
    required: *mut u32,
) -> PluginStatus {
    status((|| {
        // SAFETY: The caller supplies a live context.
        let host = unsafe { access(context, token, CAP_MEDIA_SESSION)? };
        let state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        // SAFETY: The ABI caller supplies capacity output elements and a writable count.
        unsafe { output_list(&state.sessions, out, capacity, required) }
    })())
}

pub unsafe extern "C" fn send_media(
    context: *mut c_void,
    token: PluginToken,
    session: u64,
    command: u32,
    position_ms: u64,
    out: *mut u64,
) -> PluginStatus {
    status((|| {
        if out.is_null() || !(1..=SESSION_PAUSE).contains(&command) {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: The caller supplies a live context.
        let host = unsafe { access(context, token, CAP_MEDIA_SESSION)? };
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        if session != 0 && !state.sessions.iter().any(|s| s.id == session) {
            return Err(PluginStatus::StaleHandle);
        }
        let sequence = state.request(
            token,
            HostRequest::Media {
                session,
                command,
                position_ms,
            },
        )?;
        drop(state);
        host.extensions.changed();
        // SAFETY: The caller supplies a writable request sequence output.
        unsafe {
            *out = sequence;
        }
        Ok(())
    })())
}
