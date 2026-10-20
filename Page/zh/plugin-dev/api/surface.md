# Surface API

`SurfaceApiV2` 用来添加独立展开页、紧凑区域内容或岛内绘制层。它从 crate `0.9.0` 开始提供；声明 `CAP_SURFACE` 并查询 `IFACE_SURFACE`，SDK 入口为 `host.surfaces()?`。需要网格管理的小组件时，使用 [Widget](/plugin-dev/api/widget)。

## 创建展开页

以下函数需要 `CAP_SURFACE | CAP_EVENTS`。传入插件创建时取得的 `Host`，并把返回的 `Surface` 保存在插件实例中。函数会提交初始帧、关闭连续 tick，并请求宿主打开页面。

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

还应订阅此页面的 `EVENT_RESIZE`，在尺寸改变后按新的逻辑尺寸重建绘制和[输入区域](/plugin-dev/api/input)。`show()` 只将导航请求入队，不会等待展开动画结束；它只适用于已启用的 `SURFACE_PAGE`。

## 放在哪里

| 类型 | 位置与尺寸 |
|---|---|
| `SURFACE_PAGE` | 独立展开页；显示时的逻辑尺寸由宿主的展开尺寸配置决定。 |
| `SURFACE_COMPACT_LEFT` / `SURFACE_COMPACT_RIGHT` | 紧凑岛左侧或右侧；请求宽度会被限制为 24–256 个逻辑单位，高度跟随当前紧凑岛。 |
| `SURFACE_BACKGROUND` | 宿主背景之上、岛内容之下的绘制层。 |
| `SURFACE_FOREGROUND` | 岛内容之上的绘制层，仍被裁剪在岛内。 |
| `SURFACE_COMPACT_MAIN` | 从 `0.11.0` 起提供：主紧凑内容，通过 [Activity](/plugin-dev/api/activity) 参与选择；读取 `logical_size()` 获取实际内容尺寸。 |
| `SURFACE_STATUS` | 从 `0.12.0` 起提供：紧凑岛旁的圆形状态指示，见下文“状态指示”。 |

其他 surface 按 `order`、资源 ID 排序；主紧凑区域则按 Activity 的优先级和更新时间选择。`SurfaceSpecV2` 的宽高须为 0–2048 范围内的有限数值；它们用于初始化资源，不保证是最终布局尺寸。显示后应读取 `logical_size()`。背景和前景层覆盖当前岛的范围，不会创建独立的原生窗口。

## 页面顺序

从 `0.12.0` 起，已启用的 `SURFACE_PAGE` 会与内置页面一起出现在**设置 → 小组件 → 页面**中，用户可以调整顺序或隐藏。宿主按插件 ID 和 surface 键记住每个页面，因此不同版本应保持键不变；列表中显示的名称为 surface 标题。新页面默认显示，并排在末尾。被隐藏的页面仍保持注册，但宿主会在翻页时跳过它，`show_page` 也无法打开它。

## 状态指示

从 `0.12.0` 起，`SURFACE_STATUS` 会在紧凑岛旁绘制一个圆形小指示，位置与计时器的状态小圆球相同。适合在岛缩小时仍需看到的状态，例如未读数量或紧急任务。

- 有内容时设置 `SURFACE_ENABLED` 显示，清除该标记即隐藏；宿主负责出现和消失的动画。
- 宿主以紧凑岛高度为边长的正方形呈现它。读取 `logical_size()`，在其中的内切圆内绘制，四角会被裁掉。
- 计时器正在运行且也需要小圆球时，由计时器优先占用，状态 surface 等待。
- 点击指示会展开灵动岛并打开该插件第一个已启用的展开页（用户已隐藏该页时只展开）；它不使用输入区域。
- 与其他 surface 相同，仅在可见时收到 `on_tick`。

```rust
let status = host.surfaces()?.create(SurfaceSpecV2 {
    key: str_to_fixed("urgent"),
    title: str_to_fixed("紧急提醒"),
    kind: SURFACE_STATUS,
    flags: 0,
    width: 32.0,
    height: 32.0,
    ..Default::default()
})?;
```

旧版宿主会以 `InvalidArgument` 拒绝 `SURFACE_STATUS`，应将状态指示视为可选功能。

## 方法与所有权

原始调用均先接收 `context, token`，返回 `PluginStatus`。

| 方法 | 后续参数 | 行为 |
|---|---|---|
| `create` | `*const SurfaceSpecV2`、`*mut WidgetId` | 创建归调用插件所有的 surface。 |
| `update` | `WidgetId`、`*const SurfaceSpecV2` | 修改标题、尺寸、排序或启用状态；键和类型不能改变。 |
| `release` | `WidgetId` | 移除资源及其绘制、输入呈现。 |
| `submit_draw_list` | `WidgetId`、字节指针、字节长度 | 复制完整一帧，交给宿主校验并重放。 |
| `logical_size` | `WidgetId`、宽度输出、高度输出 | 读取当前逻辑绘制尺寸。 |
| `show_page` | `WidgetId` | 请求切换到已启用的插件展开页。 |

同一插件的 surface 键必须唯一，包含 1–63 个 ASCII 字母、数字、`_`、`-` 或 `.`。固定字符串使用 NUL 结尾的 UTF-8；标题最多 255 字节。用 `..Default::default()` 初始化结构体大小。唯一标记为 `SURFACE_ENABLED`；清除此标记会隐藏内容，但不释放句柄。

`Surface` 通过 `Deref` 提供 `Widget` 的绘制方法；使用它只需 `CAP_SURFACE`，无需再声明 `CAP_WIDGET`。Surface 与网格小组件共享每插件 8 个资源、4 MiB 已存储绘制列表的配额，单份列表还受[绘制限制](/plugin-dev/api/widget)约束。

Surface 仅在被呈现且启用动画时收到 `on_tick`，动画默认启用。静态内容应关闭动画，在数据改变时提交新帧。交互区域由 [Input](/plugin-dev/api/input) 设置，回调通过 [Events](/plugin-dev/api/events) 接收。释放目标前先取消其订阅和定时器，并在宿主仍有效时释放所有句柄。

[全部插件 API](/plugin-dev/api)
