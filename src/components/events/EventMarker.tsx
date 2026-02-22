import { GameEvent } from "../../types/events";

type EventMarkerVariant = "compact" | "detailed";

interface EventMarkerProps {
  type: GameEvent["type"];
  variant?: EventMarkerVariant;
  className?: string;
}

const VARIANT_CLASS_NAMES: Record<EventMarkerVariant, string> = {
  compact: "h-2.5 w-2.5",
  detailed: "h-3 w-3",
};

export function EventMarker({ type, variant = "compact", className }: EventMarkerProps) {
  const colorClassName =
    type === "manual"
      ? variant === "detailed"
        ? "rounded-sm border border-cyan-200/60 bg-cyan-300"
        : "rounded-sm bg-cyan-300"
      : type === "death"
        ? variant === "detailed"
          ? "rounded-full border border-rose-200/40 bg-rose-400"
          : "rounded-full bg-rose-300"
        : variant === "detailed"
          ? "rounded-full border border-emerald-100/40 bg-emerald-300"
          : "rounded-full bg-emerald-200";

  return (
    <span
      className={`block ${VARIANT_CLASS_NAMES[variant]} ${colorClassName} ${className || ""}`.trim()}
    />
  );
}
