export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function formatTime(seconds: number): string {
  if (!seconds || isNaN(seconds)) return "0:00";
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

export function formatDate(timestampSeconds: number): string {
  return new Date(timestampSeconds * 1000).toLocaleString();
}

export function getEventTypeLabel(eventType: string): string {
  switch (eventType) {
    case "PARTY_KILL":
      return "Kill";
    case "UNIT_DIED":
      return "Death";
    case "SPELL_INTERRUPT":
      return "Interrupt";
    case "SPELL_DISPEL":
      return "Dispel";
    case "MANUAL_MARKER":
      return "Manual Marker";
    case "ENCOUNTER_START":
      return "Encounter Start";
    case "ENCOUNTER_END":
      return "Encounter End";
    default:
      return eventType;
  }
}
