# Surface API

`SurfaceApiV2` adds an expanded page, compact content, or an island drawing layer. It was added in crate `0.9.0`; declare `CAP_SURFACE` and query `IFACE_SURFACE`. The SDK entry point is `host.surfaces()?`. For a grid-managed tile, use [Widget](/plugin-dev/api/widget).

## Create a page

This helper requires `CAP_SURFACE | CAP_EVENTS`. Call it with the `Host` received during plugin creation and keep the returned `Surface` in the plugin instance. It submits an initial frame, disables continuous ticks, and asks the host to open the page.

```rust
use winisland_plugin_api::sdk::{DrawListBuilder, Error, Host, Rect, Rgba, Size, Surface};
use winisland_plugin_api::*;

fn create_panel(host: &Host) -> Result<Surface, Error> {
    let page = host.surfaces()?.create(SurfaceSpecV2 {
        key: str_to_fixed("panel"),
        title: str_to_fixed("My panel"),
        kind: SURFACE_PAGE,
        flags: SURFACE_ENABLED,
        width: 360.0,
        height: 200.0,
        ..Default::default()
    })?;
    host.events()?.set_animation(page.id(), false)?;
    let (width, height) = page.logical_size();
    if width > 0.0 && height > 0.0 {
        let mut frame = DrawListBuilder::new(Size::new(width, height));
        frame.fill_round_rect(
            Rect::new(0.0, 0.0, width, height),
            12.0,
            Rgba::from_argb(0xff20_2838),
        );
        page.submit(frame.finish())?;
    }
    page.show()?;
    Ok(page)
}
```

Subscribe to `EVENT_RESIZE` for this page and rebuild its drawing and [input regions](/plugin-dev/api/input) at the new logical size. `show()` queues a request; it does not wait for the expansion animation. Only enabled `SURFACE_PAGE` resources can be shown this way.

## Placement

| Kind | Placement and size |
|---|---|
| `SURFACE_PAGE` | A separate expanded page. Its displayed logical size follows the host's configured expanded dimensions. |
| `SURFACE_COMPACT_LEFT` / `SURFACE_COMPACT_RIGHT` | Content at the corresponding compact edge. The host clamps requested width to 24–256 logical units and supplies the current compact height. |
| `SURFACE_BACKGROUND` | A drawing layer above the host background and below island content. |
| `SURFACE_FOREGROUND` | A drawing layer above island content, clipped to the island. |
| `SURFACE_COMPACT_MAIN` | Since `0.11.0`: main compact content selected through [Activity](/plugin-dev/api/activity); read `logical_size()` for the presented content size. |
| `SURFACE_STATUS` | Since `0.12.0`: a round status indicator beside the compact island. See [Status indicator](#status-indicator). |

The host sorts other surfaces by `order`, then by resource ID; the main compact region is selected by Activity priority and update time. Dimensions in `SurfaceSpecV2` must be finite and in 0–2048; they initialize the resource, but are not a guarantee of its eventual layout. Read `logical_size()` after presentation. Layers cover the current island bounds and do not create separate native windows.

## Page order

Since `0.12.0`, enabled `SURFACE_PAGE` surfaces appear in **Settings → Widgets → Pages** next to the built-in pages, where users can reorder and hide them. The host remembers each page by plugin ID and surface key, so keep the key stable across versions; the surface title is the name shown in the list. A new page starts visible at the end of the order. A hidden page stays registered, but the host skips it in the pager and `show_page` cannot open it.

## Status indicator

Since `0.12.0`, a `SURFACE_STATUS` surface draws a small round indicator beside the compact island, in the same place as the timer's status orb. Use it for state that should stay visible while the island is compact, such as an unread count or an urgent task.

- Enable it with `SURFACE_ENABLED` while there is something to show, and clear the flag to hide it. The host animates it in and out.
- The host presents it as a square sized to the compact island's height. Read `logical_size()` and draw within the circle that fits; the corners are clipped.
- If a running timer also needs the orb, the timer takes it and the status surface waits.
- Clicking the indicator expands the island and opens the plugin's first enabled page, unless the user has hidden that page. Input regions are not used.
- It receives `on_tick` only while it is visible, like other surfaces.

```rust
let status = host.surfaces()?.create(SurfaceSpecV2 {
    key: str_to_fixed("urgent"),
    title: str_to_fixed("Urgent reminders"),
    kind: SURFACE_STATUS,
    flags: 0,
    width: 32.0,
    height: 32.0,
    ..Default::default()
})?;
```

Older hosts reject `SURFACE_STATUS` with `InvalidArgument`; treat the indicator as optional.

## Methods and ownership

Raw calls take `context, token` first and return `PluginStatus`.

| Method | Remaining parameters | Behavior |
|---|---|---|
| `create` | `*const SurfaceSpecV2`, `*mut WidgetId` | Creates a surface owned by the caller. |
| `update` | `WidgetId`, `*const SurfaceSpecV2` | Changes title, size, order, or enabled state; key and kind must stay the same. |
| `release` | `WidgetId` | Removes the surface and its drawing/input presentation. |
| `submit_draw_list` | `WidgetId`, byte pointer, byte length | Copies a complete frame for host validation and replay. |
| `logical_size` | `WidgetId`, width output, height output | Reads the current logical drawing dimensions. |
| `show_page` | `WidgetId` | Queues navigation to an enabled plugin page. |

Keys are unique among a plugin's surfaces: 1–63 ASCII letters, digits, `_`, `-`, or `.`. Fixed strings are NUL-terminated UTF-8; titles hold at most 255 bytes. Use `..Default::default()` to initialize structure sizes. `SURFACE_ENABLED` is the only flag; clearing it hides the surface without releasing its handle.

`Surface` exposes `Widget` drawing methods through `Deref`; keeping a `Surface` requires only `CAP_SURFACE`, not an additional `CAP_WIDGET`. Surfaces and grid widgets share the quota of eight resources and 4 MiB of stored draw lists per plugin. Each list also has the normal [drawing limits](/plugin-dev/api/widget).

Surfaces receive `on_tick` only while presented and animation is enabled; animation defaults to enabled. For static content, disable it and submit when data changes. Input is configured separately through [Input](/plugin-dev/api/input) and delivered through [Events](/plugin-dev/api/events). Cancel subscriptions and timers before dropping their target; drop all handles while the host is alive.

[All plugin APIs](/plugin-dev/api)
