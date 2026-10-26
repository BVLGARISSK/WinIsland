use crate::services::extensions;
use std::ffi::c_void;
use winisland_plugin_api::{
    ActivityApiV2, AssistantApiV2, CommandApiV2, EventsApiV2, InputApiV2, MediaSessionApiV2,
    SurfaceApiV2,
};

use winisland_plugin_api::abi::{
    self, ContextApiV2, HostStateApiV2, I18nApiV2, ImageApiV2, LogApiV2, LyricsTransformApiV2,
    MediaApiV2, PluginHostV2, SettingsApiV2, StoreApiV2, SystemApiV2, TablePrefix, TextApiV2,
    WidgetApiV2,
};

use crate::runtime::HostRuntime;
use crate::services::{
    activity, context, host_state, i18n, image, log, lyrics, media, settings, store, system, text,
    widget,
};

pub(crate) struct AbiTables {
    activity: ActivityApiV2,
    pub host: PluginHostV2,
    context: ContextApiV2,
    media: MediaApiV2,
    i18n: I18nApiV2,
    host_state: HostStateApiV2,
    widget: WidgetApiV2,
    lyrics: LyricsTransformApiV2,
    settings: SettingsApiV2,
    text: TextApiV2,
    image: ImageApiV2,
    store: StoreApiV2,
    log: LogApiV2,
    input: InputApiV2,
    command: CommandApiV2,
    assistant: AssistantApiV2,
    surface: SurfaceApiV2,
    events: EventsApiV2,
    media_session: MediaSessionApiV2,
    system: SystemApiV2,
}

fn prefix<T>(version: u32) -> TablePrefix {
    TablePrefix {
        struct_size: std::mem::size_of::<T>() as u32,
        version,
        context: std::ptr::null_mut(),
    }
}

impl AbiTables {
    pub fn new(host_build: u32) -> Self {
        Self {
            activity: ActivityApiV2 {
                prefix: prefix::<ActivityApiV2>(abi::IFACE_VERSION_1),
                create: Some(activity::create),
                update: Some(activity::update),
                release: Some(activity::release),
            },
            system: SystemApiV2 {
                prefix: prefix::<SystemApiV2>(abi::IFACE_VERSION_1),
                local_datetime: Some(system::local_datetime),
                lunar_date: Some(system::lunar_date),
                current_language: Some(system::current_language),
            },
            input: InputApiV2 {
                prefix: prefix::<InputApiV2>(abi::IFACE_VERSION_1),
                set_regions: Some(extensions::set_regions),
                release_capture: Some(extensions::release_capture),
            },
            command: CommandApiV2 {
                prefix: prefix::<CommandApiV2>(abi::IFACE_VERSION_1),
                register: Some(extensions::register_command),
                set_enabled: Some(extensions::command_enabled),
                list: Some(extensions::list_commands),
                execute: Some(extensions::execute_command),
                release: Some(extensions::release_command),
            },
            assistant: AssistantApiV2 {
                prefix: prefix::<AssistantApiV2>(abi::IFACE_VERSION_1),
                register: Some(extensions::register_assistant_tool),
                respond: Some(extensions::respond_assistant),
                release: Some(extensions::release_assistant_tool),
            },
            surface: SurfaceApiV2 {
                prefix: prefix::<SurfaceApiV2>(abi::IFACE_VERSION_1),
                create: Some(extensions::create_surface),
                update: Some(extensions::update_surface),
                release: Some(widget::release),
                submit_draw_list: Some(widget::submit_draw_list),
                logical_size: Some(widget::logical_size),
                show_page: Some(extensions::show_page),
            },
            events: EventsApiV2 {
                prefix: prefix::<EventsApiV2>(abi::IFACE_VERSION_1),
                subscribe: Some(extensions::subscribe),
                create_timer: Some(extensions::create_timer),
                release: Some(extensions::release_event),
                island_state: Some(extensions::island_state),
                set_animation: Some(extensions::set_animation),
            },
            media_session: MediaSessionApiV2 {
                prefix: prefix::<MediaSessionApiV2>(abi::IFACE_VERSION_1),
                list: Some(extensions::list_sessions),
                send: Some(extensions::send_media),
            },
            host: PluginHostV2 {
                prefix: prefix::<PluginHostV2>(abi::ABI_VERSION_2),
                host_build,
                query: Some(query),
            },
            context: ContextApiV2 {
                prefix: prefix::<ContextApiV2>(abi::IFACE_VERSION_1),
                create: Some(context::create),
                update: Some(context::update),
                release: Some(context::release),
            },
            media: MediaApiV2 {
                prefix: prefix::<MediaApiV2>(abi::IFACE_VERSION_1),
                create: Some(media::create),
                update: Some(media::update),
                release: Some(media::release),
                current_title: Some(media::current_title),
            },
            i18n: I18nApiV2 {
                prefix: prefix::<I18nApiV2>(abi::IFACE_VERSION_1),
                register_bundle: Some(i18n::register_bundle),
                release_bundle: Some(i18n::release_bundle),
            },
            host_state: HostStateApiV2 {
                prefix: prefix::<HostStateApiV2>(abi::IFACE_VERSION_1),
                get: Some(host_state::get),
                subscribe: Some(host_state::subscribe),
                release_subscription: Some(host_state::release_subscription),
            },
            widget: WidgetApiV2 {
                prefix: prefix::<WidgetApiV2>(abi::IFACE_VERSION_1),
                create: Some(widget::create),
                update: Some(widget::update),
                release: Some(widget::release),
                submit_draw_list: Some(widget::submit_draw_list),
                request_redraw: Some(widget::request_redraw),
                logical_size: Some(widget::logical_size),
            },
            lyrics: LyricsTransformApiV2 {
                prefix: prefix::<LyricsTransformApiV2>(abi::IFACE_VERSION_1),
                register: Some(lyrics::register),
                release: Some(lyrics::release),
            },
            settings: SettingsApiV2 {
                prefix: prefix::<SettingsApiV2>(abi::IFACE_VERSION_1),
                create: Some(settings::create),
                update: Some(settings::update),
                release: Some(settings::release),
            },
            text: TextApiV2 {
                prefix: prefix::<TextApiV2>(abi::IFACE_VERSION_1),
                measure: Some(text::measure),
                font_family: Some(text::font_family),
            },
            image: ImageApiV2 {
                prefix: prefix::<ImageApiV2>(abi::IFACE_VERSION_1),
                decode: Some(image::decode),
                upload_rgba: Some(image::upload_rgba),
                album_art: Some(image::album_art),
                release: Some(image::release),
            },
            store: StoreApiV2 {
                prefix: prefix::<StoreApiV2>(abi::IFACE_VERSION_1),
                get: Some(store::get),
                set: Some(store::set),
                delete: Some(store::delete),
            },
            log: LogApiV2 {
                prefix: prefix::<LogApiV2>(abi::IFACE_VERSION_1),
                write: Some(log::write),
            },
        }
    }

    pub fn bind(&mut self, context: *mut c_void) {
        self.activity.prefix.context = context;
        self.system.prefix.context = context;
        self.input.prefix.context = context;
        self.command.prefix.context = context;
        self.assistant.prefix.context = context;
        self.surface.prefix.context = context;
        self.events.prefix.context = context;
        self.media_session.prefix.context = context;
        self.host.prefix.context = context;
        self.context.prefix.context = context;
        self.media.prefix.context = context;
        self.i18n.prefix.context = context;
        self.host_state.prefix.context = context;
        self.widget.prefix.context = context;
        self.lyrics.prefix.context = context;
        self.settings.prefix.context = context;
        self.text.prefix.context = context;
        self.image.prefix.context = context;
        self.store.prefix.context = context;
        self.log.prefix.context = context;
    }
}

unsafe extern "C" fn query(
    context: *mut c_void,
    interface: u32,
    min_version: u32,
) -> *const c_void {
    if context.is_null() || min_version > abi::IFACE_VERSION_1 {
        return std::ptr::null();
    }
    // SAFETY: The context is the stable Box<HostRuntime> address while the host table is live.
    let runtime = unsafe { &*context.cast::<HostRuntime>() };
    let tables = &runtime.tables;
    match interface {
        abi::IFACE_ACTIVITY => (&tables.activity as *const ActivityApiV2).cast(),
        abi::IFACE_SYSTEM => (&tables.system as *const SystemApiV2).cast(),
        abi::IFACE_INPUT => (&tables.input as *const InputApiV2).cast(),
        abi::IFACE_COMMAND => (&tables.command as *const CommandApiV2).cast(),
        abi::IFACE_ASSISTANT => (&tables.assistant as *const AssistantApiV2).cast(),
        abi::IFACE_SURFACE => (&tables.surface as *const SurfaceApiV2).cast(),
        abi::IFACE_EVENTS => (&tables.events as *const EventsApiV2).cast(),
        abi::IFACE_MEDIA_SESSION => (&tables.media_session as *const MediaSessionApiV2).cast(),
        abi::IFACE_CONTEXT => (&tables.context as *const ContextApiV2).cast(),
        abi::IFACE_MEDIA => (&tables.media as *const MediaApiV2).cast(),
        abi::IFACE_I18N => (&tables.i18n as *const I18nApiV2).cast(),
        abi::IFACE_HOST_STATE => (&tables.host_state as *const HostStateApiV2).cast(),
        abi::IFACE_WIDGET => (&tables.widget as *const WidgetApiV2).cast(),
        abi::IFACE_LYRICS_TRANSFORM => (&tables.lyrics as *const LyricsTransformApiV2).cast(),
        abi::IFACE_SETTINGS => (&tables.settings as *const SettingsApiV2).cast(),
        abi::IFACE_TEXT => (&tables.text as *const TextApiV2).cast(),
        abi::IFACE_IMAGE => (&tables.image as *const ImageApiV2).cast(),
        abi::IFACE_STORE => (&tables.store as *const StoreApiV2).cast(),
        abi::IFACE_LOG => (&tables.log as *const LogApiV2).cast(),
        _ => std::ptr::null(),
    }
}
