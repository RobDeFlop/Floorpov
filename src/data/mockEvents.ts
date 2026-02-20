export interface GameEvent {
  id: string;
  type: "death" | "kill";
  timestamp: number;
  target?: string;
  source?: string;
}

export const mockEvents: GameEvent[] = [
  { id: "1", type: "death", timestamp: 12, target: "Tank" },
  { id: "2", type: "kill", timestamp: 28, source: "Player", target: "Enemy Hunter" },
  { id: "3", type: "death", timestamp: 45, target: "Healer" },
  { id: "4", type: "kill", timestamp: 62, source: "Player", target: "Enemy Warlock" },
  { id: "5", type: "death", timestamp: 78, target: "DPS" },
  { id: "6", type: "kill", timestamp: 95, source: "Player", target: "Enemy Priest" },
  { id: "7", type: "death", timestamp: 110, target: "Tank" },
  { id: "8", type: "kill", timestamp: 125, source: "Player", target: "Enemy Rogue" },
];
