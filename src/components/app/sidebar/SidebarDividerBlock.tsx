import { type ReactNode } from "react";

interface SidebarDividerBlockProps {
  children: ReactNode;
}

export function SidebarDividerBlock({ children }: SidebarDividerBlockProps) {
  return <div className="mt-0 border-t border-white/5 pt-3">{children}</div>;
}
