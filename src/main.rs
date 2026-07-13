mod capture;
mod ipc;
mod layout;
mod model;
mod ui;

use gtk4 as gtk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    // Layer-shell requires the Wayland GDK backend; never fall back to X11.
    std::env::set_var("GDK_BACKEND", "wayland");

    // Hidden debug mode: `hyprpanopticon --dump-window 0xADDR [out.png]`
    // captures one toplevel and writes it to a PNG.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--dump-window") {
        let addr = args
            .get(i + 1)
            .and_then(|s| ipc::parse_address(s))
            .expect("usage: --dump-window 0xADDR [out.png]");
        let out = args.get(i + 2).cloned().unwrap_or_else(|| "window.png".into());
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

    let app = gtk::Application::builder()
        .application_id("dev.seli.hyprPanopticon")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(|app| {
        let snapshot = match ipc::snapshot::take() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("hyprPanopticon: failed to query Hyprland: {e:#}");
                app.quit();
                return;
            }
        };
        if snapshot.workspaces.is_empty() {
            eprintln!("hyprPanopticon: no workspaces on the focused monitor");
            app.quit();
            return;
        }
        let window = ui::overlay::build(app, &snapshot);
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
