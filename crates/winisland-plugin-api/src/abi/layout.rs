use std::mem::{align_of, offset_of, size_of};

use super::*;
use crate::types::metadata::PluginMetadataC;
use crate::types::v2::context::{ContextDataV2, HostStateV2, MediaCommandV2, MediaSourceDataV2};
use crate::types::v2::i18n::TranslationPairV2;
use crate::types::v2::lyrics::{LyricsTextV2, LyricsTransformerDataV2};
use crate::types::v2::settings::{
    SettingsChangeV2, SettingsItemV2, SettingsOptionV2, SettingsPageDataV2,
};
use crate::types::v2::widget::WidgetSpecV2;
use crate::types::v2::{
    ActivitySpecV2, AssistantToolSpecV2, ByteSlice, CommandInfoV2, CommandSpecV2,
    EventSubscriptionV2, ImageId, InputRegionV2, IslandStateV2, LocalDateTimeV2, LunarDateV2,
    MediaSessionV2, PluginEventV2, PluginToken, ResourceId, SurfaceSpecV2, TextMetricsV2,
    TextStyleV2, TimerSpecV2, Utf8Slice, WidgetId,
};

macro_rules! assert_layout {
    ($ty:ty, $size:expr, $align:expr, $($field:tt = $offset:expr),+ $(,)?) => {
        const _: () = {
            assert!(size_of::<$ty>() == $size);
            assert!(align_of::<$ty>() == $align);
            $(assert!(offset_of!($ty, $field) == $offset);)+
        };
    };
}

const _: () = {
    assert!(size_of::<PluginStatus>() == 4);
    assert!(align_of::<PluginStatus>() == 4);
};

assert_layout!(
    ActivityApiV2,
    40,
    8,
    prefix = 0,
    create = 16,
    update = 24,
    release = 32
);
assert_layout!(
    ActivitySpecV2,
    40,
    8,
    struct_size = 0,
    flags = 4,
    priority = 8,
    timeout_ms = 12,
    compact_surface = 16,
    expanded_page = 24,
    preferred_width = 32,
    preferred_height = 36
);

assert_layout!(
    SystemApiV2,
    40,
    8,
    prefix = 0,
    local_datetime = 16,
    lunar_date = 24,
    current_language = 32
);
assert_layout!(
    LocalDateTimeV2,
    20,
    4,
    struct_size = 0,
    year = 4,
    month = 6,
    day = 8,
    day_of_week = 10,
    hour = 12,
    minute = 14,
    second = 16,
    millisecond = 18
);
assert_layout!(
    LunarDateV2,
    8,
    4,
    struct_size = 0,
    month = 4,
    day = 5,
    leap = 6,
    reserved = 7
);

assert_layout!(PluginToken, 8, 8, 0 = 0);
assert_layout!(ResourceId, 8, 8, 0 = 0);
assert_layout!(WidgetId, 8, 8, 0 = 0);
assert_layout!(ImageId, 8, 8, 0 = 0);
assert_layout!(ByteSlice, 16, 8, ptr = 0, len = 8);
assert_layout!(Utf8Slice, 16, 8, ptr = 0, len = 8);
assert_layout!(
    TablePrefix,
    16,
    8,
    struct_size = 0,
    version = 4,
    context = 8
);
assert_layout!(PluginHostV2, 32, 8, prefix = 0, host_build = 16, query = 24);
assert_layout!(
    PluginCreateInfoV2,
    24,
    8,
    struct_size = 0,
    abi_version = 4,
    plugin_token = 8,
    host_api = 16
);
assert_layout!(
    PluginMetadataC,
    608,
    1,
    id = 0,
    name = 64,
    version = 192,
    author = 224,
    description = 352
);
assert_layout!(
    PluginDescriptorV2,
    656,
    8,
    struct_size = 0,
    abi_version = 4,
    capabilities = 8,
    metadata = 16,
    create = 624,
    shutdown = 632,
    destroy = 640,
    on_tick = 648
);

assert_layout!(
    ContextApiV2,
    40,
    8,
    prefix = 0,
    create = 16,
    update = 24,
    release = 32
);
assert_layout!(
    MediaApiV2,
    48,
    8,
    prefix = 0,
    create = 16,
    update = 24,
    release = 32,
    current_title = 40
);
assert_layout!(
    I18nApiV2,
    32,
    8,
    prefix = 0,
    register_bundle = 16,
    release_bundle = 24
);
assert_layout!(
    HostStateApiV2,
    40,
    8,
    prefix = 0,
    get = 16,
    subscribe = 24,
    release_subscription = 32
);
assert_layout!(
    WidgetApiV2,
    64,
    8,
    prefix = 0,
    create = 16,
    update = 24,
    release = 32,
    submit_draw_list = 40,
    request_redraw = 48,
    logical_size = 56
);
assert_layout!(
    LyricsTransformApiV2,
    32,
    8,
    prefix = 0,
    register = 16,
    release = 24
);
assert_layout!(
    SettingsApiV2,
    40,
    8,
    prefix = 0,
    create = 16,
    update = 24,
    release = 32
);
assert_layout!(TextApiV2, 32, 8, prefix = 0, measure = 16, font_family = 24);
assert_layout!(
    ImageApiV2,
    48,
    8,
    prefix = 0,
    decode = 16,
    upload_rgba = 24,
    album_art = 32,
    release = 40
);
assert_layout!(
    StoreApiV2,
    40,
    8,
    prefix = 0,
    get = 16,
    set = 24,
    delete = 32
);
assert_layout!(LogApiV2, 24, 8, prefix = 0, write = 16);
assert_layout!(
    InputApiV2,
    32,
    8,
    prefix = 0,
    set_regions = 16,
    release_capture = 24
);
assert_layout!(
    CommandApiV2,
    56,
    8,
    prefix = 0,
    register = 16,
    set_enabled = 24,
    list = 32,
    execute = 40,
    release = 48
);
assert_layout!(
    AssistantApiV2,
    40,
    8,
    prefix = 0,
    register = 16,
    respond = 24,
    release = 32
);
assert_layout!(
    SurfaceApiV2,
    64,
    8,
    prefix = 0,
    create = 16,
    update = 24,
    release = 32,
    submit_draw_list = 40,
    logical_size = 48,
    show_page = 56
);
assert_layout!(
    EventsApiV2,
    56,
    8,
    prefix = 0,
    subscribe = 16,
    create_timer = 24,
    release = 32,
    island_state = 40,
    set_animation = 48
);
assert_layout!(MediaSessionApiV2, 32, 8, prefix = 0, list = 16, send = 24);
assert_layout!(
    PluginEventV2,
    88,
    8,
    struct_size = 0,
    detail = 4,
    kind = 8,
    target = 16,
    resource = 24,
    sequence = 32,
    time_seconds = 40,
    x = 48,
    y = 52,
    delta_x = 56,
    delta_y = 60,
    modifiers = 64,
    code = 68,
    data = 72
);
assert_layout!(
    EventSubscriptionV2,
    40,
    8,
    struct_size = 0,
    reserved = 4,
    events = 8,
    target = 16,
    callback = 24,
    callback_data = 32
);
assert_layout!(
    TimerSpecV2,
    48,
    8,
    struct_size = 0,
    flags = 4,
    delay_ms = 8,
    interval_ms = 16,
    target = 24,
    callback = 32,
    callback_data = 40
);
assert_layout!(
    InputRegionV2,
    32,
    8,
    id = 0,
    x = 8,
    y = 12,
    width = 16,
    height = 20,
    flags = 24,
    reserved = 28
);
assert_layout!(
    SurfaceSpecV2,
    344,
    4,
    struct_size = 0,
    kind = 4,
    flags = 8,
    order = 12,
    width = 16,
    height = 20,
    key = 24,
    title = 88
);
assert_layout!(
    CommandSpecV2,
    352,
    8,
    struct_size = 0,
    flags = 4,
    key = 8,
    title = 72,
    hotkey_modifiers = 328,
    hotkey_key = 332,
    callback = 336,
    callback_data = 344
);
assert_layout!(
    AssistantToolSpecV2,
    616,
    8,
    struct_size = 0,
    flags = 4,
    key = 8,
    description = 72,
    schema = 584,
    callback = 600,
    callback_data = 608
);
assert_layout!(
    CommandInfoV2,
    424,
    4,
    struct_size = 0,
    flags = 4,
    id = 8,
    title = 168
);
assert_layout!(
    MediaSessionV2,
    1072,
    8,
    struct_size = 0,
    flags = 4,
    id = 8,
    available_controls = 16,
    reserved = 20,
    duration_ms = 24,
    position_ms = 32,
    sampled_at_seconds = 40,
    source = 48,
    title = 304,
    artist = 560,
    album = 816
);
assert_layout!(
    IslandStateV2,
    32,
    8,
    struct_size = 0,
    expanded = 4,
    visible = 5,
    light_theme = 6,
    reserved = 7,
    page = 8,
    width = 16,
    height = 20,
    scale = 24,
    reserved2 = 28
);

assert_layout!(
    ContextDataV2,
    912,
    4,
    struct_size = 0,
    priority = 4,
    flags = 8,
    timeout_ms = 12,
    title = 16,
    body = 272,
    compact_text = 784
);
assert_layout!(
    HostStateV2,
    560,
    4,
    struct_size = 0,
    flags = 4,
    media_title = 8,
    media_artist = 264,
    is_playing = 520,
    reserved = 521,
    theme = 528
);
assert_layout!(
    MediaCommandV2,
    16,
    8,
    struct_size = 0,
    command = 4,
    position_ms = 8
);
assert_layout!(
    MediaSourceDataV2,
    832,
    8,
    struct_size = 0,
    flags = 4,
    duration_ms = 8,
    position_ms = 16,
    available_controls = 24,
    reserved = 28,
    title = 32,
    artist = 288,
    album = 544,
    cover = 800,
    on_command = 816,
    callback_data = 824
);
assert_layout!(TranslationPairV2, 32, 8, key = 0, value = 16);
assert_layout!(
    LyricsTextV2,
    32,
    8,
    struct_size = 0,
    flags = 4,
    line_time_ms = 8,
    text = 16
);
assert_layout!(
    LyricsTransformerDataV2,
    24,
    8,
    struct_size = 0,
    flags = 4,
    on_transform = 8,
    callback_data = 16
);
assert_layout!(
    SettingsOptionV2,
    388,
    4,
    struct_size = 0,
    value = 4,
    label = 132
);
assert_layout!(
    SettingsItemV2,
    632,
    8,
    struct_size = 0,
    kind = 4,
    flags = 8,
    key = 12,
    label = 76,
    value = 332,
    options = 592,
    option_count = 600,
    minimum = 608,
    maximum = 616,
    step = 624
);
assert_layout!(
    SettingsChangeV2,
    324,
    4,
    struct_size = 0,
    key = 4,
    value = 68
);
assert_layout!(
    SettingsPageDataV2,
    248,
    8,
    struct_size = 0,
    key = 4,
    title = 68,
    icon = 200,
    items = 216,
    item_count = 224,
    on_change = 232,
    callback_data = 240
);
assert_layout!(
    WidgetSpecV2,
    856,
    4,
    struct_size = 0,
    span_cols = 4,
    span_rows = 8,
    flags = 12,
    title = 16,
    body = 272,
    key = 784,
    min_width = 848,
    min_height = 852
);
assert_layout!(
    TextStyleV2,
    24,
    8,
    size = 0,
    weight = 4,
    italic = 6,
    reserved = 7,
    family = 8
);
assert_layout!(
    TextMetricsV2,
    16,
    4,
    width = 0,
    height = 4,
    ascent = 8,
    descent = 12
);
