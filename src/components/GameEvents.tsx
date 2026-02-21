import { useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from 'motion/react';
import { Activity } from "lucide-react";
import { EventTooltip } from "./EventTooltip";
import { useVideo } from "../contexts/VideoContext";
import { useMarker } from "../contexts/MarkerContext";
import { GameEvent } from "../types/events";

export function GameEvents() {
  const { duration, seek } = useVideo();
  const { events } = useMarker();
  const reduceMotion = useReducedMotion();
  const [hoveredEvent, setHoveredEvent] = useState<GameEvent | null>(null);
  const [tooltipX, setTooltipX] = useState(0);

  const handleEventClick = (timestamp: number) => {
    seek(timestamp);
  };

  const handleEventHover = (event: GameEvent, e: React.MouseEvent) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const containerRect = (e.currentTarget.closest(".game-events-container") as HTMLElement).getBoundingClientRect();
    const x = rect.left - containerRect.left + rect.width / 2;
    setTooltipX(x);
    setHoveredEvent(event);
  };

  return (
    <div className="game-events-container bg-[var(--surface-2)] border-t border-emerald-300/10 px-4 py-3">
      <div className="mb-2 flex items-center gap-2 text-xs uppercase tracking-[0.12em] text-neutral-400">
        <Activity className="h-3.5 w-3.5 text-emerald-300" />
        Game Events
      </div>
      <div className="relative h-6 rounded-md border border-emerald-300/10 bg-black/20 px-1">
        <div className="absolute inset-1 rounded-full bg-neutral-800" />
        {events.map((event) => {
          const position = duration > 0 ? (event.timestamp / duration) * 100 : 0;
          const isDeath = event.type === "death";
          const isManual = event.type === "manual";
          const markerClassName = isManual
            ? "h-3 w-3 rounded-sm border border-cyan-200/60 bg-cyan-300"
            : isDeath
              ? "h-3 w-3 rounded-full border border-rose-200/40 bg-rose-400"
              : "h-3 w-3 rounded-full border border-emerald-100/40 bg-emerald-300";
          return (
            <motion.div
              key={event.id}
              className="absolute top-1/2 -translate-y-1/2 cursor-pointer -ml-2"
              style={{ left: `${position}%` }}
              onClick={() => handleEventClick(event.timestamp)}
              onMouseEnter={(e) => handleEventHover(event, e)}
              onMouseLeave={() => setHoveredEvent(null)}
              initial={reduceMotion ? false : { opacity: 0, scale: 0.85 }}
              animate={{ opacity: 1, scale: 1 }}
              transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
            >
              <span className={`block transition-transform hover:scale-125 ${markerClassName}`} />
            </motion.div>
          );
        })}
        <AnimatePresence>{hoveredEvent && <EventTooltip event={hoveredEvent} x={tooltipX} />}</AnimatePresence>
      </div>
    </div>
  );
}
