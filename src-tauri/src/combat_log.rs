use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatEvent {
    pub timestamp: f64,
    pub event_type: String,
    pub source: Option<String>,
    pub target: Option<String>,
}

const MAX_DEBUG_EVENTS: usize = 2_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCombatEvent {
    pub line_number: u64,
    pub log_timestamp: String,
    pub event_type: String,
    pub source: Option<String>,
    pub target: Option<String>,
    pub target_kind: Option<String>,
    pub zone_name: Option<String>,
    pub encounter_name: Option<String>,
    pub encounter_category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseCombatLogDebugResult {
    pub file_path: String,
    pub file_size_bytes: u64,
    pub total_lines: u64,
    pub parsed_events: Vec<ParsedCombatEvent>,
    pub event_counts: BTreeMap<String, u64>,
    pub truncated: bool,
}

struct WatchState {
    handle: Option<JoinHandle<()>>,
    start_time: Instant,
}

lazy_static::lazy_static! {
    static ref WATCH_STATE: Arc<Mutex<Option<WatchState>>> = Arc::new(Mutex::new(None));
}

#[tauri::command]
pub async fn start_combat_watch(app_handle: AppHandle, wow_folder: String) -> Result<(), String> {
    let mut state = WATCH_STATE.lock().map_err(|error| error.to_string())?;

    if state.is_some() {
        return Err("Combat watch already running".to_string());
    }

    let log_path = build_combat_log_path(&wow_folder);
    if !log_path.is_file() {
        return Err(format!(
            "WoW combat log file not found at '{}'. Expected '{}'.",
            wow_folder,
            log_path.to_string_lossy()
        ));
    }

    let initial_offset = std::fs::metadata(&log_path)
        .map_err(|error| error.to_string())?
        .len();

    let app_handle_clone = app_handle.clone();
    let log_path_clone = log_path.clone();
    let start_time = Instant::now();

    let handle = tokio::spawn(async move {
        if let Err(error) = watch_combat_log(
            app_handle_clone,
            &log_path_clone,
            initial_offset,
            start_time,
        )
        .await
        {
            tracing::error!("Combat log watcher stopped: {error}");
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
    let mut state = WATCH_STATE.lock().map_err(|error| error.to_string())?;

    if let Some(watch_state) = state.take() {
        if let Some(handle) = watch_state.handle {
            handle.abort();
        }
    }

    Ok(())
}

#[tauri::command]
pub fn validate_wow_folder(path: String) -> bool {
    if path.trim().is_empty() {
        return false;
    }

    build_combat_log_path(&path).is_file()
}

#[tauri::command]
pub async fn emit_manual_marker(app_handle: AppHandle) -> Result<(), String> {
    let state = WATCH_STATE.lock().map_err(|error| error.to_string())?;

    if let Some(watch_state) = state.as_ref() {
        let elapsed = watch_state.start_time.elapsed().as_secs_f64();
        let event = CombatEvent {
            timestamp: elapsed,
            event_type: "MANUAL_MARKER".to_string(),
            source: None,
            target: None,
        };

        let _ = app_handle.emit("combat-event", &event);
        return Ok(());
    }

    Err("Combat watch not running".to_string())
}

#[tauri::command]
pub fn parse_combat_log_file(file_path: String) -> Result<ParseCombatLogDebugResult, String> {
    if !cfg!(debug_assertions) {
        return Err("Combat log debug parsing is only available in debug builds".to_string());
    }

    if file_path.trim().is_empty() {
        return Err("Combat log file path is required".to_string());
    }

    let path = Path::new(&file_path);
    if !path.is_file() {
        return Err(format!("Combat log file not found: {}", file_path));
    }

    let file_size_bytes = std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .len();
    let file = File::open(path).map_err(|error| error.to_string())?;
    let reader = BufReader::new(file);

    let mut total_lines = 0_u64;
    let mut parsed_events: Vec<ParsedCombatEvent> = Vec::new();
    let mut event_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut truncated = false;
    let mut debug_context = DebugParseContext::default();

    for line_result in reader.lines() {
        let line = line_result.map_err(|error| error.to_string())?;
        total_lines += 1;

        if let Some(parsed_event) = parse_important_log_line(&line, total_lines, &mut debug_context) {
            *event_counts
                .entry(parsed_event.event_type.clone())
                .or_insert(0) += 1;
            if parsed_events.len() < MAX_DEBUG_EVENTS {
                parsed_events.push(parsed_event);
            } else {
                truncated = true;
            }
        }
    }

    Ok(ParseCombatLogDebugResult {
        file_path,
        file_size_bytes,
        total_lines,
        parsed_events,
        event_counts,
        truncated,
    })
}

fn build_combat_log_path(wow_folder: &str) -> PathBuf {
    Path::new(wow_folder).join("Logs").join("WoWCombatLog.txt")
}

async fn watch_combat_log(
    app_handle: AppHandle,
    log_path: &Path,
    initial_offset: u64,
    start_time: Instant,
) -> Result<(), String> {
    let (notify_sender, mut notify_receiver) =
        mpsc::unbounded_channel::<Result<Event, notify::Error>>();

    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = notify_sender.send(result);
    })
    .map_err(|error| error.to_string())?;

    let watch_directory = log_path
        .parent()
        .ok_or_else(|| "Invalid WoW combat log path".to_string())?;
    watcher
        .watch(watch_directory, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())?;

    let mut file_offset = initial_offset;
    while let Some(notification_result) = notify_receiver.recv().await {
        match notification_result {
            Ok(event) => {
                if !is_relevant_notification(&event, log_path) {
                    continue;
                }

                if let Err(error) =
                    read_and_emit_new_events(&app_handle, log_path, &mut file_offset, start_time)
                {
                    tracing::warn!("Failed to parse combat log update: {error}");
                }
            }
            Err(error) => {
                tracing::warn!("Combat log watcher error: {error}");
            }
        }
    }

    Ok(())
}

fn is_relevant_notification(event: &Event, log_path: &Path) -> bool {
    let relevant_kind = matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_));
    if !relevant_kind {
        return false;
    }

    let Some(log_file_name) = log_path.file_name() else {
        return false;
    };

    event.paths.iter().any(|path| {
        path == log_path
            || path
                .file_name()
                .map(|file_name| file_name == log_file_name)
                .unwrap_or(false)
    })
}

fn read_and_emit_new_events(
    app_handle: &AppHandle,
    log_path: &Path,
    file_offset: &mut u64,
    start_time: Instant,
) -> Result<(), String> {
    let mut file = File::open(log_path).map_err(|error| error.to_string())?;
    let file_length = file.metadata().map_err(|error| error.to_string())?.len();

    if file_length < *file_offset {
        *file_offset = 0;
    }

    file.seek(SeekFrom::Start(*file_offset))
        .map_err(|error| error.to_string())?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if bytes_read == 0 {
            break;
        }

        *file_offset = file_offset.saturating_add(bytes_read as u64);
        if let Some(event) = parse_combat_log_line(&line, start_time.elapsed().as_secs_f64()) {
            let _ = app_handle.emit("combat-event", event);
        }
    }

    Ok(())
}

fn parse_combat_log_line(line: &str, elapsed_seconds: f64) -> Option<CombatEvent> {
    let parsed_line = parse_log_line_fields(line)?;
    if is_guardian_target(parsed_line.target_kind.as_deref()) {
        return None;
    }

    let normalized_event_type = match parsed_line.normalized_event_type.as_str() {
        "PARTY_KILL" => "PARTY_KILL",
        "UNIT_DIED" => "UNIT_DIED",
        _ => return None,
    };

    Some(CombatEvent {
        timestamp: elapsed_seconds,
        event_type: normalized_event_type.to_string(),
        source: parsed_line.source,
        target: parsed_line.target,
    })
}

fn parse_important_log_line(
    line: &str,
    line_number: u64,
    context: &mut DebugParseContext,
) -> Option<ParsedCombatEvent> {
    let parsed_line = parse_log_line_fields(line)?;

    update_debug_context(context, &parsed_line);

    if is_context_only_event(&parsed_line.raw_event_type) {
        return None;
    }

    if let Some(zone_name) = extract_zone_name(&parsed_line.raw_event_type, &parsed_line.fields) {
        context.current_zone = Some(zone_name);
    }

    let mut encounter_name = context.current_encounter.clone();
    let mut encounter_category = context.current_encounter_category.clone();
    if parsed_line.raw_event_type == "ENCOUNTER_START" {
        if let Some(new_encounter_name) = extract_encounter_name(&parsed_line.fields) {
            context.current_encounter = Some(new_encounter_name.clone());
            encounter_name = Some(new_encounter_name);
        }
        let category = classify_encounter_category(context, &parsed_line.fields);
        context.current_encounter_category = Some(category.to_string());
        encounter_category = context.current_encounter_category.clone();
    } else if parsed_line.raw_event_type == "ENCOUNTER_END" {
        if let Some(finished_encounter_name) = extract_encounter_name(&parsed_line.fields) {
            encounter_name = Some(finished_encounter_name);
        }
        if encounter_category.is_none() {
            encounter_category = Some(classify_encounter_category(context, &parsed_line.fields).to_string());
        }
        context.current_encounter = None;
        context.current_encounter_category = None;
    }

    if is_guardian_target(parsed_line.target_kind.as_deref()) {
        return None;
    }

    Some(ParsedCombatEvent {
        line_number,
        log_timestamp: parsed_line.log_timestamp,
        event_type: parsed_line.normalized_event_type,
        source: parsed_line.source,
        target: parsed_line.target,
        target_kind: parsed_line.target_kind,
        zone_name: context.current_zone.clone(),
        encounter_name,
        encounter_category,
    })
}

#[derive(Debug, Default)]
struct DebugParseContext {
    current_zone: Option<String>,
    current_encounter: Option<String>,
    current_encounter_category: Option<String>,
    in_challenge_mode: bool,
    in_pvp_match: bool,
}

#[derive(Debug)]
struct ParsedLogLine {
    raw_event_type: String,
    normalized_event_type: String,
    log_timestamp: String,
    source: Option<String>,
    target: Option<String>,
    target_kind: Option<String>,
    fields: Vec<String>,
}

fn parse_log_line_fields(line: &str) -> Option<ParsedLogLine> {
    let trimmed_line = line.trim();
    if trimmed_line.is_empty() {
        return None;
    }

    let mut fields = trimmed_line.split(',');
    let header = fields.next()?.trim();
    let raw_event_type = extract_event_type(header)?;
    let normalized_event_type = normalize_important_event_type(raw_event_type)?;
    let remaining_fields = fields.map(|value| value.trim().to_string()).collect::<Vec<String>>();

    let source_name = remaining_fields.get(1).map(|value| value.as_str());
    let source_guid = remaining_fields.first().map(|value| value.as_str());
    let source_flags = remaining_fields.get(2).map(|value| value.as_str());
    let dest_guid = remaining_fields.get(4).map(|value| value.as_str());
    let dest_name = remaining_fields.get(5).map(|value| value.as_str());
    let dest_flags = remaining_fields.get(6).map(|value| value.as_str());
    let source_kind = classify_unit_kind(source_flags, source_guid).map(str::to_string);
    let target_kind = classify_unit_kind(dest_flags, dest_guid).map(str::to_string);

    Some(ParsedLogLine {
        raw_event_type: raw_event_type.to_string(),
        normalized_event_type: normalized_event_type.to_string(),
        log_timestamp: extract_log_timestamp(header),
        source: normalize_entity_name(source_name, source_kind.as_deref()),
        target: normalize_entity_name(dest_name, target_kind.as_deref()),
        target_kind,
        fields: remaining_fields,
    })
}

fn normalize_important_event_type(event_type: &str) -> Option<&'static str> {
    match event_type {
        "PARTY_KILL" => Some("PARTY_KILL"),
        "UNIT_DIED" | "UNIT_DESTROYED" => Some("UNIT_DIED"),
        "SPELL_INTERRUPT" => Some("SPELL_INTERRUPT"),
        "SPELL_DISPEL" => Some("SPELL_DISPEL"),
        "ENCOUNTER_START" => Some("ENCOUNTER_START"),
        "ENCOUNTER_END" => Some("ENCOUNTER_END"),
        "ZONE_CHANGE" | "ZONE_CHANGE_NEW_AREA" | "ZONE_CHANGED" | "ZONE_CHANGED_INDOORS"
        | "PLAYER_ENTERING_WORLD" | "MAP_CHANGE" => Some("ZONE_CONTEXT"),
        "CHALLENGE_MODE_START" | "CHALLENGE_MODE_END" => Some("CHALLENGE_CONTEXT"),
        "ARENA_MATCH_START" | "ARENA_MATCH_END" | "PVP_MATCH_START" | "PVP_MATCH_COMPLETE"
        | "BATTLEGROUND_START" | "BATTLEGROUND_END" => Some("PVP_CONTEXT"),
        _ => None,
    }
}

fn update_debug_context(context: &mut DebugParseContext, parsed_line: &ParsedLogLine) {
    match parsed_line.raw_event_type.as_str() {
        "CHALLENGE_MODE_START" => context.in_challenge_mode = true,
        "CHALLENGE_MODE_END" => context.in_challenge_mode = false,
        "ARENA_MATCH_START" | "PVP_MATCH_START" | "BATTLEGROUND_START" => {
            context.in_pvp_match = true;
        }
        "ARENA_MATCH_END" | "PVP_MATCH_COMPLETE" | "BATTLEGROUND_END" => {
            context.in_pvp_match = false;
        }
        _ => {}
    }
}

fn is_context_only_event(raw_event_type: &str) -> bool {
    matches!(
        raw_event_type,
        "ZONE_CHANGE"
            | "ZONE_CHANGE_NEW_AREA"
            | "ZONE_CHANGED"
            | "ZONE_CHANGED_INDOORS"
            | "PLAYER_ENTERING_WORLD"
            | "MAP_CHANGE"
            | "CHALLENGE_MODE_START"
            | "CHALLENGE_MODE_END"
            | "ARENA_MATCH_START"
            | "ARENA_MATCH_END"
            | "PVP_MATCH_START"
            | "PVP_MATCH_COMPLETE"
            | "BATTLEGROUND_START"
            | "BATTLEGROUND_END"
    )
}

fn classify_encounter_category(context: &DebugParseContext, fields: &[String]) -> &'static str {
    if context.in_challenge_mode {
        return "mythicPlus";
    }

    if context.in_pvp_match {
        return "pvp";
    }

    if let Some(difficulty_id) = extract_encounter_difficulty_id(fields) {
        if is_raid_difficulty(difficulty_id) {
            return "raid";
        }
    }

    "unknown"
}

fn extract_encounter_difficulty_id(fields: &[String]) -> Option<u32> {
    fields
        .get(2)
        .and_then(|value| value.trim_matches('"').parse::<u32>().ok())
}

fn is_raid_difficulty(difficulty_id: u32) -> bool {
    matches!(difficulty_id, 3 | 4 | 5 | 6 | 14 | 15 | 16 | 17)
}

fn extract_encounter_name(fields: &[String]) -> Option<String> {
    normalize_name(fields.get(1).map(|value| value.as_str()))
}

fn extract_zone_name(raw_event_type: &str, fields: &[String]) -> Option<String> {
    let is_zone_context_event = matches!(
        raw_event_type,
        "ZONE_CHANGE"
            | "ZONE_CHANGE_NEW_AREA"
            | "ZONE_CHANGED"
            | "ZONE_CHANGED_INDOORS"
            | "PLAYER_ENTERING_WORLD"
            | "MAP_CHANGE"
    );

    if !is_zone_context_event {
        return None;
    }

    fields
        .iter()
        .find_map(|value| normalize_name(Some(value.as_str())))
}

fn classify_unit_kind(unit_flags: Option<&str>, unit_guid: Option<&str>) -> Option<&'static str> {
    if let Some(flags_value) = parse_combat_log_flags(unit_flags) {
        const TYPE_PLAYER: u32 = 0x0000_0400;
        const TYPE_NPC: u32 = 0x0000_0800;
        const TYPE_PET: u32 = 0x0000_1000;
        const TYPE_GUARDIAN: u32 = 0x0000_2000;
        const TYPE_OBJECT: u32 = 0x0000_4000;

        if flags_value & TYPE_PLAYER != 0 {
            return Some("PLAYER");
        }
        if flags_value & TYPE_PET != 0 {
            return Some("PET");
        }
        if flags_value & TYPE_GUARDIAN != 0 {
            return Some("GUARDIAN");
        }
        if flags_value & TYPE_NPC != 0 {
            return Some("NPC");
        }
        if flags_value & TYPE_OBJECT != 0 {
            return Some("OBJECT");
        }
    }

    let normalized_guid = normalize_name(unit_guid);
    if let Some(guid) = normalized_guid.as_deref() {
        if guid.starts_with("Player-") {
            return Some("PLAYER");
        }
        if guid.starts_with("Pet-") {
            return Some("PET");
        }
        if guid.starts_with("Creature-") || guid.starts_with("Vehicle-") {
            return Some("NPC");
        }
        if guid.starts_with("GameObject-") {
            return Some("OBJECT");
        }

        return Some("UNKNOWN");
    }

    None
}

fn parse_combat_log_flags(raw_flags: Option<&str>) -> Option<u32> {
    let value = raw_flags?.trim();
    if value.is_empty() || value == "nil" {
        return None;
    }

    let unquoted = value.trim_matches('"');
    if let Some(hex_value) = unquoted
        .strip_prefix("0x")
        .or_else(|| unquoted.strip_prefix("0X"))
    {
        return u32::from_str_radix(hex_value, 16).ok();
    }

    unquoted.parse::<u32>().ok()
}

fn is_guardian_target(target_kind: Option<&str>) -> bool {
    matches!(target_kind, Some("GUARDIAN"))
}

fn extract_event_type(header: &str) -> Option<&str> {
    if let Some((_, event_type)) = header.rsplit_once("  ") {
        return Some(event_type.trim());
    }

    header.split_whitespace().last().map(str::trim)
}

fn extract_log_timestamp(header: &str) -> String {
    if let Some((timestamp, _)) = header.rsplit_once("  ") {
        return timestamp.trim().to_string();
    }

    header
        .split_whitespace()
        .take(2)
        .collect::<Vec<&str>>()
        .join(" ")
}

fn normalize_entity_name(name: Option<&str>, unit_kind: Option<&str>) -> Option<String> {
    let normalized_name = normalize_name(name)?;
    if unit_kind != Some("PLAYER") {
        return Some(normalized_name);
    }

    Some(trim_player_region_suffix(&normalized_name))
}

fn trim_player_region_suffix(name: &str) -> String {
    let Some((without_region, region)) = name.rsplit_once('-') else {
        return name.to_string();
    };

    if !without_region.contains('-') {
        return name.to_string();
    }

    if looks_like_region_code(region) {
        return without_region.to_string();
    }

    name.to_string()
}

fn looks_like_region_code(value: &str) -> bool {
    let length = value.len();
    if !(2..=4).contains(&length) {
        return false;
    }

    value.chars().all(|character| character.is_ascii_uppercase())
}

fn normalize_name(name: Option<&str>) -> Option<String> {
    let value = name?.trim();
    if value.is_empty() || value == "nil" {
        return None;
    }

    Some(value.trim_matches('"').to_string())
}
