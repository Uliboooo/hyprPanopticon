mod capture;
mod config;
mod ipc;
mod layout;
mod model;
mod ui;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

const HELP: &str = concat!(
    "hyprpanopticon ",
    env!("CARGO_PKG_VERSION"),
    " — circular workspace overview overlay for Hyprland

USAGE:
    hyprpanopticon [OPTIONS]

Run with no arguments to open the overlay on the focused monitor.

OPTIONS:
    -h, --help                   Print this help and exit
    -V, --version                Print the version and exit
        --switch-workspace <N>   Debug: dispatch a switch to workspace N, no UI
        --dump-window <0xADDR> [OUT.png]
                                 Debug: capture one window (address from
                                 `hyprctl clients`) to a PNG (default window.png)

KEYS (while the overlay is open):
    Left/Up/h/k       rotate the ring backward
    Right/Down/l/j    rotate the ring forward (mouse wheel also rotates)
    Enter/Space       switch to the focused workspace and close
    1-9               toggle the numbered special workspace
    Esc/q             close without switching
    click a preview   switch to that workspace and close

CONFIGURATION:
    Optional TOML file at ~/.config/hyprpanopticon/config.toml
    ($XDG_CONFIG_HOME is honored). All keys are optional; out-of-range
    values are clamped:

        min_scale = 0.45               # smallest preview scale (0.05..1)
        falloff = 2.0                  # shrink speed away from focus (0.1..10)
        focus_width = 0.34             # focused width as screen fraction (0.1..0.8)
        margin = 24                    # screen-edge margin in px (0..200)
        spread = 0.7                   # angular density (0..1)
        center_pull = 0.4              # pull side previews inward (0..1)
        per_monitor_workspaces = false # true = focused monitor's workspaces only

    See the README for details on each key.
"
);

fn main() -> glib::ExitCode {
    // Layer-shell requires the Wayland GDK backend; never fall back to X11.
    std::env::set_var("GDK_BACKEND", "wayland");

    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("hyprpanopticon {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return glib::ExitCode::SUCCESS;
    }

    // Hidden debug mode: `hyprpanopticon --switch-workspace N` exercises the
    // workspace dispatch path without the UI.
    if let Some(i) = args.iter().position(|a| a == "--switch-workspace") {
        let id: i32 = args
            .get(i + 1)
            .and_then(|s| s.parse().ok())
            .expect("usage: --switch-workspace N");
        return match ipc::switch_workspace(id) {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("hyprPanopticon: {e:#}");
                glib::ExitCode::FAILURE
            }
        };
    }

    // Hidden debug mode: `hyprpanopticon --dump-window 0xADDR [out.png]`
    // captures one toplevel and writes it to a PNG.
    if let Some(i) = args.iter().position(|a| a == "--dump-window") {
        let addr = args
            .get(i + 1)
            .and_then(|s| ipc::parse_address(s))
            .expect("usage: --dump-window 0xADDR [out.png]");
        let out = args
            .get(i + 2)
            .cloned()
            .unwrap_or_else(|| "window.png".into());
        return match dump_window(addr, &out) {
            Ok(()) => {
                println!("wrote {out}");
                glib::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("hyprPanopticon: {e:#}");
                glib::ExitCode::FAILURE
            }
        };
    }

    // Query Hyprland while GTK starts up; the IPC round trips and GTK/Wayland
    // init otherwise serialize and both sit on the path to the first frame.
    let config = config::load();
    let per_monitor = config.per_monitor_workspaces;
    let early_snapshot = std::cell::RefCell::new(Some(std::thread::spawn(move || {
        ipc::snapshot::take(per_monitor)
    })));

    let app = gtk::Application::builder()
        .application_id("dev.seli.hyprPanopticon")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        let result = early_snapshot
            .borrow_mut()
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or_else(|| ipc::snapshot::take(per_monitor));
        let snapshot = match result {
            Ok(s) => s,
            Err(e) => {
                eprintln!("hyprPanopticon: failed to query Hyprland: {e:#}");
                app.quit();
                return;
            }
        };
        if snapshot.workspaces.is_empty() {
            eprintln!("hyprPanopticon: no workspaces to show");
            app.quit();
            return;
        }
        let window = ui::overlay::build(app, &snapshot, config);
        window.present();
    });

    // GTK must not see our own CLI arguments.
    app.run_with_args::<&str>(&[])
}

fn dump_window(addr: u64, out: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    gtk::init().context("gtk init")?;
    let mut engine = capture::engine::Engine::new()?;
    let frame = engine.capture(addr)?;
    let texture = capture::frame_to_texture(&frame)
        .with_context(|| format!("unsupported pixel format {:?}", frame.format))?;
    texture.save_to_png(out).context("write png")?;
    Ok(())
}
