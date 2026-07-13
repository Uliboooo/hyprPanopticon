//! One-shot snapshot of Hyprland state via IPC: monitors, normal workspaces,
//! and every mapped client with monitor-relative geometry. By default the
//! snapshot covers every monitor's workspaces; `per_monitor` restricts it to
//! the focused monitor.

use std::collections::HashMap;

use anyhow::{Context, Result};
use hyprland::data::{Clients, Monitors, Workspaces};
use hyprland::shared::{HyprData, HyprDataVec};

use crate::ipc::parse_address;
use crate::model::{MonitorModel, Rect, Snapshot, WindowThumb, WorkspaceModel};

pub fn take(per_monitor: bool) -> Result<Snapshot> {
    // Three independent socket round trips; overlap them so the snapshot
    // costs one round trip instead of three.
    let (monitors, workspaces, clients) = std::thread::scope(|s| {
        let m = s.spawn(|| Monitors::get().map(HyprDataVec::to_vec));
        let w = s.spawn(|| Workspaces::get().map(HyprDataVec::to_vec));
        let c = s.spawn(|| Clients::get().map(HyprDataVec::to_vec));
        (m.join().unwrap(), w.join().unwrap(), c.join().unwrap())
    });
    let monitors = monitors.context("hyprctl monitors")?;
    let workspaces = workspaces.context("hyprctl workspaces")?;
    let clients = clients.context("hyprctl clients")?;

    let monitor = monitors
        .iter()
        .find(|m| m.focused)
        .or_else(|| monitors.first())
        .context("no monitors reported by Hyprland")?;

    // Origin and logical size per monitor, for monitor-relative window rects.
    let mon_models: HashMap<&str, MonitorModel> = monitors
        .iter()
        .map(|m| {
            let scale = m.scale as f64;
            let model = MonitorModel {
                x: m.x as f64,
                y: m.y as f64,
                w: m.width as f64 / scale,
                h: m.height as f64 / scale,
                scale,
            };
            (m.name.as_str(), model)
        })
        .collect();
    let mon = mon_models[monitor.name.as_str()];

    let mut ws_models: Vec<WorkspaceModel> = Vec::new();
    let mut special_models: Vec<WorkspaceModel> = Vec::new();
    // Monitor origin per workspace id, to translate its windows below.
    let mut ws_origin: HashMap<i32, (f64, f64)> = HashMap::new();
    for w in workspaces {
        if per_monitor && w.monitor != monitor.name {
            continue;
        }
        let ws_mon = mon_models.get(w.monitor.as_str()).unwrap_or(&mon);
        ws_origin.insert(w.id, (ws_mon.x, ws_mon.y));
        let viewport = (ws_mon.w, ws_mon.h);
        if w.id > 0 {
            ws_models.push(WorkspaceModel { id: w.id, name: w.name, windows: Vec::new(), viewport });
        } else if w.id < 0 {
            let name = w.name.strip_prefix("special:").unwrap_or(&w.name).to_string();
            special_models.push(WorkspaceModel { id: w.id, name, windows: Vec::new(), viewport });
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
        let (ox, oy) = ws_origin[&ws.id];
        ws.windows.push(WindowThumb {
            addr,
            rect: Rect {
                x: c.at.0 as f64 - ox,
                y: c.at.1 as f64 - oy,
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
