# macOS 27 原生 Spaces CLI（SIP 保持开启）

默认使用 SkyLight 私有 API，不打开 Mission Control、不模拟输入、不注入 Dock，也不需要 root。用已登录的图形会话运行：

```sh
cd native-spaces
xcrun clang -fobjc-arc -Wall -Wextra -Werror \
  -framework AppKit -framework ApplicationServices spaces.m -o spaces
./spaces probe
./spaces list
```

- `./spaces create [display-uuid]`：创建普通桌面并输出新 Space ID。传入 `list` 中的显示器标识即可在该显示器上创建；省略时由系统选定显示器。指定目标要求 macOS 开启「显示器具有单独的空间」，并能枚举到该显示器。
- `./spaces focus <space-id>`：切换至目标 Space。
- `./spaces destroy <space-id>`：删除非当前的普通桌面；删除可能改变窗口归属。
- `./spaces move <window-id> <space-id>`：将窗口迁移到目标普通桌面。窗口 ID 是当前会话的 `CGWindowID`，不是 PID。

`list` 输出显示器标识和 Space ID，`*` 表示当前 Space。`probe` 只检查私有类和方法是否存在；操作命令会回读系统状态。系统仍可能显示自身的切换动画或重排窗口。私有接口行为可能随 macOS 版本变化，不会自动回退到界面操作。

## SkyLight 私有 API

CLI 动态加载 `/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight`，用 Objective-C 运行时查找下列操作类。初始化后，调用对象的 `performWithWMBridgeDelegate`：

| CLI 操作 | SkyLight 操作类 | 初始化方法 | 成功判据 |
| --- | --- | --- | --- |
| `create` | `SLSBridgedSpaceCreateOperation` | `initWithOptions:values:`，传入 `0` 和 `@{@"type": @0}`；指定显示器时在字典中加入 `@"Display Identifier"` | 同步结果对象的 `spaceID` 非零，新 ID 是普通桌面，且归属指定显示器（若有） |
| `destroy` | `SLSBridgedSpaceDestroyOperation` | `initWithSpaceID:` | 目标 ID 从受管桌面列表中消失 |
| `focus` | `SLSBridgedManagedDisplaySetCurrentSpaceOperation` | `initWithDisplayIdentifier:spaceID:`，显示器标识取自目标 Space 所在的显示器 | 该显示器的 `Current Space` 变为目标 ID |
| `move` | `SLSBridgedMoveWindowsToManagedSpaceOperation` | `initWithWindows:spaceID:`，窗口 ID 封装成 `NSNumber` 数组 | 异步提交后，窗口的 Space 归属包含目标 ID |

读回使用 `SLSMainConnectionID()` 获取连接，再用 `SLSCopyManagedDisplaySpaces` 查询受管桌面和当前桌面，或用 `SLSCopySpacesForWindows`（掩码 `0x7`）查询窗口归属。`move` 若能解析到 `SLSPerformAsynchronousBridgedWindowManagementOperation` 则优先使用该提交函数，否则直接调用对象方法；没有可用的 bridged 入口时，才尝试旧的 `SLSMoveWindowsToManagedSpace`。

**验证范围：** Apple Silicon，macOS 27.0.1（26A434），SIP 开启。四项操作均已通过 CLI 实测，包括移动另一进程的窗口。此 build 上异步 C 提交函数无法解析，但对象方法可用。作为对照，`SLSSpaceCreate` 返回 `0`，`SLSSpaceDestroy` 未删掉测试桌面，旧的 `SLSMoveWindowsToManagedSpace` 未移动测试窗口。符号存在并不代表调用有效。

## 可选的辅助功能路径

如需显式改用 Mission Control，可以运行 `./spaces --ax create [display-uuid]`。`--ax` 也支持 `focus`、`destroy`，会通过 [WindowManager 的 AX 树](https://github.com/Hammerspoon/hammerspoon/issues/3897) 操作 Mission Control，并需要给运行 CLI 的宿主授予辅助功能权限。`--ax` 不支持 `move`。
