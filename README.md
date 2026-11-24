<div align="center">

# rift
  <p>rift is a tiling window manager for macOS that focuses on performance and usability. </p>
  <img src="assets/demo.gif" alt="Rift demo" />

  <p>
    <a href="https://github.com/acsandmann/rift/actions/workflows/rust.yml">
      <img src="https://img.shields.io/github/actions/workflow/status/acsandmann/rift/rust.yml?style=flat-square" alt="Rust CI Status" />
    </a>
    <a href="https://github.com/acsandmann/rift/commits/main">
      <img src="https://img.shields.io/github/last-commit/acsandmann/rift?style=flat-square" alt="Last Commit" />
    </a>
    <a href="https://github.com/acsandmann/rift/issues">
      <img src="https://img.shields.io/github/issues/acsandmann/rift?style=flat-square" alt="Open Issues" />
    </a>
    <a href="https://github.com/acsandmann/rift/stargazers">
      <img src="https://img.shields.io/github/stars/acsandmann/rift?style=flat-square" alt="GitHub stars" />
    </a>
    <a href="https://matrix.to/#/%23rift:matrix.org">
      <img src="https://img.shields.io/matrix/rift%3Amatrix.org?style=flat-square" alt="Matrix" />
    </a>
  </p>
</div>

## Native Spaces fork

This branch replaces virtual desktops with ordinary macOS Spaces. It keeps the Rift command and configuration syntax, including the `[virtual_workspaces]` table, and uses the existing layouts (including niri-style scrolling). Normal desktops appear in both Rift's overview and macOS Mission Control. Fullscreen Spaces are not workspace targets.

```sh
cargo build --release --bins
mkdir -p ~/.config/rift-native
cp -n rift.default.toml ~/.config/rift-native/config.toml
./target/release/rift
```

Use the `rift-cli` built alongside that binary. Configuration and layout presets are under `~/.config/rift-native/`, the saved layout is `~/.rift-native/layout.ron`, and IPC and launchd use `git.xbunax.rift-native`. Keep these binaries together in a separate directory from upstream Rift. **Stop the original window manager before starting this fork**: separate configuration does not isolate frame writes or hotkeys in the same GUI session. The upstream install links below are not native-fork installation instructions.

Select `mode = "scrolling"` under `[settings.layout]` to use scrolling for every desktop, or continue using workspace layout rules. Use existing workspace shortcuts or macOS Mission Control to switch, and `create_workspace` to add a desktop. Rift neither provisions the legacy configured workspace count nor removes desktops on startup. macOS owns desktop order, display placement and Space-switch animation; global `workspace_rules.display` pins cannot move a native desktop. The cross-display workspace command moves its windows to the desktop at the same ordinal, rather than moving the macOS desktop itself. Private operations require the SkyLight bridged APIs described in [native-spaces](native-spaces/README.md); there is no simulated fallback for workspace mutations.

### Native workspace CLI

```sh
./target/release/rift-cli query workspaces
./target/release/rift-cli execute workspace list  # same JSON as query workspaces
./target/release/rift-cli execute workspace create
./target/release/rift-cli execute workspace destroy 2
```

`list` and `query workspaces` accept `--display <UUID>` and `--space-id <ID>`. Use the zero-based `index` field for `destroy`, not `space_id`. Only an inactive ordinary desktop can be removed. macOS may relocate windows from the removed desktop. Create and destroy return the native `space_id` after the change shows up in the desktop inventory.

## features
- multiple layout styles
  - tiling (i3/sway-like)
  - binary space partitioning (bspwm-like)
  - floating (independent window frames with optional stacks)
  - master-stack (dwm-like)
  - scrolling columns (niri-style)
  - stack (accordion)
- menubar icon that opens a menu for switching workspaces, changing layouts, and accessing quick rift controls <details> <summary><sup>click to see the menu bar icon</sup></summary><img src="assets/menu_menu.png" alt="Rift menu bar icon" /></details>
- save and restore layouts from the menu bar or CLI, with reusable layouts listed from a configurable folder <details> <summary><sup>click to see the menu</sup></summary><img src="assets/menu_layouts.png" alt="Rift menu for restoring layouts" /></details>
- niri-style mission control for viewing workspaces and dragging windows between them
 <!--<details><summary><sup>click to see mission control</sup></summary><img src="assets/mission_control.png" alt="Rift Mission Control view" /></details> -->
- focus follows the mouse with auto raise
<!-- - drag windows over one another to swap positions -->
- does **not** require disabling SIP
- native macOS tab support
- performant animations <sup>(as seen in the [demo](#rift))</sup>
- switch to next/previous workspace with trackpad gestures <sup>(just like native macOS)</sup>
- hot reloadable configuration
- mach port based IPC for communicating with rift from <a href="https://acsandmann.github.io/rift-docs/ecosystem/plugins/">third-party programs</a> (sketchybar, etc)
- works with "Displays have separate Spaces" enabled (unlike all other major WMs)

## quick start
Get up and running via the docs:
<br>

[<kbd><br>config<br></kbd>][config_link]

[<kbd><br>quick start<br></kbd>][quick_start]
<br>

## community

Join [#rift:matrix.org](https://matrix.to/#/#rift:matrix.org) for discussion, support, and development.

## support

If rift is part of your daily workflow, consider [sponsoring its development](https://github.com/sponsors/acsandmann).

## motivation
Aerospace worked well for me, but I missed animations and the ability to use fullscreen on one display while working on the other. I also prefer leveraging private/undocumented APIs as they tend to be more reliable (due to the OS being built on them and all the public APIs) and performant.
<sup><sup>for more on why rift exists and what rift strives to do, see the [manifesto](manifesto.md)</sup></sup>

<!---------------------------------------------------------------------------->

[config_link]: https://acsandmann.github.io/rift-docs/reference/configuration/
[quick_start]: https://acsandmann.github.io/rift-docs/quick-start/
