//! Optional user configuration from
//! `$XDG_CONFIG_HOME/hyprpanopticon/config.toml` (usually
//! `~/.config/hyprpanopticon/config.toml`). Every key is optional; missing
//! keys keep the built-in defaults. Values are clamped to sane ranges.

use serde::Deserialize;

use crate::layout::RingParams;

#[derive(Debug, Default, Clone, Copy)]
pub struct Config {
    pub ring: RingParams,
    /// Show only the focused monitor's workspaces instead of every monitor's.
    pub per_monitor_workspaces: bool,
    /// Overlay each ring preview with its workspace name/index badge.
    pub show_workspace_index: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    /// Scale of the preview opposite the focus (0.05..=1.0).
    min_scale: Option<f64>,
    /// Scale falloff sharpness (0.1..=10.0). Higher shrinks neighbors faster.
    falloff: Option<f64>,
    /// Focused preview width as a fraction of screen width (0.1..=0.8).
    focus_width: Option<f64>,
    /// Screen-edge margin in px (0..=200).
    margin: Option<f64>,
    /// Angular density warp (0.0..=1.0): 0 = uniform ring, higher = more
    /// room at the top, small previews packed at the bottom.
    spread: Option<f64>,
    /// Pull small side previews horizontally toward the center (0.0..=1.0):
    /// 0 = all on one circle (hollow middle), higher = center filled.
    center_pull: Option<f64>,
    /// true = only the focused monitor's workspaces; false (default) = all.
    per_monitor_workspaces: Option<bool>,
    /// true = badge each ring preview with its workspace index; false (default) = no badge.
    show_workspace_index: Option<bool>,
}

fn config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    Some(base.join("hyprpanopticon/config.toml"))
}

pub fn load() -> Config {
    let mut config = Config::default();
    let Some(path) = config_path() else { return config };
    let Ok(text) = std::fs::read_to_string(&path) else { return config };
    match toml::from_str::<FileConfig>(&text) {
        Ok(file) => apply(&mut config, &file),
        Err(e) => eprintln!("hyprPanopticon: ignoring bad {}: {e}", path.display()),
    }
    config
}

fn apply(config: &mut Config, file: &FileConfig) {
    let params = &mut config.ring;
    if let Some(v) = file.min_scale {
        params.s_min = v.clamp(0.05, 1.0);
    }
    if let Some(v) = file.falloff {
        params.falloff = v.clamp(0.1, 10.0);
    }
    if let Some(v) = file.focus_width {
        params.focus_width_frac = v.clamp(0.1, 0.8);
    }
    if let Some(v) = file.margin {
        params.margin = v.clamp(0.0, 200.0);
    }
    if let Some(v) = file.spread {
        params.spread = v.clamp(0.0, 1.0);
    }
    if let Some(v) = file.center_pull {
        params.center_pull = v.clamp(0.0, 1.0);
    }
    if let Some(v) = file.per_monitor_workspaces {
        config.per_monitor_workspaces = v;
    }
    if let Some(v) = file.show_workspace_index {
        config.show_workspace_index = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_clamps() {
        let file: FileConfig =
            toml::from_str("min_scale = 0.5\nspread = 3.0\nfalloff = 1.5").unwrap();
        let mut config = Config::default();
        apply(&mut config, &file);
        assert_eq!(config.ring.s_min, 0.5);
        assert_eq!(config.ring.spread, 1.0); // clamped
        assert_eq!(config.ring.falloff, 1.5);
        // untouched keys keep defaults
        assert_eq!(config.ring.margin, RingParams::default().margin);
        assert!(!config.per_monitor_workspaces);
    }

    #[test]
    fn show_workspace_index_defaults_off_and_parses() {
        let mut config = Config::default();
        apply(&mut config, &toml::from_str("").unwrap());
        assert!(!config.show_workspace_index);
        apply(&mut config, &toml::from_str("show_workspace_index = true").unwrap());
        assert!(config.show_workspace_index);
    }

    #[test]
    fn per_monitor_workspaces_defaults_off_and_parses() {
        let mut config = Config::default();
        apply(&mut config, &toml::from_str("").unwrap());
        assert!(!config.per_monitor_workspaces);
        apply(&mut config, &toml::from_str("per_monitor_workspaces = true").unwrap());
        assert!(config.per_monitor_workspaces);
    }

    #[test]
    fn empty_and_unknown_keys_are_fine() {
        assert!(toml::from_str::<FileConfig>("").is_ok());
        // Unknown keys are ignored rather than fatal.
        assert!(toml::from_str::<FileConfig>("nonsense = true").is_ok());
    }
}
