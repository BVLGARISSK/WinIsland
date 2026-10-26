use std::ffi::c_void;

use super::{ByteSlice, ResourceId, Utf8Slice, WidgetId};

pub const EVENT_INPUT: u64 = 1 << 0;
pub const EVENT_HOST: u64 = 1 << 1;
pub const EVENT_MEDIA: u64 = 1 << 2;
pub const EVENT_VISIBILITY: u64 = 1 << 3;
pub const EVENT_RESIZE: u64 = 1 << 4;
pub const EVENT_TIMER: u64 = 1 << 5;
pub const EVENT_COMMAND: u64 = 1 << 6;
pub const EVENT_RESULT: u64 = 1 << 7;
pub const EVENT_ALL: u64 = (1 << 8) - 1;
/// Delivered only to an assistant tool when the assistant calls it: `sequence` identifies the
/// call and `data` holds the JSON arguments. Answer with `AssistantApiV2::respond`.
pub const EVENT_ASSISTANT: u64 = 1 << 8;

pub const INPUT_MOVE: u32 = 1;
pub const INPUT_DOWN: u32 = 2;
pub const INPUT_UP: u32 = 3;
pub const INPUT_ENTER: u32 = 4;
pub const INPUT_LEAVE: u32 = 5;
pub const INPUT_WHEEL: u32 = 6;
pub const INPUT_KEY_DOWN: u32 = 7;
pub const INPUT_KEY_UP: u32 = 8;
pub const INPUT_TEXT: u32 = 9;
pub const INPUT_FOCUS: u32 = 10;
pub const INPUT_BLUR: u32 = 11;
pub const INPUT_CANCEL: u32 = 12;
pub const INPUT_DROP: u32 = 13;
pub const INPUT_COMPOSITION: u32 = 14;
pub const INPUT_POINTER: u32 = 1 << 0;
pub const INPUT_SCROLL: u32 = 1 << 1;
pub const INPUT_KEYBOARD: u32 = 1 << 2;
pub const INPUT_FILES: u32 = 1 << 3;
pub const INPUT_CAPTURE_ON_PRESS: u32 = 1 << 4;
/// Marks a scroll region whose content cannot scroll further toward its start. A fresh wheel
/// gesture in that direction then switches pages instead of reaching the plugin.
pub const INPUT_SCROLL_AT_START: u32 = 1 << 5;
/// Marks a scroll region whose content cannot scroll further toward its end.
pub const INPUT_SCROLL_AT_END: u32 = 1 << 6;
pub const INPUT_FLAGS: u32 = (1 << 7) - 1;
pub const MOD_CONTROL: u32 = 1;
pub const MOD_ALT: u32 = 2;
pub const MOD_SHIFT: u32 = 4;
pub const MOD_SUPER: u32 = 8;
pub const KEY_BACKSPACE: u32 = 8;
pub const KEY_TAB: u32 = 9;
pub const KEY_ENTER: u32 = 13;
pub const KEY_ESCAPE: u32 = 27;
pub const KEY_LEFT: u32 = 0x25;
pub const KEY_UP: u32 = 0x26;
pub const KEY_RIGHT: u32 = 0x27;
pub const KEY_DOWN: u32 = 0x28;
pub const KEY_DELETE: u32 = 0x2e;

pub const SURFACE_PAGE: u32 = 1;
pub const SURFACE_COMPACT_LEFT: u32 = 2;
pub const SURFACE_COMPACT_RIGHT: u32 = 3;
pub const SURFACE_BACKGROUND: u32 = 4;
pub const SURFACE_FOREGROUND: u32 = 5;
pub const SURFACE_COMPACT_MAIN: u32 = 6;
/// A round status indicator beside the compact island, shown while enabled. Clicking it opens
/// the plugin's first page.
pub const SURFACE_STATUS: u32 = 7;
pub const SURFACE_ENABLED: u32 = 1;
pub const TIMER_VISIBLE_ONLY: u32 = 1;
pub const COMMAND_ENABLED: u32 = 1;
pub const COMMAND_TRAY: u32 = 2;
pub const ASSISTANT_TOOL_ENABLED: u32 = 1;
pub const ASSISTANT_RESULT_OK: u32 = 0;
pub const ASSISTANT_RESULT_ERROR: u32 = 1;
pub const SESSION_PLAYING: u32 = 1;
pub const SESSION_CURRENT: u32 = 2;
pub const SESSION_PLUGIN: u32 = 4;
pub const SESSION_TOGGLE: u32 = 1;
pub const SESSION_PREVIOUS: u32 = 2;
pub const SESSION_NEXT: u32 = 3;
pub const SESSION_SEEK: u32 = 4;
pub const SESSION_SELECT: u32 = 5;
pub const SESSION_PLAY: u32 = 6;
pub const SESSION_PAUSE: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginEventV2 {
    pub struct_size: u32,
    pub detail: u32,
    pub kind: u64,
    pub target: WidgetId,
    pub resource: ResourceId,
    pub sequence: u64,
    pub time_seconds: f64,
    pub x: f32,
    pub y: f32,
    pub delta_x: f32,
    pub delta_y: f32,
    pub modifiers: u32,
    pub code: u32,
    pub data: ByteSlice,
}

pub type PluginEventFnV2 = unsafe extern "C" fn(*mut c_void, *const PluginEventV2);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventSubscriptionV2 {
    pub struct_size: u32,
    pub reserved: u32,
    pub events: u64,
    pub target: WidgetId,
    pub callback: Option<PluginEventFnV2>,
    pub callback_data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TimerSpecV2 {
    pub struct_size: u32,
    pub flags: u32,
    pub delay_ms: u64,
    pub interval_ms: u64,
    pub target: WidgetId,
    pub callback: Option<PluginEventFnV2>,
    pub callback_data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InputRegionV2 {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SurfaceSpecV2 {
    pub struct_size: u32,
    pub kind: u32,
    pub flags: u32,
    pub order: i32,
    pub width: f32,
    pub height: f32,
    pub key: [u8; 64],
    pub title: [u8; 256],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CommandSpecV2 {
    pub struct_size: u32,
    pub flags: u32,
    pub key: [u8; 64],
    pub title: [u8; 256],
    pub hotkey_modifiers: u32,
    pub hotkey_key: u32,
    pub callback: Option<PluginEventFnV2>,
    pub callback_data: *mut c_void,
}

/// A tool the voice assistant may call. `key` is unique within the plugin, `description` tells the
/// model what the tool does, and `schema` is a JSON Schema object for the arguments.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AssistantToolSpecV2 {
    pub struct_size: u32,
    pub flags: u32,
    pub key: [u8; 64],
    pub description: [u8; 512],
    pub schema: Utf8Slice,
    pub callback: Option<PluginEventFnV2>,
    pub callback_data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CommandInfoV2 {
    pub struct_size: u32,
    pub flags: u32,
    pub id: [u8; 160],
    pub title: [u8; 256],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct MediaSessionV2 {
    pub struct_size: u32,
    pub flags: u32,
    pub id: u64,
    pub available_controls: u32,
    pub reserved: u32,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub sampled_at_seconds: f64,
    pub source: [u8; 256],
    pub title: [u8; 256],
    pub artist: [u8; 256],
    pub album: [u8; 256],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct IslandStateV2 {
    pub struct_size: u32,
    pub expanded: u8,
    pub visible: u8,
    pub light_theme: u8,
    pub reserved: u8,
    pub page: u64,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub reserved2: u32,
}

macro_rules! sized_default {
    ($($name:ty),+ $(,)?) => {$(
        impl Default for $name {
            fn default() -> Self {
                // SAFETY: These C structs contain only numbers, nullable pointers and optional callbacks.
                let mut value: Self = unsafe { std::mem::zeroed() };
                value.struct_size = std::mem::size_of::<Self>() as u32;
                value
            }
        }
    )+};
}

sized_default!(
    PluginEventV2,
    EventSubscriptionV2,
    TimerSpecV2,
    SurfaceSpecV2,
    CommandSpecV2,
    CommandInfoV2,
    AssistantToolSpecV2,
    MediaSessionV2,
    IslandStateV2
);

pub type CommandExecuteFnV2 = unsafe extern "C" fn(
    *mut c_void,
    super::PluginToken,
    Utf8Slice,
    ByteSlice,
    *mut u64,
) -> crate::abi::PluginStatus;
