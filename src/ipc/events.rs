//! Hyprland event socket → refresh notifications for the UI.
//!
//! Runs the blocking `EventListener` on its own thread and forwards a unit
//! signal whenever something changed that could affect the overview. The UI
//! drains the channel and re-takes a full IPC snapshot.

use hyprland::event_listener::EventListener;

pub fn spawn() -> async_channel::Receiver<()> {
    let (tx, rx) = async_channel::unbounded::<()>();

    std::thread::Builder::new()
        .name("hypr-events".into())
        .spawn(move || {
            let mut listener = EventListener::new();

            macro_rules! notify {
                ($add:ident) => {{
                    let tx = tx.clone();
                    listener.$add(move |_| {
                        let _ = tx.send_blocking(());
                    });
                }};
            }

            notify!(add_workspace_added_handler);
            notify!(add_workspace_deleted_handler);
            notify!(add_workspace_moved_handler);
            notify!(add_workspace_renamed_handler);
            notify!(add_window_opened_handler);
            notify!(add_window_closed_handler);
            notify!(add_window_moved_handler);
            notify!(add_window_title_changed_handler);
            notify!(add_fullscreen_state_changed_handler);

            if let Err(e) = listener.start_listener() {
                eprintln!("hyprPanopticon: event listener stopped: {e}");
            }
        })
        .expect("spawn hypr-events thread");

    rx
}
