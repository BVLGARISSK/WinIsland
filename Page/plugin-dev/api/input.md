# Input API

`InputApiV2` defines hit regions on a plugin-owned widget or surface. It was added in crate `0.9.0`; declare `CAP_INPUT` and query `IFACE_INPUT`. Receiving callbacks also needs `CAP_EVENTS` and an [Events](/plugin-dev/api/events) subscription. Drawing a button alone does not make it interactive.

## Bind a button

The target must already exist. Pass its current logical width and height, and keep the returned subscription in the plugin instance. The target itself is owned separately.

```rust
use winisland_plugin_api::sdk::{CallbackResource, Error, Host};
use winisland_plugin_api::*;

fn bind_button(
    host: &Host,
    target: WidgetId,
    width: f32,
    height: f32,
) -> Result<CallbackResource, Error> {
    let log = host.log();
    let subscription = host.events()?.subscribe(EVENT_INPUT, target, move |event| {
        if event.detail == INPUT_UP && event.sequence == 1 {
            log.write(2, "Button released");
        }
    })?;
    host.input()?.set_regions(target, &[InputRegionV2 {
        id: 1,
        x: 0.0,
        y: 0.0,
        width,
        height,
        flags: INPUT_POINTER | INPUT_CAPTURE_ON_PRESS,
        ..Default::default()
    }])?;
    Ok(subscription)
}
```

This example reports releases, including releases outside a captured button. For click activation, track press/cancel state and decide whether release coordinates are inside the button.

## Regions and methods

| SDK method | Raw method and remaining parameters | Behavior |
|---|---|---|
| `set_regions(target, regions)` | `set_regions(WidgetId, *const InputRegionV2, count)` | Copies and replaces the complete region list. An empty list removes it. |
| `release_capture(target)` | `release_capture(WidgetId)` | Releases this target's pointer capture and emits `INPUT_CANCEL` when capture existed. It does not clear keyboard focus. |

Raw calls take `context, token` and return `PluginStatus`. Each target supports at most 128 regions. Region IDs must be unique within the target; coordinates must be finite, width/height positive, and `reserved` zero. Rebuild regions when layout changes. Coordinates use the target's logical drawing space, not screen pixels; the host maps rendered positions into that space.

| Flag | Receives or enables |
|---|---|
| `INPUT_POINTER` | Pointer hit testing, down/up, move, enter/leave |
| `INPUT_SCROLL` | Wheel events over the region |
| `INPUT_KEYBOARD` | Keyboard focus when clicked; combine with `INPUT_POINTER` |
| `INPUT_FILES` | Dropped file paths over the region |
| `INPUT_CAPTURE_ON_PRESS` | Capture from a pointer press until its matching release or cancellation; combine with `INPUT_POINTER` |
| `INPUT_SCROLL_AT_START` | Since `0.12.0`: on an `INPUT_SCROLL` region, the content cannot scroll further toward its start (up) |
| `INPUT_SCROLL_AT_END` | Since `0.12.0`: on an `INPUT_SCROLL` region, the content cannot scroll further toward its end (down) |

Hit testing uses the last matching region in the topmost interactive presentation. Regions without an `EVENT_INPUT` subscription do not claim input. Layers are therefore passive until regions and a subscription are supplied. Hidden or transitioning noninteractive targets lose capture and focus. Touch is routed as a single pointer, not a multitouch API.

## Scrolling inside a page

An `INPUT_SCROLL` region normally keeps every wheel event over it, so the user cannot switch pages with the wheel while the pointer is over a scrollable list. Since `0.12.0`, report the list's scroll edges with `INPUT_SCROLL_AT_START` and `INPUT_SCROLL_AT_END`, and the host shares the wheel with the pager:

- While the content can still scroll in the wheel's direction, the plugin receives the event.
- When a continuous wheel gesture reaches an edge, the remaining events still go to the plugin and the page stays put.
- A new gesture toward an edge the content has already reached switches pages. Events more than 350 ms apart start a new gesture.
- Horizontal wheel input over the region always switches pages.

Set both flags when the content fits without scrolling. Clear them as soon as the content can move again, and set them on every region that carries `INPUT_SCROLL`. Update the flags together with the scroll position, preferably from the target position of a smooth scroll rather than the animated one:

```rust
fn scroll_flags(scroll: f32, max_scroll: f32) -> u32 {
    let mut flags = INPUT_SCROLL;
    if scroll <= 0.5 {
        flags |= INPUT_SCROLL_AT_START;
    }
    if scroll >= max_scroll - 0.5 {
        flags |= INPUT_SCROLL_AT_END;
    }
    flags
}
```

Hosts older than `0.12.0` reject unknown flags with `InvalidArgument`. To support them, retry `set_regions` with the flags masked to `(1 << 5) - 1`; the region then keeps every wheel event as before.

## Event payload

Callbacks receive `kind = EVENT_INPUT`. `target` identifies the surface/widget, `sequence` is the **region ID**, and `resource` is the subscription ID. SDK `position` corresponds to raw `x, y`; SDK `delta` corresponds to raw `delta_x, delta_y`.

| `detail` | Payload |
|---|---|
| `INPUT_DOWN`, `INPUT_UP` | `code`: 1 left/touch, 2 right, 3 other button; logical position |
| `INPUT_MOVE`, `INPUT_ENTER`, `INPUT_LEAVE` | Logical pointer position |
| `INPUT_WHEEL` | Delta pair; `code = 0` for lines, `1` for pixels |
| `INPUT_KEY_DOWN`, `INPUT_KEY_UP` | `KEY_*` or character code; `delta_x = 1` for repeat; character keys also carry UTF-8 `data` |
| `INPUT_TEXT` | Committed UTF-8 text, including IME commits |
| `INPUT_COMPOSITION` | UTF-8 preedit text; delta pair contains cursor range offsets, or `(-1, -1)` when absent |
| `INPUT_DROP` | UTF-8 file path, not file contents |
| `INPUT_FOCUS`, `INPUT_BLUR`, `INPUT_CANCEL` | Update focus/drag state; synthetic events need not contain meaningful pointer coordinates |

Use `INPUT_TEXT` for inserted text; inserting character key data too would duplicate text. Keep IME composition separate until commit. `modifiers` uses `MOD_CONTROL`, `MOD_ALT`, `MOD_SHIFT`, and `MOD_SUPER`. Input is confined to the plugin's presented regions; this API is not a global keyboard hook. Callbacks run on the plugin worker and cannot synchronously override the host's routing decision.

[All plugin APIs](/plugin-dev/api)
