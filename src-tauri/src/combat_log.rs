use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatEvent {
    pub timestamp: f64,
    pub event_type: String,
    pub source: Option<String>,
    pub target: Option<String>,
}

struct WatchState {
    handle: Option<JoinHandle<()>>,
    start_time: Instant,
}

lazy_static::lazy_static! {
    static ref WATCH_STATE: Arc<Mutex<Option<WatchState>>> = Arc::new(Mutex::new(None));
}

#[tauri::command]
pub async fn start_combat_watch(app_handle: AppHandle) -> Result<(), String> {
    let mut state = WATCH_STATE.lock().map_err(|e| e.to_string())?;

    if state.is_some() {
        return Err("Combat watch already running".to_string());
    }

    let start_time = Instant::now();
    let app_handle_clone = app_handle.clone();

    let handle = tokio::spawn(async move {
        let player_names = vec!["Thrallmaster", "Jainaproud", "Sylvanaswind", "Arthaslight"];
        let enemy_names = vec!["Orcwarrior", "Undeadrogue", "Taurenshaman", "Trollhunter"];

        loop {
            tokio::time::sleep(Duration::from_secs(rand::random::<u64>() % 10 + 5)).await;

            let elapsed = start_time.elapsed().as_secs_f64();

            let event_type = if rand::random::<bool>() {
                "PARTY_KILL"
            } else {
                "UNIT_DIED"
            };

            let (source, target) = if event_type == "PARTY_KILL" {
                (
                    Some(player_names[rand::random::<usize>() % player_names.len()].to_string()),
                    Some(enemy_names[rand::random::<usize>() % enemy_names.len()].to_string()),
                )
            } else {
                (
                    Some(enemy_names[rand::random::<usize>() % enemy_names.len()].to_string()),
                    Some(player_names[rand::random::<usize>() % player_names.len()].to_string()),
                )
            };

            let event = CombatEvent {
                timestamp: elapsed,
                event_type: event_type.to_string(),
                source,
                target,
            };

            let _ = app_handle_clone.emit("combat-event", &event);
        }
    });

    *state = Some(WatchState {
        handle: Some(handle),
        start_time,
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_combat_watch() -> Result<(), String> {
    let mut state = WATCH_STATE.lock().map_err(|e| e.to_string())?;

    if let Some(watch_state) = state.take() {
        if let Some(handle) = watch_state.handle {
            handle.abort();
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn emit_manual_marker(app_handle: AppHandle) -> Result<(), String> {
    let state = WATCH_STATE.lock().map_err(|e| e.to_string())?;

    if let Some(watch_state) = state.as_ref() {
        let elapsed = watch_state.start_time.elapsed().as_secs_f64();

        let event = CombatEvent {
            timestamp: elapsed,
            event_type: "MANUAL_MARKER".to_string(),
            source: None,
            target: None,
        };

        let _ = app_handle.emit("combat-event", &event);
        Ok(())
    } else {
        Err("Combat watch not running".to_string())
    }
}
