interface SidebarSectionLabelProps {
  label: string;
}

export function SidebarSectionLabel({ label }: SidebarSectionLabelProps) {
  return (
    <div className="mb-2 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-neutral-300">
      {label}
    </div>
  );
}
