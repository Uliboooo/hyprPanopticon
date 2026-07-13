//! One-shot snapshot of Hyprland state via IPC: focused monitor, its normal
//! workspaces, and every mapped client with monitor-relative geometry.

use anyhow::{Context, Result};
use hyprland::data::{Clients, Monitors, Workspaces};
use hyprland::shared::{HyprData, HyprDataVec};

use crate::ipc::parse_address;
use crate::model::{MonitorModel, Rect, Snapshot, WindowThumb, WorkspaceModel};

pub fn take() -> Result<Snapshot> {
    let monitors = Monitors::get().context("hyprctl monitors")?.to_vec();
    let workspaces = Workspaces::get().context("hyprctl workspaces")?.to_vec();
    let clients = Clients::get().context("hyprctl clients")?.to_vec();

    let monitor = monitors
        .iter()
        .find(|m| m.focused)
        .or_else(|| monitors.first())
        .context("no monitors reported by Hyprland")?;

    let scale = monitor.scale as f64;
    let mon = MonitorModel {
        x: monitor.x as f64,
        y: monitor.y as f64,
        w: monitor.width as f64 / scale,
        h: monitor.height as f64 / scale,
        scale,
    };

    let mut ws_models: Vec<WorkspaceModel> = Vec::new();
    let mut special_models: Vec<WorkspaceModel> = Vec::new();
    for w in workspaces {
        if w.monitor != monitor.name {
            continue;
        }
        if w.id > 0 {
            ws_models.push(WorkspaceModel { id: w.id, name: w.name, windows: Vec::new() });
        } else if w.id < 0 {
            let name = w.name.strip_prefix("special:").unwrap_or(&w.name).to_string();
            special_models.push(WorkspaceModel { id: w.id, name, windows: Vec::new() });
        }
    }
    ws_models.sort_by_key(|w| w.id);
    special_models.sort_by_key(|w| w.id);

    for c in clients {
        if !c.mapped {
            continue;
        }
        let Some(ws) = ws_models
            .iter_mut()
            .chain(special_models.iter_mut())
            .find(|w| w.id == c.workspace.id)
        else {
            continue;
        };
        let Some(addr) = parse_address(&c.address.to_string()) else {
            continue;
        };
        ws.windows.push(WindowThumb {
            addr,
            rect: Rect {
                x: c.at.0 as f64 - mon.x,
                y: c.at.1 as f64 - mon.y,
                w: c.size.0 as f64,
                h: c.size.1 as f64,
            },
            class: c.class,
            title: c.title,
            texture: None,
            y_invert: false,
            focus_order: c.focus_history_id.max(0) as usize,
        });
    }

    // Draw least-recently-focused first so the most recent window ends up on top.
    for ws in ws_models.iter_mut().chain(special_models.iter_mut()) {
        ws.windows.sort_by_key(|w| std::cmp::Reverse(w.focus_order));
    }

    Ok(Snapshot {
        monitor: mon,
        monitor_name: monitor.name.clone(),
        active_workspace: monitor.active_workspace.id,
        workspaces: ws_models,
        specials: special_models,
    })
}
