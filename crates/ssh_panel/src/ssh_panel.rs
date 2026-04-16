mod ssh_panel_settings;
pub mod ssh_status_button;

use anyhow::Result;
use collections::{BTreeSet, HashSet};
use fs::Fs;
use futures::StreamExt as _;
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    ParentElement, Render, SharedString, Styled, Subscription, Task, WeakEntity, Window,
};
use paths::{global_ssh_config_file, user_ssh_config_file};
use recent_projects::{RemoteServerProjects, RemoteSettings, SshServerIndex, open_remote_project};
use remote::{self, RemoteConnectionOptions, SshConnectionOptions};
use settings::{
    DockSide, RemoteProject, Settings as _, SshConnection, update_settings_file, watch_config_file,
};
use ssh_panel_settings::SshPanelSettings;
use std::path::PathBuf;
use std::sync::Arc;
use ui::prelude::*;
use ui::{IconButton, IconButtonShape, IconSize, ListItem, ListSeparator, Tooltip};
use workspace::dock::{DockPosition, Panel, PanelEvent};
use workspace::{MultiWorkspace, OpenOptions, Workspace};
use zed_actions::ssh_panel::ToggleFocus;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<SshPanel>(window, cx);
        });
    })
    .detach();
}

const SSH_PANEL_KEY: &str = "ssh_panel";

pub struct SshPanel {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    fs: Arc<dyn Fs>,
    ssh_connections: Vec<SshConnection>,
    ssh_config_servers: BTreeSet<SharedString>,
    ssh_config_watch_task: Task<()>,
    selected_index: Option<usize>,
    expanded_servers: HashSet<usize>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Debug)]
enum SshServerEntry {
    Configured {
        index: usize,
        connection: SshConnection,
    },
    FromConfig {
        host: SharedString,
    },
}

impl SshServerEntry {
    fn display_name(&self) -> SharedString {
        match self {
            SshServerEntry::Configured { connection, .. } => {
                if let Some(nickname) = &connection.nickname {
                    nickname.clone().into()
                } else {
                    let mut display = connection.host.clone();
                    if let Some(username) = &connection.username {
                        display = format!("{}@{}", username, display);
                    }
                    if let Some(port) = connection.port {
                        if port != 22 {
                            display = format!("{}:{}", display, port);
                        }
                    }
                    display.into()
                }
            }
            SshServerEntry::FromConfig { host } => host.clone(),
        }
    }

    fn connection_options(&self) -> SshConnectionOptions {
        match self {
            SshServerEntry::Configured { connection, .. } => connection.clone().into(),
            SshServerEntry::FromConfig { host } => SshConnectionOptions {
                host: host.to_string().into(),
                ..Default::default()
            },
        }
    }

    fn projects(&self) -> Vec<RemoteProject> {
        match self {
            SshServerEntry::Configured { connection, .. } => {
                connection.projects.iter().cloned().collect()
            }
            SshServerEntry::FromConfig { .. } => Vec::new(),
        }
    }
}

impl SshPanel {
    pub fn new(
        workspace: &Workspace,
        fs: Arc<dyn Fs>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let weak_workspace = workspace.weak_handle();

        let settings = RemoteSettings::get_global(cx);
        let ssh_connections: Vec<SshConnection> = settings.ssh_connections().collect();
        let read_ssh_config = settings.read_ssh_config;

        let ssh_config_watch_task = if read_ssh_config {
            Self::spawn_ssh_config_watch(fs.clone(), cx)
        } else {
            Task::ready(())
        };

        let settings_subscription = cx.observe_global::<settings::SettingsStore>(|this, cx| {
            this.refresh_connections(cx);
        });

        let expanded_servers = ssh_connections
            .iter()
            .enumerate()
            .filter(|(_, connection)| !connection.projects.is_empty())
            .map(|(index, _)| index)
            .collect();

        SshPanel {
            focus_handle,
            workspace: weak_workspace,
            fs,
            ssh_connections,
            ssh_config_servers: BTreeSet::new(),
            ssh_config_watch_task,
            selected_index: None,
            expanded_servers,
            _subscriptions: vec![settings_subscription],
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            let fs = workspace.project().read(cx).fs().clone();
            cx.new(|cx| Self::new(workspace, fs, window, cx))
        })
    }

    fn refresh_connections(&mut self, cx: &mut Context<Self>) {
        let settings = RemoteSettings::get_global(cx);
        self.ssh_connections = settings.ssh_connections().collect();

        for (index, connection) in self.ssh_connections.iter().enumerate() {
            if !connection.projects.is_empty() {
                self.expanded_servers.insert(index);
            }
        }

        let read_ssh_config = settings.read_ssh_config;
        if read_ssh_config {
            self.ssh_config_watch_task = Self::spawn_ssh_config_watch(self.fs.clone(), cx);
        } else {
            self.ssh_config_servers.clear();
            self.ssh_config_watch_task = Task::ready(());
        }

        cx.notify();
    }

    fn server_entries(&self) -> Vec<SshServerEntry> {
        let mut entries: Vec<SshServerEntry> = self
            .ssh_connections
            .iter()
            .enumerate()
            .map(|(index, connection)| SshServerEntry::Configured {
                index,
                connection: connection.clone(),
            })
            .collect();

        let configured_hosts: BTreeSet<&str> = self
            .ssh_connections
            .iter()
            .map(|conn| conn.host.as_str())
            .collect();

        for host in &self.ssh_config_servers {
            if !configured_hosts.contains(host.as_ref()) {
                entries.push(SshServerEntry::FromConfig { host: host.clone() });
            }
        }

        entries
    }

    fn connect_to_server(
        &mut self,
        entry: &SshServerEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let connection_options = entry.connection_options();
        let server_index = match entry {
            SshServerEntry::Configured { index, .. } => Some(SshServerIndex::new(*index)),
            SshServerEntry::FromConfig { .. } => None,
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let fs = self.fs.clone();

        workspace.update(cx, |workspace, cx| {
            let weak = cx.entity().downgrade();
            workspace.toggle_modal(window, cx, |window, cx| {
                RemoteServerProjects::connect_to_ssh_server(
                    server_index,
                    connection_options,
                    false,
                    fs,
                    weak,
                    window,
                    cx,
                )
            });
        });
    }

    fn connect_to_project(
        &mut self,
        entry: &SshServerEntry,
        project: &RemoteProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let connection_options = entry.connection_options();
        let paths: Vec<PathBuf> = project.paths.iter().map(PathBuf::from).collect();
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let app_state = workspace.read(cx).app_state().clone();

        let requesting_window = window.window_handle().downcast::<MultiWorkspace>();

        cx.spawn_in(window, async move |_this, cx| {
            let result = open_remote_project(
                RemoteConnectionOptions::Ssh(connection_options),
                paths,
                app_state,
                OpenOptions {
                    requesting_window,
                    ..OpenOptions::default()
                },
                cx,
            )
            .await;
            if let Err(error) = result {
                log::error!("Failed to connect to SSH project: {error:#}");
                cx.prompt(
                    gpui::PromptLevel::Critical,
                    "Failed to connect",
                    Some(&error.to_string()),
                    &["Ok"],
                )
                .await
                .ok();
            }
        })
        .detach();
    }

    fn view_server_options(
        &mut self,
        entry: &SshServerEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SshServerEntry::Configured { index, connection } = entry else {
            return;
        };
        let connection_options: SshConnectionOptions = connection.clone().into();
        let server_index = SshServerIndex::new(*index);
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let fs = self.fs.clone();

        workspace.update(cx, |workspace, cx| {
            let weak = cx.entity().downgrade();
            workspace.toggle_modal(window, cx, |window, cx| {
                RemoteServerProjects::view_ssh_server_options(
                    server_index,
                    connection_options,
                    fs,
                    weak,
                    window,
                    cx,
                )
            });
        });
    }

    fn remove_server(&mut self, index: usize, cx: &mut Context<Self>) {
        let fs = self.fs.clone();
        update_settings_file(fs, cx, move |content, _| {
            if let Some(connections) = content.remote.ssh_connections.as_mut() {
                if index < connections.len() {
                    connections.remove(index);
                }
            }
        });
    }

    fn remove_project(
        &mut self,
        server_index: usize,
        project: RemoteProject,
        cx: &mut Context<Self>,
    ) {
        let fs = self.fs.clone();
        update_settings_file(fs, cx, move |content, _| {
            if let Some(connections) = content.remote.ssh_connections.as_mut() {
                if let Some(server) = connections.get_mut(server_index) {
                    server.projects.remove(&project);
                }
            }
        });
    }

    fn spawn_ssh_config_watch(fs: Arc<dyn Fs>, cx: &Context<Self>) -> Task<()> {
        enum ConfigSource {
            User(String),
            Global(String),
        }

        let mut streams = Vec::new();
        let mut tasks = Vec::new();

        let user_path = user_ssh_config_file();
        let (user_stream, user_task) =
            watch_config_file(cx.background_executor(), fs.clone(), user_path);
        streams.push(user_stream.map(ConfigSource::User).boxed());
        tasks.push(user_task);

        if let Some(global_path) = global_ssh_config_file() {
            let (global_stream, global_task) =
                watch_config_file(cx.background_executor(), fs, global_path.to_path_buf());
            streams.push(global_stream.map(ConfigSource::Global).boxed());
            tasks.push(global_task);
        }

        let mut merged_stream = futures::stream::select_all(streams);

        cx.spawn(async move |panel, cx| {
            let _tasks = tasks;
            let mut global_hosts = BTreeSet::default();
            let mut user_hosts = BTreeSet::default();

            while let Some(event) = merged_stream.next().await {
                match event {
                    ConfigSource::Global(content) => {
                        global_hosts = parse_ssh_config_hosts(&content);
                    }
                    ConfigSource::User(content) => {
                        user_hosts = parse_ssh_config_hosts(&content);
                    }
                }

                if panel
                    .update(cx, |panel, cx| {
                        panel.ssh_config_servers = global_hosts
                            .iter()
                            .chain(user_hosts.iter())
                            .map(SharedString::from)
                            .collect();
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
    }

    fn is_server_connected(&self, entry: &SshServerEntry, cx: &App) -> bool {
        let connection_options = entry.connection_options();
        let remote_options = RemoteConnectionOptions::Ssh(connection_options);
        remote::has_active_connection(&remote_options, cx)
    }

    fn render_server_entry(
        &self,
        entry: &SshServerEntry,
        index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let display_name = entry.display_name();
        let is_selected = self.selected_index == Some(index);
        let entry_for_click = entry.clone();
        let is_from_config = matches!(entry, SshServerEntry::FromConfig { .. });
        let projects = entry.projects();
        let has_projects = !projects.is_empty();
        let is_expanded = self.expanded_servers.contains(&index);
        let is_connected = self.is_server_connected(entry, cx);

        let icon = if is_from_config {
            IconName::FileToml
        } else {
            IconName::Server
        };

        let configured_index = if let SshServerEntry::Configured { index: idx, .. } = entry {
            Some(*idx)
        } else {
            None
        };

        let chevron = if has_projects {
            if is_expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            }
        } else {
            IconName::ChevronRight
        };

        let server_row = ListItem::new(SharedString::from(format!("ssh-server-{}", index)))
            .toggle_state(is_selected)
            .inset(true)
            .spacing(ui::ListItemSpacing::Sparse)
            .start_slot(
                h_flex()
                    .gap_0p5()
                    .child(Icon::new(chevron).size(IconSize::Small).color(Color::Muted))
                    .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(Label::new(display_name.clone()).size(LabelSize::Small))
                    .when(is_connected, |this| {
                        this.child(
                            div()
                                .w(px(7.))
                                .h(px(7.))
                                .rounded_full()
                                .bg(Color::Created.color(cx)),
                        )
                    })
                    .when(is_from_config, |this| {
                        this.child(
                            Label::new("ssh config")
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected_index = Some(index);
                if has_projects {
                    if this.expanded_servers.contains(&index) {
                        this.expanded_servers.remove(&index);
                    } else {
                        this.expanded_servers.insert(index);
                    }
                    cx.notify();
                } else {
                    this.connect_to_server(&entry_for_click, window, cx);
                }
            }))
            .tooltip(Tooltip::text(if has_projects {
                "Toggle Projects"
            } else {
                "Open Folder"
            }))
            .when_some(configured_index, |this, idx| {
                let options_entry = entry.clone();
                this.end_slot(div()).end_slot_on_hover(
                    h_flex()
                        .gap_1()
                        .child(
                            IconButton::new(
                                SharedString::from(format!("ssh-settings-{}", idx)),
                                IconName::Settings,
                            )
                            .shape(IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Server Options"))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.view_server_options(&options_entry, window, cx);
                                },
                            )),
                        )
                        .child(
                            IconButton::new(
                                SharedString::from(format!("ssh-remove-{}", idx)),
                                IconName::Trash,
                            )
                            .shape(IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Remove Server"))
                            .on_click(cx.listener(
                                move |this, _, _window, cx| {
                                    this.remove_server(idx, cx);
                                },
                            )),
                        ),
                )
            });

        let project_rows: Vec<_> = if !is_expanded {
            Vec::new()
        } else {
            projects
                .iter()
                .enumerate()
                .map(|(project_index, project)| {
                    let path_label = project.paths.join(", ");
                    let path_tooltip = project.paths.join("\n");
                    let entry_for_project = entry.clone();
                    let project_clone = project.clone();
                    let project_for_remove = project.clone();

                    div()
                        .id(SharedString::from(format!(
                            "ssh-project-{}-{}",
                            index, project_index
                        )))
                        .child(
                            ListItem::new(SharedString::from(format!(
                                "ssh-project-item-{}-{}",
                                index, project_index
                            )))
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(
                                h_flex()
                                    .gap_0p5()
                                    .child(div().w(IconSize::Small.rems()).flex_shrink_0())
                                    .child(
                                        Icon::new(IconName::Folder)
                                            .size(IconSize::Small)
                                            .color(Color::Muted),
                                    ),
                            )
                            .child(Label::new(path_label).size(LabelSize::Small).single_line())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.connect_to_project(
                                    &entry_for_project,
                                    &project_clone,
                                    window,
                                    cx,
                                );
                            }))
                            .tooltip(Tooltip::text(path_tooltip))
                            .when_some(
                                configured_index,
                                |this, server_idx| {
                                    this.end_slot(div()).end_slot_on_hover(
                                        IconButton::new(
                                            SharedString::from(format!(
                                                "ssh-remove-project-{}-{}",
                                                index, project_index
                                            )),
                                            IconName::Trash,
                                        )
                                        .shape(IconButtonShape::Square)
                                        .icon_size(IconSize::Small)
                                        .icon_color(Color::Muted)
                                        .tooltip(Tooltip::text("Remove Project"))
                                        .on_click(
                                            cx.listener(move |this, _, _window, cx| {
                                                this.remove_project(
                                                    server_idx,
                                                    project_for_remove.clone(),
                                                    cx,
                                                );
                                            }),
                                        ),
                                    )
                                },
                            ),
                        )
                })
                .collect()
        };

        v_flex().child(server_row).children(project_rows)
    }

    fn render_header(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<'_> {
        let is_remote = self.workspace.upgrade().map_or(false, |workspace| {
            workspace.read(cx).project().read(cx).is_remote()
        });

        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .justify_between()
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::Server)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new("SSH Servers")
                            .size(LabelSize::Small)
                            .color(Color::Default),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .when(is_remote, |this| {
                        this.child(
                            IconButton::new("disconnect-remote", IconName::Disconnected)
                                .shape(IconButtonShape::Square)
                                .icon_size(IconSize::Small)
                                .icon_color(Color::Error)
                                .tooltip(Tooltip::text("Close Remote Connection"))
                                .on_click(cx.listener(|_this, _, window, cx| {
                                    window
                                        .dispatch_action(workspace::CloseProject.boxed_clone(), cx);
                                })),
                        )
                    })
                    .child(
                        IconButton::new("add-ssh-server", IconName::Plus)
                            .shape(IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Add SSH Server"))
                            .on_click(cx.listener(|_this, _, window, cx| {
                                window.dispatch_action(
                                    zed_actions::OpenRemote {
                                        from_existing_connection: false,
                                        create_new_window: false,
                                    }
                                    .boxed_clone(),
                                    cx,
                                );
                            })),
                    ),
            )
    }

    fn render_empty_state(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        v_flex()
            .p_4()
            .gap_2()
            .child(
                Label::new("No SSH servers configured")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Label::new("Add a server or enable reading from ~/.ssh/config in settings")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
    }
}

fn parse_ssh_config_hosts(content: &str) -> BTreeSet<String> {
    let mut hosts = BTreeSet::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("Host ")
            .or_else(|| line.strip_prefix("host "))
        {
            for host in rest.split_whitespace() {
                if !host.contains('*') && !host.starts_with('!') && !host.is_empty() {
                    hosts.insert(host.to_string());
                }
            }
        }
    }
    hosts
}

impl EventEmitter<PanelEvent> for SshPanel {}

impl Focusable for SshPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for SshPanel {
    fn persistent_name() -> &'static str {
        "SshPanel"
    }

    fn panel_key() -> &'static str {
        SSH_PANEL_KEY
    }

    fn position(&self, _window: &Window, cx: &App) -> DockPosition {
        match SshPanelSettings::get_global(cx).dock {
            DockSide::Left => DockPosition::Left,
            DockSide::Right => DockPosition::Right,
        }
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fs = self.fs.clone();
        update_settings_file(fs, cx, move |content, _| {
            let dock = match position {
                DockPosition::Left | DockPosition::Bottom => DockSide::Left,
                DockPosition::Right => DockSide::Right,
            };
            content.ssh_panel.get_or_insert_with(Default::default).dock = Some(dock);
        });
    }

    fn default_size(&self, _window: &Window, cx: &App) -> Pixels {
        SshPanelSettings::get_global(cx).default_width
    }

    fn icon(&self, _window: &Window, cx: &App) -> Option<IconName> {
        SshPanelSettings::get_global(cx)
            .button
            .then_some(IconName::Server)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("SSH Servers")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        7
    }
}

impl Render for SshPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.server_entries();
        let panel_bg = cx.theme().colors().panel_background;
        let header = self.render_header(window, cx);

        let content = if entries.is_empty() {
            self.render_empty_state(window, cx).into_any_element()
        } else {
            let entry_elements: Vec<_> = entries
                .iter()
                .enumerate()
                .flat_map(|(index, entry)| {
                    let element = self.render_server_entry(entry, index, window, cx);
                    let separator = if index > 0 {
                        Some(ListSeparator.into_any_element())
                    } else {
                        None
                    };
                    separator
                        .into_iter()
                        .chain(std::iter::once(element.into_any_element()))
                })
                .collect();
            div().children(entry_elements).into_any_element()
        };

        v_flex()
            .id("ssh-panel")
            .key_context("SshPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(panel_bg)
            .child(header)
            .child(content)
    }
}
