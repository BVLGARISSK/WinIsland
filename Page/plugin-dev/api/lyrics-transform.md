# Lyrics Transform API

`LyricsTransformApiV2` registers a callback that changes parsed lyric text before display. Declare `CAP_LYRICS` and query `IFACE_LYRICS_TRANSFORM`. The SDK `host.lyrics()?.register(|line| ...)` manages callback storage for a text-to-text closure.

## A typical use

Register one transformer during `create`, retain its `Resource`, and drop it during `shutdown`. For example, a plugin can replace a known spelling in displayed lines:

```rust
let transform = host.lyrics()?.register(|line| line.replace("colour", "Colour"))?;
// Keep `transform` in the plugin instance.
```

This example preserves the number of Unicode characters, which matters for word-synchronised lyrics. The SDK closure receives only text, so use count-preserving changes for every line. The raw callback receives `LYRICS_TEXT_FLAG_WORD_SYNCED` if you need to treat synchronised lines differently.

## Methods

Both methods take `context, token` first and return `PluginStatus`.

| Method | Remaining parameters | Result |
|---|---|---|
| `register` | `*const LyricsTransformerDataV2`, `*mut ResourceId` | Registers a transformer and writes its ID. |
| `release` | `ResourceId` | Removes it when no callback is in flight. |

## Transformation callback

`LyricsTransformerDataV2` requires `on_transform`; `flags` is zero or `LYRICS_TRANSFORMER_FLAG_SECONDARY`. The callback receives a `LyricsTextV2` containing the line timestamp, flags, and borrowed UTF-8 text. It is called twice for each line: first with null output and zero capacity to ask for the required byte length, then with a writable buffer. Write `out_len` in both passes and return `PluginStatus::Ok` on success. Keep the output stable between passes.

When `LYRICS_TEXT_FLAG_WORD_SYNCED` is set, preserve the same Unicode character count to keep word timing boundaries. The input text is valid only during the callback. Keep `callback_data` valid until release succeeds; releasing during an active callback returns `LimitExceeded`. The current quota is four transformers per plugin.

## Secondary text

Register with `LYRICS_TRANSFORMER_FLAG_SECONDARY`, or call `host.lyrics()?.register_secondary(|line| ...)`, to supply text shown below each line, such as a translation, instead of replacing the line. The host calls secondary transformers after the original lines are displayed, one line at a time in song order, and waits up to 180 seconds for each call, so a transformer can process the whole song on its first line and answer later lines from a cache. Return an empty string to leave a line without secondary text; word timing is unaffected. The host stops asking once the track changes or the user turns off secondary lyrics.

`LyricsTextV2` also carries `line_index`, `line_count`, and `song`, every line joined by `
`, for both kinds of transformer. Check `struct_size` before reading these fields on an older host. When a secondary transformer supplies any text for a song, provider translations are not shown for that song.

See the [lyrics example](https://github.com/WinIslandProject/WinIsland/blob/master/crates/winisland-plugin-api/examples/lyrics_transform.rs) and [callback type](https://github.com/WinIslandProject/WinIsland/blob/master/crates/winisland-plugin-api/src/types/v2/lyrics.rs).

[All plugin APIs](/plugin-dev/api)
