use gpui::{Action, Context, Render, Window};
use ui::prelude::*;
use ui::{IconButton, IconButtonShape, IconSize, Tooltip};
use workspace::{ItemHandle, StatusItemView};
use zed_actions::ssh_panel::ToggleFocus;

pub struct SshStatusButton;

impl SshStatusButton {
    pub fn new() -> Self {
        Self
    }
}

impl Render for SshStatusButton {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("ssh-status-button", IconName::Server)
            .shape(IconButtonShape::Square)
            .icon_size(IconSize::Small)
            .tooltip(|_window, cx| Tooltip::for_action("SSH Servers", &ToggleFocus, cx))
            .on_click(|_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx);
            })
    }
}

impl StatusItemView for SshStatusButton {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}
