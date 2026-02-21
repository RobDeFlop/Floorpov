import { useState } from 'react';
import { motion, useReducedMotion } from 'motion/react';
import { Activity, Bug, PanelLeft, Radar, SlidersHorizontal } from 'lucide-react';

const gameModes = ['Mythic+', 'Raid', 'PvP'];

interface SidebarProps {
  onNavigate: (view: 'main' | 'settings' | 'debug') => void;
  currentView: 'main' | 'settings' | 'debug';
  showDebug: boolean;
}

export function Sidebar({ onNavigate, currentView, showDebug }: SidebarProps) {
  const [activeMode, setActiveMode] = useState<string | null>(null);
  const reduceMotion = useReducedMotion();
  const isMain = currentView === 'main';
  const isSettings = currentView === 'settings';
  const isDebug = currentView === 'debug';

  return (
    <aside className="w-56 border-r border-emerald-300/10 bg-[var(--surface-1)]/95 backdrop-blur-md flex flex-col">
      <div className="border-b border-emerald-300/10 px-3 py-3">
        <div className="mb-2 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-emerald-200">
          <PanelLeft className="h-3.5 w-3.5" />
          Navigation
        </div>
        <div className="grid gap-1.5">
          <motion.button
            onClick={() => onNavigate('main')}
            className={`flex w-full items-center gap-2 rounded-md border px-2.5 py-2 text-sm transition-colors ${
              isMain
                ? 'border-emerald-300/30 bg-emerald-500/15 text-emerald-100'
                : 'border-transparent text-neutral-300 hover:border-emerald-300/20 hover:bg-white/5 hover:text-neutral-100'
            }`}
            whileHover={reduceMotion ? undefined : { x: 2 }}
            whileTap={reduceMotion ? undefined : { scale: 0.99 }}
          >
            <Radar className="h-4 w-4" />
            Home
          </motion.button>
          <motion.button
            onClick={() => onNavigate('settings')}
            className={`flex w-full items-center gap-2 rounded-md border px-2.5 py-2 text-sm transition-colors ${
              isSettings
                ? 'border-emerald-300/30 bg-emerald-500/15 text-emerald-100'
                : 'border-transparent text-neutral-300 hover:border-emerald-300/20 hover:bg-white/5 hover:text-neutral-100'
            }`}
            whileHover={reduceMotion ? undefined : { x: 2 }}
            whileTap={reduceMotion ? undefined : { scale: 0.99 }}
          >
            <SlidersHorizontal className="h-4 w-4" />
            Settings
          </motion.button>
          {showDebug && (
            <motion.button
              onClick={() => onNavigate('debug')}
              className={`flex w-full items-center gap-2 rounded-md border px-2.5 py-2 text-sm transition-colors ${
                isDebug
                  ? 'border-amber-300/35 bg-amber-500/15 text-amber-100'
                  : 'border-transparent text-neutral-300 hover:border-amber-300/25 hover:bg-white/5 hover:text-neutral-100'
              }`}
              whileHover={reduceMotion ? undefined : { x: 2 }}
              whileTap={reduceMotion ? undefined : { scale: 0.99 }}
            >
              <Bug className="h-4 w-4" />
              Debug
            </motion.button>
          )}
        </div>
      </div>

      <nav className="flex-1 p-3">
        <div className="mb-2 flex items-center gap-2 text-[11px] uppercase tracking-[0.14em] text-neutral-500">
          <Activity className="h-3.5 w-3.5" />
          Game Mode
        </div>
        <div className="space-y-1.5">
          {gameModes.map((mode) => (
            <motion.button
              key={mode}
              onClick={() => setActiveMode(activeMode === mode ? null : mode)}
              className={`w-full text-left px-3 py-2 rounded-md text-sm border transition-colors ${
                activeMode === mode
                  ? 'border-emerald-300/30 bg-emerald-500/12 text-emerald-100'
                  : 'border-transparent text-neutral-400 hover:text-neutral-100 hover:border-emerald-300/15 hover:bg-white/5'
              }`}
              whileHover={reduceMotion ? undefined : { x: 2 }}
              whileTap={reduceMotion ? undefined : { scale: 0.99 }}
            >
              {mode}
            </motion.button>
          ))}
        </div>
      </nav>

      <div className="border-t border-emerald-300/10 p-3">
        <div className="rounded-md border border-emerald-300/15 bg-emerald-500/10 px-3 py-2">
          <div className="text-[11px] uppercase tracking-[0.12em] text-emerald-300">App Status</div>
          <div className="mt-1 text-xs text-neutral-300">Ready to start preview.</div>
        </div>
      </div>
    </aside>
  );
}
