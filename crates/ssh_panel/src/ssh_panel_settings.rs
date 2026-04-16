use gpui::Pixels;
use settings::{DockSide, RegisterSetting, Settings};

#[derive(Debug, Clone, Copy, PartialEq, RegisterSetting)]
pub struct SshPanelSettings {
    pub button: bool,
    pub default_width: Pixels,
    pub dock: DockSide,
}

impl Settings for SshPanelSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let panel = content.ssh_panel.as_ref().unwrap();
        Self {
            button: panel.button.unwrap(),
            default_width: panel.default_width.map(gpui::px).unwrap(),
            dock: panel.dock.unwrap(),
        }
    }
}
