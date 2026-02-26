import { AlertTriangle } from "lucide-react";

interface DeleteConfirmDialogProps {
  /** Content rendered inside the description paragraph. */
  description: React.ReactNode;
  /** Whether a deletion is currently in progress. */
  isDeleting: boolean;
  /** Label for the confirm button when not deleting. */
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  /** Optional ref forwarded to the dialog container div (used for focus trapping). */
  dialogRef?: React.RefObject<HTMLDivElement | null>;
  /** Optional ref for the cancel button (used for initial focus). */
  cancelButtonRef?: React.RefObject<HTMLButtonElement | null>;
  /** id for the dialog title element, used by aria-labelledby. */
  titleId?: string;
  /** id for the dialog description element, used by aria-describedby. */
  descriptionId?: string;
  title: string;
}

export function DeleteConfirmDialog({
  description,
  isDeleting,
  confirmLabel = "Delete",
  onConfirm,
  onCancel,
  dialogRef,
  cancelButtonRef,
  titleId,
  descriptionId,
  title,
}: DeleteConfirmDialogProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm">
      <div
        ref={dialogRef}
        className="w-full max-w-md rounded-sm border border-white/15 bg-(--surface-2) p-4 shadow-(--surface-glow)"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
      >
        <div className="mb-3 inline-flex h-8 w-8 items-center justify-center rounded-sm border border-rose-300/25 bg-rose-500/12">
          <AlertTriangle className="h-4 w-4 text-rose-200" />
        </div>
        <h3
          id={titleId}
          className="text-sm font-semibold uppercase tracking-[0.11em] text-neutral-100"
        >
          {title}
        </h3>
        <p id={descriptionId} className="mt-2 text-sm text-neutral-300">
          {description}
        </p>
        <div className="mt-4 flex items-center justify-end gap-2">
          <button
            ref={cancelButtonRef}
            type="button"
            onClick={onCancel}
            disabled={isDeleting}
            className="inline-flex h-8 items-center rounded-sm border border-white/20 bg-black/20 px-3 text-xs text-neutral-200 transition-colors hover:bg-white/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/45 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={isDeleting}
            className="inline-flex h-8 items-center rounded-sm border border-rose-300/35 bg-rose-500/14 px-3 text-xs font-semibold text-rose-100 transition-colors hover:bg-rose-500/22 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-rose-300/60 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isDeleting ? "Deleting..." : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
