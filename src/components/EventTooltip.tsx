import { GameEvent } from "../types/events";

interface EventTooltipProps {
  event: GameEvent;
  x: number;
}

function formatTime(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

export function EventTooltip({ event, x }: EventTooltipProps) {
  const label = 
    event.type === "death" ? "Death" :
    event.type === "manual" ? "Manual Marker" :
    "Kill";
  
  const description =
    event.type === "death" ? `${event.target} died` :
    event.type === "manual" ? "User marked this moment" :
    `${event.source} killed ${event.target}`;

  return (
    <div
      className="absolute bottom-full mb-2 px-2 py-1 bg-neutral-700 text-neutral-200 text-xs rounded whitespace-nowrap pointer-events-none z-10"
      style={{ left: x }}
    >
      <div className="font-medium">{label}</div>
      <div className="text-neutral-400">{description}</div>
      <div className="text-neutral-500">{formatTime(event.timestamp)}</div>
    </div>
  );
}
