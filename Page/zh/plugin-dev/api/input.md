# Input API

`InputApiV2` 为插件自己的小组件或 surface 设置命中区域。从 crate `0.9.0` 开始提供；声明 `CAP_INPUT` 并查询 `IFACE_INPUT`。接收回调还需要 `CAP_EVENTS` 和 [Events](/plugin-dev/api/events) 订阅。只画出按钮并不会自动获得交互能力。

## 绑定按钮

目标资源必须已经存在。传入它当前的逻辑宽高，并将返回的订阅句柄保存在插件实例中；目标资源本身需要另外保留。

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

这个示例记录释放事件，包括捕获后在按钮外释放的情况。若要实现点击激活，还需记录按下、取消状态，并判断释放坐标是否位于按钮内。

## 区域与方法

| SDK 方法 | 原始方法及后续参数 | 行为 |
|---|---|---|
| `set_regions(target, regions)` | `set_regions(WidgetId, *const InputRegionV2, count)` | 复制并替换完整区域列表；空列表会移除区域。 |
| `release_capture(target)` | `release_capture(WidgetId)` | 释放此目标的指针捕获；原先存在捕获时发送 `INPUT_CANCEL`。不会清除键盘焦点。 |

原始调用先接收 `context, token`，返回 `PluginStatus`。每个目标最多 128 个区域；区域 ID 在目标内唯一，坐标必须有限、宽高必须为正，`reserved` 为零。布局变化后重新设置区域。坐标采用目标绘制的逻辑坐标，不是屏幕像素，宿主负责将实际呈现位置映射回来。

| 标记 | 接收或启用的行为 |
|---|---|
| `INPUT_POINTER` | 指针命中、按下/释放、移动、进入/离开 |
| `INPUT_SCROLL` | 区域上方的滚轮事件 |
| `INPUT_KEYBOARD` | 点击后获得键盘焦点；应同时设置 `INPUT_POINTER` |
| `INPUT_FILES` | 拖入此区域的文件路径 |
| `INPUT_CAPTURE_ON_PRESS` | 按下后捕获指针，直到对应释放或取消；应同时设置 `INPUT_POINTER` |
| `INPUT_SCROLL_AT_START` | 从 `0.12.0` 起提供：用于 `INPUT_SCROLL` 区域，表示内容已无法再向开头（向上）滚动 |
| `INPUT_SCROLL_AT_END` | 从 `0.12.0` 起提供：用于 `INPUT_SCROLL` 区域，表示内容已无法再向末尾（向下）滚动 |

命中测试优先选择最上层可交互呈现中的最后一个匹配区域。没有 `EVENT_INPUT` 订阅的区域不会接管输入；因此，绘制层在设置区域和订阅之前不会拦截操作。目标隐藏或进入不可交互的过渡状态时，会失去捕获与焦点。触摸按单指针路由，不提供多点触控接口。

## 页面内滚动

`INPUT_SCROLL` 区域默认接管其上方的全部滚轮事件，因此指针停在可滚动列表上时，用户无法用滚轮切换页面。从 `0.12.0` 起，可用 `INPUT_SCROLL_AT_START` 和 `INPUT_SCROLL_AT_END` 报告列表的滚动边界，宿主会与翻页共享滚轮：

- 内容还能沿滚轮方向滚动时，事件交给插件。
- 一次连续滚动到达边界后，剩余事件仍交给插件，不会翻页。
- 在已到达的边界方向上开始新的一次滚动时，切换页面。相邻事件间隔超过 350 毫秒即视为新的一次滚动。
- 区域上的横向滚轮始终用于切换页面。

内容无需滚动时同时设置两个标记；内容能再次滚动时立即清除。所有带 `INPUT_SCROLL` 的区域都应设置这些标记。标记应与滚动位置同步更新；使用平滑滚动时，以目标位置而非动画中的位置为准：

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

`0.12.0` 之前的宿主会以 `InvalidArgument` 拒绝未知标记。如需兼容，可在失败后用 `(1 << 5) - 1` 屏蔽标记再调用一次 `set_regions`；此时区域会像以前一样接管全部滚轮事件。

## 事件数据

回调收到 `kind = EVENT_INPUT`。`target` 是目标资源，`sequence` 是**区域 ID**，`resource` 是订阅 ID。SDK 的 `position` 对应原始 `x, y`，`delta` 对应原始 `delta_x, delta_y`。

| `detail` | 数据 |
|---|---|
| `INPUT_DOWN`、`INPUT_UP` | `code`：1 左键/触摸，2 右键，3 其他按钮；逻辑坐标位置 |
| `INPUT_MOVE`、`INPUT_ENTER`、`INPUT_LEAVE` | 指针逻辑坐标 |
| `INPUT_WHEEL` | 滚动增量；`code = 0` 表示行，`1` 表示像素 |
| `INPUT_KEY_DOWN`、`INPUT_KEY_UP` | `KEY_*` 或字符码；`delta_x = 1` 表示重复；字符键的 `data` 还包含 UTF-8 文本 |
| `INPUT_TEXT` | 已提交的 UTF-8 文本，包括输入法提交结果 |
| `INPUT_COMPOSITION` | UTF-8 预编辑文本；delta 为光标范围偏移，无范围时为 `(-1, -1)` |
| `INPUT_DROP` | UTF-8 文件路径，不是文件内容 |
| `INPUT_FOCUS`、`INPUT_BLUR`、`INPUT_CANCEL` | 更新焦点或拖动状态；合成事件不保证携带有意义的指针坐标 |

插入文字应使用 `INPUT_TEXT`；同时插入字符按键的 `data` 会重复输入。输入法组合文本应单独保存，到提交时才写入正文。`modifiers` 使用 `MOD_CONTROL`、`MOD_ALT`、`MOD_SHIFT`、`MOD_SUPER`。输入范围限于插件已呈现的区域，不是全局键盘钩子。回调在插件工作线程执行，不能同步改变宿主已经做出的路由决定。

[全部插件 API](/plugin-dev/api)
