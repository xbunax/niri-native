# niri-native

**本项目 fork 自 [Rift（acsandmann/rift）](https://github.com/acsandmann/rift)，克隆地址为 `https://github.com/xbunax/niri-native.git`。** niri-native 是 macOS 平铺窗口管理器：保留 Rift 的布局、快捷键和 Overview，把每个工作区直接映射到一个原生 macOS 桌面。名字里的 niri 指滚动列式布局的交互风格；本项目不是 Linux [niri](https://github.com/YaLTeR/niri) 合成器的移植。

![Rift 窗口布局演示](assets/demo.gif)

## Homebrew 安装

```sh
brew tap xbunax/tap
brew install xbunax/tap/niri-native
niri-native --version
```

Homebrew 安装的是 GitHub Release 的预编译通用二进制，不会启动服务，也不会覆盖上游 Rift 的命令。若 Homebrew 提示 tap 未受信任，确认来源后可执行 `brew trust --formula xbunax/tap/niri-native` 再安装。

niri-native 从 **0.1.0** 起独立编号；旧的 `0.8.6` 沿用了 Rift 的版本号。已安装旧版可执行 `brew update` 和 `brew upgrade xbunax/tap/niri-native`。如果已在使用旧配置，可以直接启动，不需要复制模板。

新用户如需创建可编辑的配置文件：

```sh
mkdir -p ~/.config/niri-native
cp -n "$(brew --prefix niri-native)/share/niri-native/rift.default.toml" ~/.config/niri-native/config.toml
```

默认读取 `~/.config/niri-native/config.toml`；找不到该文件时，回退到 `~/.config/rift-native/config.toml`。两者都不存在时，使用内置默认配置。

## 启动与登录自启

先退出正在运行的 Rift 或 niri-native，避免两个窗口管理器同时接管键盘和窗口。首次启动前，请在「系统设置 → 隐私与安全性 → 辅助功能」中授权 `niri-native`。可先运行 `niri-native` 验证权限，再按 `Ctrl+C` 退出，或直接设置登录自启：

```sh
niri-native service install   # 写入当前用户的 LaunchAgent；仅首次需要
niri-native service start     # 启用并启动，以后登录时自动启动
launchctl print "gui/$(id -u)/git.xbunax.rift-native"  # 查看服务状态
```

服务已安装时直接执行 `service start`，它也可以在首次启动时自动安装。无需 `sudo`，也不使用 `brew services`。要停止服务用 `niri-native service stop`；若要取消登录自启，在停止后再执行 `niri-native service uninstall`。

## 从源码构建

需要 macOS、Rust 工具链和 Xcode Command Line Tools。使用 SkyLight 私有接口，不需要关闭 SIP；不同 macOS 版本的接口可能变化。首次启动请按系统提示授予辅助功能权限；Overview 的窗口预览还可能需要屏幕录制权限。

若已有旧版配置，可跳过下面的 `mkdir` 和 `cp`，继续使用旧配置，或按下方说明迁移。

```sh
git clone https://github.com/xbunax/niri-native.git
cd niri-native
cargo build --release --locked --bins
mkdir -p ~/.config/niri-native
cp -n rift.default.toml ~/.config/niri-native/config.toml
./target/release/niri-native
```

源码构建版也可用 `./target/release/niri-native service start` 设置登录自启；请将二进制保留在固定位置，LaunchAgent 会记录它的绝对路径。

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

新版优先读取 `~/.config/niri-native/config.toml`，没有时回退到 `~/.config/rift-native/config.toml`。要主动迁移，可执行 `mkdir -p ~/.config/niri-native`，再执行 `cp -n ~/.config/rift-native/config.toml ~/.config/niri-native/config.toml`。旧文件不会被删除，配置中显式指定的布局目录也不会被重写。

`rift` 和 `rift-cli` 仍随源码构建，与新命令使用同一份实现，不是另一个服务。为兼容现有快捷键、LaunchAgent 和 SketchyBar 脚本，以下名称保持不变：

| 项目 | 继续使用 |
| --- | --- |
| 保存的布局 | `~/.rift-native/layout.ron` |
| Mach IPC / launchd 服务 | `git.xbunax.rift-native` |
| 配置表 | `[virtual_workspaces]` |

上游 Rift 的 `~/.config/rift/` 不会被覆盖。[默认配置](rift.default.toml)列出了可用设置；[原生 Spaces 接口说明](native-spaces/README.md)介绍了底层操作。感谢 [Rift 的作者及贡献者](https://github.com/acsandmann/rift/graphs/contributors)，许可见 [LICENSE](LICENSE)。
