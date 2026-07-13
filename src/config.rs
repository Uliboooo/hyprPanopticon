//! Optional user configuration from
//! `$XDG_CONFIG_HOME/hyprpanopticon/config.toml` (usually
//! `~/.config/hyprpanopticon/config.toml`). Every key is optional; missing
//! keys keep the built-in defaults. Values are clamped to sane ranges.

use serde::Deserialize;

use crate::layout::RingParams;

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
}

fn config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    Some(base.join("hyprpanopticon/config.toml"))
}

pub fn load() -> RingParams {
    let mut params = RingParams::default();
    let Some(path) = config_path() else { return params };
    let Ok(text) = std::fs::read_to_string(&path) else { return params };
    match toml::from_str::<FileConfig>(&text) {
        Ok(config) => apply(&mut params, &config),
        Err(e) => eprintln!("hyprPanopticon: ignoring bad {}: {e}", path.display()),
    }
    params
}

fn apply(params: &mut RingParams, config: &FileConfig) {
    if let Some(v) = config.min_scale {
        params.s_min = v.clamp(0.05, 1.0);
    }
    if let Some(v) = config.falloff {
        params.falloff = v.clamp(0.1, 10.0);
    }
    if let Some(v) = config.focus_width {
        params.focus_width_frac = v.clamp(0.1, 0.8);
    }
    if let Some(v) = config.margin {
        params.margin = v.clamp(0.0, 200.0);
    }
    if let Some(v) = config.spread {
        params.spread = v.clamp(0.0, 1.0);
    }
    if let Some(v) = config.center_pull {
        params.center_pull = v.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_clamps() {
        let config: FileConfig =
            toml::from_str("min_scale = 0.5\nspread = 3.0\nfalloff = 1.5").unwrap();
        let mut params = RingParams::default();
        apply(&mut params, &config);
        assert_eq!(params.s_min, 0.5);
        assert_eq!(params.spread, 1.0); // clamped
        assert_eq!(params.falloff, 1.5);
        // untouched keys keep defaults
        assert_eq!(params.margin, RingParams::default().margin);
    }

    #[test]
    fn empty_and_unknown_keys_are_fine() {
        assert!(toml::from_str::<FileConfig>("").is_ok());
        // Unknown keys are ignored rather than fatal.
        assert!(toml::from_str::<FileConfig>("nonsense = true").is_ok());
    }
}
