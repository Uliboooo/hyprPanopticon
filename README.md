# hyprPanopticon

[日本語 README はこちら](README.ja.md)

A Hyprland workspace overview. When launched, it displays the workspaces open
in Hyprland arranged along a circle. The focused workspace is shown at maximum
size while the other previews are automatically scaled down so the entire
layout stays visible.

Previews are **live thumbnails**: every window is captured through Hyprland's
`hyprland-toplevel-export-v1` protocol (this works for windows on non-visible
workspaces too) and composed according to the real window geometry. The
ring-focused workspace is re-captured continuously while the overlay is open,
and Hyprland events (windows opening/closing/moving) refresh the overview
live.

## Usage

Launch `hyprpanopticon` (typically from a Hyprland keybinding). The overlay
covers the focused monitor.

| Input | Action |
|---|---|
| `←` `↑` / `h` `k` | rotate focus counter-clockwise |
| `→` `↓` / `l` `j` | rotate focus clockwise |
| mouse wheel | rotate focus |
| `Enter` / `Space` | switch to the focused workspace and close |
| click a preview | switch to that workspace and close |
| `1`–`9` | toggle the numbered special workspace |
| `Esc` / `q` | close without switching |

Special workspaces (scratchpads) are shown as a numbered column outside the
ring on the left edge; click one or press its number to toggle it. Windows that extend beyond the
monitor viewport (e.g. with a scrolling layout) are clipped to what the
monitor would show.

## Installation (Nix)

Try it without installing:

```sh
nix run github:Uliboooo/hyprPanopticon
```

### As a flake input (e.g. NixOS / home-manager)

Add it to your flake's inputs:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    hyprpanopticon = {
      url = "github:Uliboooo/hyprPanopticon";
      # optional: share your nixpkgs instead of pulling a second copy
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, hyprpanopticon, ... }: {
    # ... your outputs
  };
}
```

Then reference `hyprpanopticon.packages.${system}.default` wherever you build
your package list. With home-manager:

```nix
{ pkgs, inputs, ... }:
{
  home.packages = [
    inputs.hyprpanopticon.packages.${pkgs.system}.default
  ];
}
```

Or with a NixOS module:

```nix
{ pkgs, inputs, ... }:
{
  environment.systemPackages = [
    inputs.hyprpanopticon.packages.${pkgs.system}.default
  ];
}
```

Supported systems: `x86_64-linux`, `aarch64-linux`.

Example Hyprland config:

```conf
bind = SUPER, Tab, exec, hyprpanopticon

# optional: blur the overlay backdrop
layerrule = blur, hyprPanopticon
layerrule = ignorealpha 0.2, hyprPanopticon
```

## Configuration

Optional, at `~/.config/hyprpanopticon/config.toml`. All keys are optional;
out-of-range values are clamped:

```toml
# Scale of the preview opposite the focus (0.05..1.0, default 0.45).
min_scale = 0.45
# How fast previews shrink away from the focus (0.1..10, default 2.0).
falloff = 2.0
# Focused preview width as a fraction of the screen (0.1..0.8, default 0.34).
focus_width = 0.34
# Screen-edge margin in px (0..200, default 24).
margin = 24
# Angular density (0..1, default 0.7): 0 spaces previews evenly around the
# circle; higher values give the top more room and pack the small previews
# together at the bottom.
spread = 0.7
# Pull small side previews horizontally toward the center (0..1, default
# 0.4): 0 keeps everything on one circle (hollow middle), higher fills the
# center while the top and bottom previews stay put.
center_pull = 0.4
```

## Building from source

```sh
nix develop        # dev shell with rustc, cargo, GTK4, gtk4-layer-shell
cargo build
cargo test         # layout math unit tests
```

Debug helper: `hyprpanopticon --dump-window 0xADDR [out.png]` captures a
single toplevel (address from `hyprctl clients`) to a PNG.

## Architecture

- `src/layout.rs` — pure circular-layout math (angle, cosine scale falloff,
  radius fitting); unit-tested.
- `src/ipc/` — Hyprland IPC: one-shot snapshot (monitors/workspaces/clients)
  and the event listener that triggers live refreshes.
- `src/capture/` — a worker thread with its own Wayland connection speaking
  `hyprland-toplevel-export-v1`; sequential captures into `wl_shm` buffers,
  delivered to the UI as bytes and turned into `gdk::MemoryTexture`s on the
  main thread.
- `src/ui/` — GTK4 widgets: the layer-shell overlay window, the `RingView`
  container (circle layout + rotation animation), and `WorkspacePreview`
  (composes window textures, falls back to colored rectangles until pixels
  arrive).
