import { Flag, Skull, Sword } from "lucide-react";
import { GameEvent } from "../../types/events";

type EventMarkerVariant = "compact" | "detailed";

interface EventMarkerProps {
  type: GameEvent["type"];
  variant?: EventMarkerVariant;
  className?: string;
}

const VARIANT_CLASS_NAMES: Record<EventMarkerVariant, string> = {
  compact: "h-4 w-4",
  detailed: "h-7 w-7",
};

const ICON_CLASS_NAMES: Record<EventMarkerVariant, string> = {
  compact: "h-2.5 w-2.5",
  detailed: "h-4.5 w-4.5",
};

const ICONS: Record<GameEvent["type"], React.ComponentType<{ className?: string }>> = {
  kill: Sword,
  death: Skull,
  manual: Flag,
};

export function EventMarker({ type, variant = "compact", className }: EventMarkerProps) {
  const Icon = ICONS[type];
  const colorClassName =
    type === "manual"
      ? variant === "detailed"
        ? "rounded-sm border border-neutral-100/55 bg-neutral-400 text-neutral-900"
        : "rounded-sm bg-neutral-400 text-neutral-900"
      : type === "death"
        ? variant === "detailed"
          ? "rounded-full border border-rose-200/40 bg-rose-500 text-rose-950"
          : "rounded-full bg-rose-500 text-rose-950"
        : variant === "detailed"
          ? "rounded-full border border-neutral-100/45 bg-neutral-500 text-neutral-900"
          : "rounded-full bg-neutral-500 text-neutral-900";

  return (
    <span
      className={`flex items-center justify-center ${VARIANT_CLASS_NAMES[variant]} ${colorClassName} ${className || ""}`.trim()}
    >
      <Icon className={ICON_CLASS_NAMES[variant]} />
    </span>
  );
}
