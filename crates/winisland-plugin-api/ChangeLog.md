# Changelog

This changelog lists `winisland-plugin-api` versions. The website displays this file as its plugin API update log.

## Unreleased

Added:

- `LYRICS_TRANSFORMER_FLAG_SECONDARY` for lyric transformers whose output is shown below each line, such as a translation, instead of replacing it; the host runs them after the original lines are displayed
- `LyricsTextV2::{line_index, line_count, song}` so a transformer can process the whole song at once
- Rust SDK `LyricsApi::register_secondary` and `LyricsLine`

Changed:

- `LyricsTextV2` grows from 32 to 56 bytes; the existing fields keep their offsets, and callbacks built against the previous layout read only the prefix they know

## 0.11.0 - Oct 3, 2026

Added:

- `ActivityApiV2`, `IFACE_ACTIVITY`, and `CAP_ACTIVITY` to create, update, and release activities with priority, timeout, preferred compact dimensions, an owned compact surface, and an optional expanded page
- `ActivitySpecV2`, `ACTIVITY_ENABLED`, and `ACTIVITY_KEEP_VISIBLE`; active activities can keep the island visible during inactivity auto-hide
- `SURFACE_COMPACT_MAIN` for activity-owned content in the main compact region, selected by the host alongside music, timers, and legacy text contexts
- Rust SDK `Host::activities()`, `Activities::create`, and owned `Activity::{id, update}` handles that release their resources on drop, plus a download-progress example
- Compile-time ABI layout assertions and layout-tool output for the activity table and specification
- English and Chinese website references for System, Activity, and the drawing SDK, including the previously undocumented drawing helpers and commands

Changed:

- The host selects native and plugin activities together by priority, update time, and resource ID; legacy Context resources enter the same selection mechanism
- Activity selection, preferred sizing, click-to-open navigation, and inactivity auto-hide share activity declarations; expired activities and unavailable compact surfaces stop participating without freeing the activity handle
- Kept ABI v2 and all existing entry-point, descriptor, service-table, and drawing-protocol layouts; the new activity capability and surface kind require a host implementing these additions

## 0.10.0 - Oct 2, 2026

Added:

- `SystemApiV2`, `IFACE_SYSTEM`, and `CAP_SYSTEM` for local date/time, Gregorian-to-Chinese-lunar conversion, and the current WinIsland UI language
- `LocalDateTimeV2` and `LunarDateV2` ABI value types, with compile-time layout assertions for the types and service table
- Rust SDK `Host::system()` and `SystemApi::{local_datetime, lunar_date, current_language}`; lunar conversion returns `None` when no result is available and reports invalid dates or service failures as errors
- `Rgba::with_alpha(alpha)` in the Rust SDK to replace a color's alpha while preserving its RGB channels

Changed:

- Kept ABI v2 and all existing entry-point, descriptor, service-table, and drawing-protocol layouts; the new system capability requires a host that implements `IFACE_SYSTEM`
- The built-in Calendar now uses the public SDK from an independent `winisland-plugin-calendar` crate, depending only on `winisland-plugin-api`

## 0.9.1 - Oct 2, 2026

Added:

- `TextApi::measure_style(text, &TextStyle)` in the Rust SDK for measuring text with a matching size, weight, italic style, and font family

Changed:

- `TextApi::measure` delegates to the styled helper while retaining its weight-400, upright behavior
- No changes to the ABI v2 entry point, descriptor, service tables, or drawing protocol; styled measurement uses the existing `TextApiV2::measure` function

## 0.9.0 - Oct 1, 2026

Added:

- `SurfaceApiV2` and `CAP_SURFACE` for independent expanded pages, compact left/right content, and background/foreground drawing layers
- `InputApiV2` and `CAP_INPUT` for hit regions, pointer capture, scrolling, keyboard focus, text/IME input, and file-drop events on plugin-owned widgets and surfaces
- `EventsApiV2` and `CAP_EVENTS` for worker-thread event subscriptions, timers, island-state snapshots, visibility/resize notifications, asynchronous results, and animation scheduling
- `CommandApiV2` and `CAP_COMMAND` for registering and executing plugin commands, invoking built-in commands, and adding optional tray entries and global hotkeys
- `MediaSessionApiV2` and `CAP_MEDIA_SESSION` for enumerating native SMTC sessions and plugin media sources, selecting a session, and controlling playback and seeking
- Rust SDK wrappers for surfaces, input, events, commands, and media sessions, with owned callback resources for subscriptions, timers, and commands
- Compile-time ABI layout assertions for the new service tables and extension types

Changed:

- Kept ABI v2 and all existing entry-point, descriptor, and service-table layouts; the new capabilities require a host that implements their interfaces

## 0.8.0 - Sep 27, 2026

Changed:

- **Breaking**: replaced the native ABI v1 entry with `winisland_plugin_entry_v2` and `PluginDescriptorV2`; ABI v1 DLLs must be rebuilt and packaged with `abi-version: 2`
- Replaced `PluginResultC` with `PluginStatus` and moved optional `on_tick` to the plugin descriptor
- Replaced render-thread widget callbacks with complete draw lists submitted from plugin workers, validated by the host, and replayed without entering plugin code
- Updated `PluginPackager` to emit ABI v2 manifests and validate the built DLL descriptor before producing a ZIP
- Declared `winisland-plugin-api` and its publishable `winisland-plugin-package` dependency as GPL-3.0-only

Added:

- Eleven versioned host interfaces for Context, Media, i18n, Host State, Widget, Lyrics Transform, Settings, Text, Image, Store, and Log
- An opt-in Rust SDK with owned resource handles, widget draw-list builders, image helpers, persistent Store operations, and simple settings-page helpers
- Draw-list operations for clipping, transforms, alpha, shapes, gradients, strokes, shadows, images, and UTF-8 text with explicit font families
- Album-art access, rich declarative settings items, plugin-scoped persistent storage, and host-state subscriptions

Fixed:

- Bounded draw-list validation and resource ownership checks before rendering
- DLL lifetime protection when shutdown or partial-create cleanup fails

## 0.7.0 - Sep 13, 2026

Added:

- `CAPABILITY_SETTINGS` and the `SettingsApiV1` host service
- Declarative plugin settings pages with custom sidebar icons
- Section, group, label, switch, select, stepper, and button setting items
- Synchronous change callbacks with rejection errors and controlled value updates

Changed:

- Plugin settings pages are added to the settings sidebar and removed automatically during unload
- Settings resources participate in token ownership, resource limits, and callback-safe shutdown

## 0.6.0 - Aug 24, 2026

Added:

- `CAPABILITY_LYRICS_TRANSFORM` and the `LyricsTransformApiV1` host service
- `LyricsTransformerDataV1` and a two-pass UTF-8 line transformation callback
- `LyricsTextV1` with line timestamps and `LYRICS_TEXT_FLAG_WORD_SYNCED`

Changed:

- Registered transformers process parsed lyrics once after fetching, before the host caches them
- Word-synchronised lines preserve their timing boundaries and reject transformed output with a
  different Unicode character count
- Plugin unload and transformer release are rejected while a lyric callback is in progress

## 0.5.0 - Aug 14, 2026

Added:

- Stable per-plugin widget keys through `WidgetDataV1::key`
- Persistent plugin widget placement and settings layout control using those stable keys

Changed:

- Widgets with a key can be managed in the layout editor and keep their placement across restarts;
  keyless 0.4 widgets remain supported
- Widget keys must be unique within a plugin, match `[a-zA-Z0-9_-]+`, and remain unchanged
  after creation

## 0.4.1 - Aug 14, 2026

Added:

- Optional `icon` and `readme` fields in packaged plugin manifests
- `PluginPackager::icon()` and `PluginPackager::readme()` for including plugin detail assets
- Automatic detection of supported root-level icon and README files during packaging

Fixed:

- Legacy package signature verification when the optional `icon` and `readme` fields are absent

## 0.4.0 - Aug 13, 2026

Added:

- `CAPABILITY_WIDGET` and the `WidgetApiV1` host service (create, update, release)
- `WidgetDataV1` — plugin-owned widget resources with grid span and a render callback
- `WidgetDrawContextV1` / `DrawApiV1` — host-provided drawing operations so plugins can render
  on the host Skia canvas without linking any graphics library
- Draw operations: text, text measurement, rect, rounded rect, circle, line, arc, image,
  and a plugin-local `save` / `restore` / `translate` transform stack
- `WIDGET_FLAG_SHOW_COMPACT` — reserved flag for future mini-island rendering
- `HostApiV1::widget_api()` for querying the widget service during `create`

Changed:

- Widgets are rendered synchronously on the render thread in the expanded widget page;
  coordinates are logical and relative to the widget slot, and the host applies `scale`/`alpha`
- `callback_data` uses the same opaque `*mut c_void` convention as the Media interface
- The `DrawApiV1` interface is versioned and validated through `WidgetDrawContextV1::draw_api()`

## 0.3.0 - Aug 9, 2026

Added:

- Native DLL ABI v1 with the `winisland_plugin_entry_v1` descriptor entry point
- `PluginToken` and `ResourceId` identities for host-validated ownership
- Capability declarations for Context, Media, i18n, and Host State services
- Versioned host service discovery through `HostApiV1::query_interface`
- Create, update, and release operations for Context and Media resources
- Optional Media control callbacks for play/pause, previous, next, and seek commands
- Releasable translation bundles and current media/theme Host State snapshots
- `id`, `abi-version`, and single `entry` DLL fields in `plugin.yml`
- Packager support for Cargo `repository` and `[lib].name` metadata

Changed:

- **Breaking**: removed the 0.2 `PluginVTable`, `PluginType`, `PluginInstanceC`,
  `HostApiC`, `plugin_get_instance`, and `plugin_set_host_api` interfaces
- **Breaking**: removed the unfinished Theme and Shortcut interfaces
- Plugin lifecycle is now strictly `create -> shutdown -> destroy`; WinIsland unloads a DLL
  only after `shutdown` succeeds
- Context IDs are host-issued numeric resources instead of plugin-provided strings
- Plugin Media remains active independently of the SMTC setting and exposes only declared controls
- Plugin worker-thread resource changes wake the WinIsland event loop
- Development examples and packaging documentation now target Rust 2024 and ABI v1

Fixed:

- Descriptor size, ABI version, capability, metadata, and lifecycle callback validation
- Per-plugin resource ownership checks, count limits, and memory limits
- Media callback reentrancy and unload synchronization
- UTF-8-safe fixed-buffer truncation and bounded borrowed-slice copying
- Context update/release event coalescing and media seek source binding
- Bounded ZIP extraction with staging, Windows path-collision checks, transactional activation,
  backup, and explicit rollback errors
- Translation bundle cleanup and host wake coalescing during plugin shutdown

## 0.2.0 - Jun 19, 2026

Added:

- `TranslationPairC` — FFI-safe translation key-value pair for plugin i18n
- `HostApiC::register_translations` — plugin registers translations during `on_load`;
  later registrations override earlier ones for the same key
- i18n overlay layer — `tr()` checks plugin-registered translations before `.lang` files

Changed:

- **Breaking**: `HostApiC` gains a new required field `register_translations`;
  all host implementations must provide this callback
- `lib.rs` split into modular files: `host.rs`, `vtable.rs`, `types/mod.rs`,
  `types/{metadata,content,context,theme,shortcut,i18n}.rs`
- All public types re-exported from crate root — import paths unchanged for plugin authors

## 0.1.3 - Jun 19, 2026

Added:

- `MediaSourceC` — plugin-injectable media source (title, artist, album, duration, position, cover art)
- `HostApiC::set_media_source` — replace SMTC with plugin-provided media data
- `HostApiC::clear_media_source` — restore SMTC as the active media source

Changed:

- `HostApiC` derives `Clone`, `Copy` for safe FFI usage
- `PluginResultC` derives `Debug`, `Clone`, `Copy`
- `ContextDataC`, `ContextIdC`, `HostStateC` — new push-based context types
- `PluginVTable::set_host_api` — optional slot for plugin to receive `HostApiC` pointer

## 0.1.2 - Jun 17, 2026

Added:

- README.md with crate-level documentation, usage examples and feature flags

## 0.1.1 - Jun 16, 2026

Added:

- `packager` feature: `PluginPackager` for building, signing and zipping plugins
- Cargo.toml metadata for crates.io publishing (repository, homepage, license, keywords, categories)
- `docs.rs` configuration with `packager` feature enabled

Changed:

- Use `str_to_fixed` helper for byte-buffer initialization, replacing manual padding loops
- Packager validates `manifest.yaml` during `build()`; checks for missing fields and oversized buffers
- `github_link` field in `Manifest` is now required (non-empty) to satisfy host validation

Fixed:

- `plugin_get_instance` doc example uses proper `#[no_mangle]` export, no extraneous `fn main`
- Broken doc links in packager module docs
- `BG_CACHE` size check in signing flow

## 0.1.0 - Jun 15, 2026

Added:

- Initial release — C ABI types extracted from the WinIsland host into a standalone crate
- Core types: `PluginInstanceC`, `PluginVTable`, `PluginMetadataC`, `IslandContentC`, `ThemeColorsC`, `AnimationConfigC`, `ShortcutC`, `PluginResultC`
- `PluginType` enum with `from_u32` conversion
- `PluginGetInstanceFn` — entry-point signature for plugin DLLs
- `str_to_fixed` / `read_c_str` / `read_opt_c_str` helpers for FFI byte-buffer handling
- Priority constants: `PRIORITY_LOW`, `PRIORITY_MEDIUM`, `PRIORITY_HIGH`
- Content tag constants: `ISLAND_CONTENT_TAG_MUSIC`, `ISLAND_CONTENT_TAG_NOTIFICATION`, `ISLAND_CONTENT_TAG_STATUS`
