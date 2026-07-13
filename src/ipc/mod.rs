pub mod events;
pub mod snapshot;

use anyhow::Context;

/// Send one raw command to Hyprland's command socket and return the reply.
fn send_command(cmd: &str) -> anyhow::Result<String> {
    use std::io::{Read, Write};
    let runtime = std::env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR not set")?;
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .context("HYPRLAND_INSTANCE_SIGNATURE not set (not running under Hyprland?)")?;
    let path = format!("{runtime}/hypr/{sig}/.socket.sock");
    let mut stream = std::os::unix::net::UnixStream::connect(&path)
        .with_context(|| format!("connect {path}"))?;
    stream.write_all(cmd.as_bytes())?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// Switch to a workspace. Tries the classic dispatcher syntax first, then the
/// Lua-config syntax (`hl.dsp.focus({ workspace = N })`) used by Hyprland
/// setups with a Lua IPC layer.
pub fn switch_workspace(id: i32) -> anyhow::Result<()> {
    let classic = send_command(&format!("dispatch workspace {id}"))?;
    if classic.trim() == "ok" {
        return Ok(());
    }
    let lua = send_command(&format!("dispatch hl.dsp.focus({{ workspace = {id} }})"))?;
    if lua.trim() == "ok" {
        return Ok(());
    }
    anyhow::bail!("workspace dispatch rejected: {}", lua.trim())
}

/// Toggle a special workspace by its short name (without "special:").
pub fn toggle_special(name: &str) -> anyhow::Result<()> {
    let classic = send_command(&format!("dispatch togglespecialworkspace {name}"))?;
    if classic.trim() == "ok" {
        return Ok(());
    }
    let lua = send_command(&format!(
        "dispatch hl.dsp.workspace.toggle_special(\"{name}\")"
    ))?;
    if lua.trim() == "ok" {
        return Ok(());
    }
    anyhow::bail!("special workspace dispatch rejected: {}", lua.trim())
}

/// Parse a Hyprland window address ("0x5693…") into its numeric form.
pub fn parse_address(s: &str) -> Option<u64> {
    let hex = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_addresses() {
        assert_eq!(super::parse_address("0x5693171ffe00"), Some(0x5693171ffe00));
        assert_eq!(super::parse_address("5693171ffe00"), Some(0x5693171ffe00));
        assert_eq!(super::parse_address("nope"), None);
    }
}
