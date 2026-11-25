# niri-native

**本项目 fork 自 [Rift（acsandmann/rift）](https://github.com/acsandmann/rift)，克隆地址为 `https://github.com/xbunax/niri-native.git`。** niri-native 是 macOS 平铺窗口管理器：保留 Rift 的布局、快捷键和 Overview，把每个工作区直接映射到一个原生 macOS 桌面。名字里的 niri 指滚动列式布局的交互风格；本项目不是 Linux [niri](https://github.com/YaLTeR/niri) 合成器的移植。

![Rift 窗口布局演示](assets/demo.gif)

## 构建与启动

需要 macOS、Rust 工具链和 Xcode Command Line Tools。使用 SkyLight 私有接口，不需要关闭 SIP；不同 macOS 版本的接口可能变化。首次启动请按系统提示授予辅助功能权限；Overview 的窗口预览还可能需要屏幕录制权限。

```sh
git clone https://github.com/xbunax/niri-native.git
cd niri-native
cargo build --release --locked --bins
mkdir -p ~/.config/rift-native
cp -n rift.default.toml ~/.config/rift-native/config.toml
./target/release/niri-native
```

启动前先退出已运行的 Rift 或 niri-native，避免两个窗口管理器同时接管快捷键和窗口位置。将 `niri-native` 和 `niri-native-cli` 放在同一目录，便于 CLI 管理服务。

## 原生工作区

```sh
./target/release/niri-native-cli query workspaces
./target/release/niri-native-cli execute workspace create
./target/release/niri-native-cli execute workspace switch 1   # 第二个桌面
./target/release/niri-native-cli execute workspace destroy 2  # 删除第三个桌面
```

- `index` 从 0 开始，是切换和删除命令接收的参数；`space_id` 是 macOS 自己的标识，不是序号。
- 切换时先在目标显示器上确认当前 Space，再隐藏原 Space 的窗口。不需要模拟滑动或打开 Mission Control。全屏 Space 不是普通工作区目标。
- 只能删除非活动桌面；macOS 可能将其中的窗口迁移到其他桌面。启动时不会根据 `default_workspace_count` 自动创建或删除桌面。
- Overview 按原生窗口归属展示已跟踪的窗口。可在 `[settings.layout]` 中设置 `mode = "scrolling"` 使用滚动列式布局，也可继续使用其他 Rift 布局与应用规则。

## 从 rift-native 升级

**不需要搬配置。** `rift` 和 `rift-cli` 仍随源码构建，与新命令使用同一份实现，不是另一个服务。为兼容现有快捷键、LaunchAgent 和 SketchyBar 脚本，以下名称保持不变：

| 项目 | 继续使用 |
| --- | --- |
| 用户配置 | `~/.config/rift-native/config.toml` |
| 保存的布局 | `~/.rift-native/layout.ron` |
| Mach IPC / launchd 服务 | `git.xbunax.rift-native` |
| 配置表 | `[virtual_workspaces]` |

上游 Rift 的 `~/.config/rift/` 不会被覆盖。[默认配置](rift.default.toml)列出了可用设置；[原生 Spaces 接口说明](native-spaces/README.md)介绍了底层操作。感谢 [Rift 的作者及贡献者](https://github.com/acsandmann/rift/graphs/contributors)，许可见 [LICENSE](LICENSE)。
