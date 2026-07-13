# hyprPanopticon

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
| `Esc` / `q` | close without switching |

Special workspaces (scratchpads) are not shown. Windows that extend beyond the
monitor viewport (e.g. with a scrolling layout) are clipped to what the
monitor would show.

## Installation (Nix)

```sh
nix run github:<you>/hyprPanopticon
# or in a flake: inputs.hyprpanopticon.url = "github:<you>/hyprPanopticon";
```

Example Hyprland config:

```conf
bind = SUPER, Tab, exec, hyprpanopticon

# optional: blur the overlay backdrop
layerrule = blur, hyprPanopticon
layerrule = ignorealpha 0.2, hyprPanopticon
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
