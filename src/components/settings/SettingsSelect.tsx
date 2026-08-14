import { ChevronDown } from "lucide-react";

export interface SettingsSelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface SettingsSelectProps {
  id: string;
  value: string;
  options: SettingsSelectOption[];
  placeholder?: string;
  disabled?: boolean;
  ariaDescribedBy?: string;
  onChange: (value: string) => void;
}

export function SettingsSelect({
  id,
  value,
  options,
  placeholder,
  disabled = false,
  ariaDescribedBy,
  onChange,
}: SettingsSelectProps) {
  return (
    <div className="relative">
      <select
        id={id}
        value={value}
        disabled={disabled}
        aria-describedby={ariaDescribedBy}
        onChange={(event) => onChange(event.target.value)}
        className="w-full appearance-none rounded-sm border border-white/20 bg-black/20 px-3 py-2 pr-9 text-left text-sm text-neutral-100 transition-colors focus:border-white/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/45 disabled:cursor-not-allowed disabled:border-white/10 disabled:bg-black/10 disabled:text-neutral-500"
      >
        {placeholder && (
          <option value="" disabled>
            {placeholder}
          </option>
        )}
        {options.map((option) => (
          <option key={option.value} value={option.value} disabled={option.disabled}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDown
        className="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-neutral-400"
        aria-hidden="true"
      />
    </div>
  );
}
