import { useState } from "react";
import { Crosshair, Skull } from "lucide-react";
import { mockEvents, GameEvent } from "../data/mockEvents";
import { EventTooltip } from "./EventTooltip";

const VIDEO_DURATION = 150;

export function Timeline() {
  const [currentTime] = useState("00:00");
  const [duration] = useState("02:30");
  const [progress] = useState(0);
  const [hoveredEvent, setHoveredEvent] = useState<GameEvent | null>(null);
  const [tooltipX, setTooltipX] = useState(0);

  const handleEventClick = (timestamp: number) => {
    console.log("Seek to:", timestamp);
  };

  const handleEventHover = (event: GameEvent, e: React.MouseEvent) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const containerRect = (e.currentTarget.closest(".timeline-container") as HTMLElement).getBoundingClientRect();
    const x = rect.left - containerRect.left + rect.width / 2;
    setTooltipX(x);
    setHoveredEvent(event);
  };

  return (
    <div className="bg-neutral-800 border-t border-neutral-700 p-3">
      <div className="timeline-container flex items-center gap-3 relative">
        <button className="text-neutral-400 hover:text-neutral-100 transition-colors text-xl">
          ▶
        </button>
        <span className="text-xs text-neutral-400 font-mono w-12">{currentTime}</span>
        <div className="flex-1 h-2 bg-neutral-700 rounded-full cursor-pointer group relative">
          <div
            className="h-full bg-neutral-400 rounded-full relative group-hover:bg-neutral-300 transition-colors"
            style={{ width: `${progress}%` }}
          >
            <div className="absolute right-0 top-1/2 -translate-y-1/2 w-3 h-3 bg-neutral-100 rounded-full opacity-0 group-hover:opacity-100 transition-opacity" />
          </div>
          {mockEvents.map((event) => {
            const position = (event.timestamp / VIDEO_DURATION) * 100;
            const isDeath = event.type === "death";
            return (
              <div
                key={event.id}
                className="absolute top-1/2 -translate-y-1/2 cursor-pointer -ml-1.5"
                style={{ left: `${position}%` }}
                onClick={() => handleEventClick(event.timestamp)}
                onMouseEnter={(e) => handleEventHover(event, e)}
                onMouseLeave={() => setHoveredEvent(null)}
              >
                {isDeath ? (
                  <Skull className="w-3 h-3 text-orange-400 hover:scale-125 transition-transform" />
                ) : (
                  <Crosshair className="w-3 h-3 text-emerald-400 hover:scale-125 transition-transform" />
                )}
              </div>
            );
          })}
        </div>
        <span className="text-xs text-neutral-400 font-mono w-12 text-right">{duration}</span>
        {hoveredEvent && <EventTooltip event={hoveredEvent} x={tooltipX} />}
      </div>
    </div>
  );
}
