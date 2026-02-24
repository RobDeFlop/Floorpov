import { type ComponentType } from "react";
import { Button } from "../../ui/Button";

interface SidebarNavButtonProps {
  label: string;
  icon?: ComponentType<{ className?: string }>;
  isActive: boolean;
  activeClassName: string;
  defaultClassName: string;
  onClick: () => void;
}

export function SidebarNavButton({
  label,
  icon: Icon,
  isActive,
  activeClassName,
  defaultClassName,
  onClick,
}: SidebarNavButtonProps) {
  return (
    <Button
      variant="ghost"
      onClick={onClick}
      className={`flex w-full items-center gap-2 ${isActive ? activeClassName : defaultClassName}`}
      ariaLabel={label}
    >
      {Icon && <Icon className="h-4 w-4" />}
      {label}
    </Button>
  );
}
