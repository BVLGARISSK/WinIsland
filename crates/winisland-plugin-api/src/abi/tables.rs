use crate::types::v2::{
    ActivitySpecV2, AssistantToolSpecV2, CommandExecuteFnV2, CommandInfoV2, CommandSpecV2,
    EventSubscriptionV2, InputRegionV2, IslandStateV2, LocalDateTimeV2, LunarDateV2,
    MediaSessionV2, SurfaceSpecV2, TimerSpecV2,
};
use std::ffi::c_void;

use crate::abi::{PluginStatus, TablePrefix};
use crate::types::v2::context::{ContextDataV2, HostStateV2, MediaSourceDataV2};
use crate::types::v2::i18n::TranslationPairV2;
use crate::types::v2::lyrics::LyricsTransformerDataV2;
use crate::types::v2::settings::SettingsPageDataV2;
use crate::types::v2::widget::WidgetSpecV2;
use crate::types::v2::{
    ByteSlice, HostStateChangedFnV2, ImageId, PluginToken, ResourceId, TextMetricsV2, TextStyleV2,
    Utf8Slice, WidgetId,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ContextApiV2 {
    pub prefix: TablePrefix,
    pub create: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const ContextDataV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub update: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            ResourceId,
            *const ContextDataV2,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ActivityApiV2 {
    pub prefix: TablePrefix,
    pub create: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const ActivitySpecV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub update: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            ResourceId,
            *const ActivitySpecV2,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MediaApiV2 {
    pub prefix: TablePrefix,
    pub create: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const MediaSourceDataV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub update: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            ResourceId,
            *const MediaSourceDataV2,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
    pub current_title: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, *mut u8, u32, *mut u32) -> PluginStatus,
    >,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct I18nApiV2 {
    pub prefix: TablePrefix,
    pub register_bundle: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            Utf8Slice,
            *const TranslationPairV2,
            u32,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub release_bundle:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostStateApiV2 {
    pub prefix: TablePrefix,
    pub get:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, *mut HostStateV2) -> PluginStatus>,
    pub subscribe: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            HostStateChangedFnV2,
            *mut c_void,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub release_subscription:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WidgetApiV2 {
    pub prefix: TablePrefix,
    pub create: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const WidgetSpecV2,
            *mut WidgetId,
        ) -> PluginStatus,
    >,
    pub update: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            WidgetId,
            *const WidgetSpecV2,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId) -> PluginStatus>,
    pub submit_draw_list: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId, *const u8, u32) -> PluginStatus,
    >,
    pub request_redraw:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId) -> PluginStatus>,
    pub logical_size: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            WidgetId,
            *mut f32,
            *mut f32,
        ) -> PluginStatus,
    >,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LyricsTransformApiV2 {
    pub prefix: TablePrefix,
    pub register: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const LyricsTransformerDataV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SettingsApiV2 {
    pub prefix: TablePrefix,
    pub create: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const SettingsPageDataV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub update: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            ResourceId,
            *const SettingsPageDataV2,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TextApiV2 {
    pub prefix: TablePrefix,
    pub measure: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            Utf8Slice,
            *const TextStyleV2,
            *mut TextMetricsV2,
        ) -> PluginStatus,
    >,
    pub font_family: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, u32, *mut u8, u32, *mut u32) -> PluginStatus,
    >,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImageApiV2 {
    pub prefix: TablePrefix,
    pub decode: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, ByteSlice, *mut ImageId) -> PluginStatus,
    >,
    pub upload_rgba: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            u32,
            u32,
            ByteSlice,
            *mut ImageId,
        ) -> PluginStatus,
    >,
    pub album_art:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, *mut ImageId) -> PluginStatus>,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ImageId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StoreApiV2 {
    pub prefix: TablePrefix,
    pub get: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            Utf8Slice,
            *mut u8,
            u32,
            *mut u32,
            *mut u8,
        ) -> PluginStatus,
    >,
    pub set: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, Utf8Slice, ByteSlice) -> PluginStatus,
    >,
    pub delete: Option<unsafe extern "C" fn(*mut c_void, PluginToken, Utf8Slice) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LogApiV2 {
    pub prefix: TablePrefix,
    pub write:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, u32, Utf8Slice) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputApiV2 {
    pub prefix: TablePrefix,
    pub set_regions: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            WidgetId,
            *const InputRegionV2,
            u32,
        ) -> PluginStatus,
    >,
    pub release_capture:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CommandApiV2 {
    pub prefix: TablePrefix,
    pub register: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const CommandSpecV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub set_enabled:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId, u8) -> PluginStatus>,
    pub list: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *mut CommandInfoV2,
            u32,
            *mut u32,
        ) -> PluginStatus,
    >,
    pub execute: Option<CommandExecuteFnV2>,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AssistantApiV2 {
    pub prefix: TablePrefix,
    pub register: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const AssistantToolSpecV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub respond:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, u64, u32, Utf8Slice) -> PluginStatus>,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SurfaceApiV2 {
    pub prefix: TablePrefix,
    pub create: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const SurfaceSpecV2,
            *mut WidgetId,
        ) -> PluginStatus,
    >,
    pub update: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            WidgetId,
            *const SurfaceSpecV2,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId) -> PluginStatus>,
    pub submit_draw_list: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId, *const u8, u32) -> PluginStatus,
    >,
    pub logical_size: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            WidgetId,
            *mut f32,
            *mut f32,
        ) -> PluginStatus,
    >,
    pub show_page: Option<unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventsApiV2 {
    pub prefix: TablePrefix,
    pub subscribe: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const EventSubscriptionV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub create_timer: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *const TimerSpecV2,
            *mut ResourceId,
        ) -> PluginStatus,
    >,
    pub release: Option<unsafe extern "C" fn(*mut c_void, PluginToken, ResourceId) -> PluginStatus>,
    pub island_state:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, *mut IslandStateV2) -> PluginStatus>,
    pub set_animation:
        Option<unsafe extern "C" fn(*mut c_void, PluginToken, WidgetId, u8) -> PluginStatus>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MediaSessionApiV2 {
    pub prefix: TablePrefix,
    pub list: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            *mut MediaSessionV2,
            u32,
            *mut u32,
        ) -> PluginStatus,
    >,
    pub send: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, u64, u32, u64, *mut u64) -> PluginStatus,
    >,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SystemApiV2 {
    pub prefix: TablePrefix,
    pub local_datetime: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, *mut LocalDateTimeV2) -> PluginStatus,
    >,
    pub lunar_date: Option<
        unsafe extern "C" fn(
            *mut c_void,
            PluginToken,
            u16,
            u16,
            u16,
            *mut LunarDateV2,
            *mut u8,
        ) -> PluginStatus,
    >,
    pub current_language: Option<
        unsafe extern "C" fn(*mut c_void, PluginToken, *mut u8, u32, *mut u32) -> PluginStatus,
    >,
}
