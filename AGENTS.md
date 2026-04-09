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

- **panel tab bar 的 "+" 按钮**：改为 dispatch `NewCenterTerminal`，行为与 zterm 理念一致（在 center 打开）。
- **右键菜单 "New Terminal"**：改为 dispatch `NewCenterTerminal`，避免原来因焦点条件不满足而静默失效的问题。

- **右键 "Open in Terminal"**：`TerminalPanel::open_terminal` 从 dock 路由改为走 `add_center_terminal`，所有上下文菜单（project panel、outline panel、editor、pane tab）的 "Open in Terminal" 现在都在 center pane 打开终端。
- **vim `:term` 命令**：从 dispatch `terminal_panel::Toggle` 改为 dispatch `workspace::NewCenterTerminal`。

---

## 关键 Actions

| Action | 命名空间 | 说明 |
|--------|----------|------|
| `NewCenterTerminal` | `workspace` | 在 center pane 打开新终端（zterm 主要入口） |
| `NewTerminal` | `workspace` | 同 `NewCenterTerminal`，handler 已改为始终走 center |
| `OpenTerminal` | `workspace` | 携带 `working_directory` 的 action，已改为走 center（右键菜单 "Open in Terminal"） |
| `ToggleFocus` | `terminal_panel` | 切换 dock terminal panel 焦点（app 菜单保留） |
| `Toggle` | `terminal_panel` | 打开/关闭 dock terminal panel（不再有默认快捷键绑定） |

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
6. ~~**关闭最后一个 center item 后 pane 为空**（#7）：在 `workspace.rs` `handle_pane_event` 的 `RemovedItem` 分支里自动 dispatch `NewCenterTerminal`~~ **已撤销**：该行为导致关闭最后一个终端后无法关闭窗口，已从 `RemovedItem` 分支中移除。
7. **"About Zed" 窗口标题**（#10）：`zed.rs` `open_about_window` 中 `TitlebarOptions::title` 已从 `"About Zed"` 改为 `"About Zterm"`。
8. **project panel 在 terminal-first 布局下默认折叠**（#11）：`zed.rs` `initialize_panels` 中，启动时若 center pane 为空（无文件），在 dispatch `NewCenterTerminal` 前先调用 `workspace.close_panel::<ProjectPanel>()`。

9. **右键 "Open in Terminal" 打开 dock 而非 center**（#13）：`TerminalPanel::open_terminal` 从 `panel.add_terminal_shell()` 改为 `Self::add_center_terminal()`，终端在 center pane 打开。
10. **vim `:term` 打开 dock 而非 center**（#14）：`:term`/`:Term` 从 dispatch `terminal_panel::Toggle` 改为 `workspace::NewCenterTerminal`。
11. **dock terminal panel 从序列化状态恢复后仍然可见**（#15）：`initialize_panels` 中在所有面板加载完毕后无条件调用 `workspace.close_panel::<TerminalPanel>()`，确保 dock terminal panel 启动时始终关闭。之前 dock 会从上次 session 的序列化状态恢复为打开，导致底部出现一个空的终端区域。

---

## 待处理的问题

| # | 问题 | 位置 |
|---|------|------|
| 8 | center terminal 序列化恢复需验证（代码已有实现，缺集成测试） | `terminal_view.rs` `deserialize` |

---

## 开发注意事项

- **不要**在 `main.rs` 的 `open_new` 回调里再次 dispatch `NewCenterTerminal`，启动终端的唯一触发点是 `zed.rs` 的 `initialize_panels`。
- `TerminalPanel::set_active` 里的自动 spawn terminal 逻辑**必须保留**——这是 dock panel 首次打开时自动创建终端的入口（上游原版行为）。
- **不要**在 `workspace.rs` 的 `RemovedItem` 事件分支里自动 dispatch `NewCenterTerminal`——这会导致关闭最后一个终端后无法关闭窗口。
- 新的终端相关 action handler 统一走 `TerminalPanel::add_center_terminal`，不要再引入 panel dock 路由逻辑。
- **不要**把默认快捷键绑定到 `terminal_panel::Toggle`——`Toggle` 仍可用但没有默认快捷键，用户可自行绑定。
- dock terminal panel 在 `initialize_panels` 中始终被关闭（`close_panel::<TerminalPanel>`），不要移除该逻辑，否则序列化恢复会导致底部出现空的 dock 终端区域。
- 构建/lint 用 `./script/clippy`，不要直接用 `cargo clippy`。
