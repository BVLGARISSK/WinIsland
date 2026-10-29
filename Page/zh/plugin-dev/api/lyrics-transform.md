# Lyrics Transform API

`LyricsTransformApiV2` 注册显示前处理已解析歌词的回调。声明 `CAP_LYRICS` 并查询 `IFACE_LYRICS_TRANSFORM`。SDK `host.lyrics()?.register(|line| ...)` 会管理文字转换闭包的回调存储。

## 一个常见用法

在 `create` 中注册一次转换器，把返回的 `Resource` 存在实例里，`shutdown` 时释放。例如把已知拼写替换成另一种写法：

```rust
let transform = host.lyrics()?.register(|line| line.replace("colour", "Colour"))?;
// 把 transform 保存在插件实例中。
```

这个例子保留了 Unicode 字符数，对逐词同步歌词很重要。SDK 闭包只收到文字，因此最好让所有行的替换都保持字符数不变。若需要区别处理逐词同步行，可用原始回调读取 `LYRICS_TEXT_FLAG_WORD_SYNCED`。

## 方法

两种方法都先接收 `context, token`，并返回 `PluginStatus`。

| 方法 | 其余参数 | 作用 |
|---|---|---|
| `register` | `*const LyricsTransformerDataV2`、`*mut ResourceId` | 注册转换器并写入 ID。 |
| `release` | `ResourceId` | 没有进行中的回调时移除转换器。 |

## 转换回调

`LyricsTransformerDataV2` 必须提供 `on_transform`，`flags` 为零或 `LYRICS_TRANSFORMER_FLAG_SECONDARY`。回调收到 `LyricsTextV2`，包含行时间戳、标记和借用的 UTF-8 文本。每行调用两次：第一次输出指针为空、容量为零，用于查询所需字节数；第二次提供可写缓冲区。两次都要写入 `out_len`，成功时返回 `PluginStatus::Ok`，并保持两次输出一致。

设置 `LYRICS_TEXT_FLAG_WORD_SYNCED` 时，应保留相同的 Unicode 字符数，以维持逐词时间边界。输入文字只在本次回调期间有效。保持 `callback_data` 有效，直到释放成功；回调进行期间释放返回 `LimitExceeded`。每个插件当前最多有 4 个转换器。

## 附加文字

注册时设置 `LYRICS_TRANSFORMER_FLAG_SECONDARY`，或调用 `host.lyrics()?.register_secondary(|line| ...)`，回调的输出会显示在每行下方（例如译文），而不是替换该行。宿主仅在开启“显示附加歌词”时调用附加文字转换器，在原文显示后按歌曲顺序逐行调用。每次最多等待 180 秒，因此转换器可以在第一行处理整首歌，之后各行直接从缓存返回；宿主约每秒显示一次已完成的行。返回空字符串表示该行没有附加文字；逐词时间不受影响。切歌后宿主不再继续调用，该歌曲已排队的请求会直接跳过，不会调用插件。

同一插件的所有回调都在它的工作线程上逐个执行，因此附加文字调用进行时，该插件的组件 tick、设置变更和媒体事件都要等它完成。工作线程忙碌时，宿主会跳过该插件的主转换器，设置变更仍会在 2 秒后超时。单次调用超过 180 秒后，宿主会跳过该插件，直到该调用返回。停用、更新或卸载插件时，宿主最多等待正在执行的调用 2 秒，超时则操作失败、插件保持加载，所以每次调用应尽量简短。

`LyricsTextV2` 另外提供 `line_index`、`line_count` 和 `song`（所有行以换行符 `\n` 连接），两种转换器都会收到。在旧版宿主上读取这些字段前，请先检查 `struct_size`。某首歌只要有附加文字转换器提供了文字，该歌曲就不显示歌词来源附带的翻译；未注册附加文字转换器时，歌词来源的翻译照常显示。

参见[歌词示例](https://github.com/WinIslandProject/WinIsland/blob/master/crates/winisland-plugin-api/examples/lyrics_transform.rs)和[回调类型](https://github.com/WinIslandProject/WinIsland/blob/master/crates/winisland-plugin-api/src/types/v2/lyrics.rs)。

[返回 API 目录](/plugin-dev/api)
