import { createContext, ReactNode, useContext, useState, useCallback } from "react";
import { GameEvent } from "../types/events";

interface MarkerContextType {
  events: GameEvent[];
  addEvent: (event: GameEvent) => void;
  setEvents: (events: GameEvent[]) => void;
  clearEvents: () => void;
}

const MarkerContext = createContext<MarkerContextType | undefined>(undefined);

export function MarkerProvider({ children }: { children: ReactNode }) {
  const [events, setEvents] = useState<GameEvent[]>([]);

  const sortEventsByTimestamp = useCallback((unsortedEvents: GameEvent[]) => {
    return [...unsortedEvents].sort((a, b) => a.timestamp - b.timestamp);
  }, []);

  const addEvent = useCallback((event: GameEvent) => {
    setEvents((prev) => sortEventsByTimestamp([...prev, event]));
  }, [sortEventsByTimestamp]);

  const replaceEvents = useCallback((nextEvents: GameEvent[]) => {
    setEvents(sortEventsByTimestamp(nextEvents));
  }, [sortEventsByTimestamp]);

  const clearEvents = useCallback(() => {
    setEvents([]);
  }, []);

  return (
    <MarkerContext.Provider value={{ events, addEvent, setEvents: replaceEvents, clearEvents }}>
      {children}
    </MarkerContext.Provider>
  );
}

export function useMarker() {
  const context = useContext(MarkerContext);
  if (!context) {
    throw new Error("useMarker must be used within MarkerProvider");
  }
  return context;
}
