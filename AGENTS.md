# Zterm — Agent Context

## 项目是什么

**Zterm** 是基于 [Zed](https://github.com/zed-industries/zed) 编辑器 fork 出来的、以**终端为核心**的开发工具。
上游 Zed 是一个以代码编辑器为核心、终端作为辅助面板的工具；Zterm 的目标是把这个主次关系翻转过来：
**终端是主视图，编辑器是次级辅助视图**。

当前工作分支名为 `zterm`，基于 upstream `main` 分叉。

---

## 与上游 Zed 的核心差异（`zterm` 分支）

### 1. 身份标识重命名
所有配置路径、数据路径、日志路径均从 `Zed` 重命名为 `Zterm/zterm`，避免与用户系统上已安装的 Zed 冲突：

| 平台 | 配置目录 | 数据目录 |
|------|----------|----------|
| macOS | `~/.config/zterm` | `~/Library/Application Support/Zterm` |
| Linux | `$XDG_CONFIG_HOME/zterm` | `$XDG_DATA_HOME/zterm` |
| Windows | `%APPDATA%\Zterm` | `%LOCALAPPDATA%\Zterm` |

Windows app identifier、bundle ID 也均改为 `Zterm-Editor-*`。
`ReleaseChannel::display_name()` 返回 `"zterm"` / `"zterm Dev"` 等。
`ReleaseChannel::app_id()` 返回 `"io.zterm.Zterm"` 等。

涉及文件：
- `crates/paths/src/paths.rs`
- `crates/release_channel/src/lib.rs`
- `crates/zed/build.rs`

### 2. 终端优先的启动布局

- **禁用 Welcome 页面**：启动时不再显示欢迎页。
- **启动自动开终端**：`initialize_panels`（`crates/zed/src/zed.rs`）在所有面板加载完毕后，若 center pane 为空则自动 dispatch `NewCenterTerminal`，打开一个终端作为默认视图。
- **`NewTerminal` 始终在 center pane 打开**：`TerminalPanel::new_terminal` 的路由逻辑被简化，无论当前焦点在哪里，新终端都创建在 center pane，而非 bottom dock panel。

涉及文件：
- `crates/terminal_view/src/terminal_panel.rs`（`new_terminal` 函数）
- `crates/zed/src/zed.rs`（`initialize_panels`）
- `crates/zed/src/main.rs`（`restore_or_create_workspace`）
- `crates/workspace/src/workspace.rs`（`NewCenterTerminal` action 定义）

### 3. Dock panel 行为调整

- **`TerminalPanel::set_active` 不再自动建终端**：原来打开 dock terminal panel 时若为空会自动 spawn 一个终端，现已移除该行为，dock panel 完全由用户手动控制。
- **panel tab bar 的 "+" 按钮**：改为 dispatch `NewCenterTerminal`，行为与 zterm 理念一致（在 center 打开）。
- **右键菜单 "New Terminal"**：改为 dispatch `NewCenterTerminal`，避免原来因焦点条件不满足而静默失效的问题。

---

## 关键 Actions

| Action | 命名空间 | 说明 |
|--------|----------|------|
| `NewCenterTerminal` | `workspace` | 在 center pane 打开新终端（zterm 主要入口） |
| `NewTerminal` | `workspace` | 同 `NewCenterTerminal`，handler 已改为始终走 center |
| `ToggleFocus` | `terminal_panel` | 切换 dock terminal panel 焦点 |
| `Toggle` | `terminal_panel` | 打开/关闭 dock terminal panel |

`TerminalView::deploy` 处理 `NewCenterTerminal`，调用 `TerminalPanel::add_center_terminal`。

---

## 重要文件索引

```
crates/terminal_view/src/terminal_panel.rs   终端面板（dock + center 路由逻辑）
crates/terminal_view/src/terminal_view.rs    终端视图（渲染、事件、右键菜单）
crates/zed/src/zed.rs                        initialize_panels（启动自动打开终端）
crates/zed/src/main.rs                       restore_or_create_workspace（启动流程）
crates/workspace/src/workspace.rs            NewCenterTerminal / NewTerminal action 定义
crates/paths/src/paths.rs                    所有路径（已改名为 zterm）
crates/release_channel/src/lib.rs            display_name / app_id（已改名为 zterm）
crates/zed/build.rs                          Windows 构建标识（已改名为 Zterm）
```

---

## 已修复的问题（当前 session）

1. **测试与新行为不一致**：5 个测试用例从断言"panel 路由"改为断言"始终 center 路由"，测试名也同步更新。
2. **启动时 `NewCenterTerminal` double dispatch**：`main.rs` 的 `open_new` 回调里多余的 dispatch 已移除，唯一触发点在 `zed.rs` 的 `initialize_panels`，并有 `items_len() == 0` 保护。
3. **dock `set_active` 自动建终端**：已移除，dock panel 打开时不再自动 spawn terminal。
4. **panel tab bar "+" 使用 `NewTerminal`**：改为 `NewCenterTerminal`，行为明确。
5. **右键菜单 "New Terminal" 静默失效**：改为 `NewCenterTerminal`，直接走 center 路径。

---

## 待处理的问题

| # | 问题 | 位置 |
|---|------|------|
| 7 | 关闭最后一个 center terminal 后 pane 为空，不自动补开 | `workspace.rs` `handle_pane_event` `RemovedItem` 分支 |
| 8 | center terminal 序列化恢复需验证 | `terminal_view.rs` `deserialize` |
| 10 | "About Zed" 窗口标题仍写 "About Zed" 未改为 "About Zterm" | `zed.rs` `open_about_window` |
| 11 | project panel 在 terminal-first 布局下可考虑默认折叠 | `zed.rs` `initialize_panels` |

---

## 开发注意事项

- **不要**在 `main.rs` 的 `open_new` 回调里再次 dispatch `NewCenterTerminal`，启动终端的唯一触发点是 `zed.rs` 的 `initialize_panels`。
- **不要**在 `TerminalPanel::set_active` 里自动 spawn terminal——dock panel 的生命周期由用户控制。
- 新的终端相关 action handler 统一走 `TerminalPanel::add_center_terminal`，不要再引入 panel dock 路由逻辑。
- 构建/lint 用 `./script/clippy`，不要直接用 `cargo clippy`。