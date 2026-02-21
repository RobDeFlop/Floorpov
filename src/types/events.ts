export interface GameEvent {
  id: string;
  timestamp: number;
  type: "kill" | "death" | "manual";
  source?: string;
  target?: string;
}

export interface CombatEvent {
  timestamp: number;
  eventType: string;
  source?: string;
  target?: string;
}

export interface ParsedCombatEvent {
  lineNumber: number;
  logTimestamp: string;
  eventType: string;
  source?: string;
  target?: string;
  targetKind?: string;
  zoneName?: string;
  encounterName?: string;
  encounterCategory?: "mythicPlus" | "raid" | "pvp" | "unknown";
}

export interface ParseCombatLogDebugResult {
  filePath: string;
  fileSizeBytes: number;
  totalLines: number;
  parsedEvents: ParsedCombatEvent[];
  eventCounts: Record<string, number>;
  truncated: boolean;
}

export function convertCombatEvent(combatEvent: CombatEvent): GameEvent {
  const type = 
    combatEvent.eventType === "PARTY_KILL" ? "kill" :
    combatEvent.eventType === "UNIT_DIED" ? "death" :
    "manual";

  return {
    id: `${combatEvent.timestamp}-${combatEvent.eventType}`,
    timestamp: combatEvent.timestamp,
    type,
    source: combatEvent.source,
    target: combatEvent.target,
  };
}
