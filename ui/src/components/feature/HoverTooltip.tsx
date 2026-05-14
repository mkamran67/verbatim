import { useLayoutEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { createPortal } from 'react-dom';

interface Props {
  /** The trigger — wrapped in a span the mouse enters/leaves. */
  children: ReactNode;
  /** Rich tooltip content. Rendered into a body-level portal when hovered. */
  content: ReactNode;
  /** Optional className passed to the trigger wrapper. Use this to keep flex sizing intact. */
  className?: string;
  /** Optional className for the tooltip panel (defaults to a dark slate panel). */
  tooltipClassName?: string;
  /** Don't show tooltip at all (e.g. zero-data days). */
  disabled?: boolean;
}

const VIEWPORT_MARGIN = 8;

/// Hover tooltip that renders into a body-level portal. Because the panel
/// lives outside the trigger's DOM ancestry, it can't be clipped by an
/// `overflow: auto`/`hidden` parent (e.g. the horizontally-scrolling chart
/// containers), and its z-index sits above all other app chrome. The panel
/// is positioned with `position: fixed`, anchored above the trigger by
/// default and flipped below when there isn't room. Horizontal placement is
/// clamped to the viewport so the panel never spills off-screen.
export default function HoverTooltip({ children, content, className, tooltipClassName, disabled }: Props) {
  const triggerRef = useRef<HTMLDivElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    if (!open || !triggerRef.current || !panelRef.current) return;
    const trigger = triggerRef.current.getBoundingClientRect();
    const panel = panelRef.current.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    // Default: above the trigger, horizontally centered.
    let top = trigger.top - panel.height - 8;
    if (top < VIEWPORT_MARGIN) {
      // Not enough room above — flip below.
      top = trigger.bottom + 8;
    }
    if (top + panel.height > vh - VIEWPORT_MARGIN) {
      top = Math.max(VIEWPORT_MARGIN, vh - panel.height - VIEWPORT_MARGIN);
    }

    let left = trigger.left + trigger.width / 2 - panel.width / 2;
    left = Math.max(VIEWPORT_MARGIN, Math.min(left, vw - panel.width - VIEWPORT_MARGIN));

    setPos({ top, left });
  }, [open, content]);

  if (disabled) {
    return <div className={className}>{children}</div>;
  }

  return (
    <>
      <div
        ref={triggerRef}
        className={className}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => { setOpen(false); setPos(null); }}
      >
        {children}
      </div>
      {open && createPortal(
        <div
          ref={panelRef}
          // Render off-screen on first paint until useLayoutEffect computes
          // the correct position — avoids a one-frame flash at top-left.
          style={{
            position: 'fixed',
            top: pos?.top ?? -9999,
            left: pos?.left ?? -9999,
            visibility: pos ? 'visible' : 'hidden',
            zIndex: 9999,
            pointerEvents: 'none',
          }}
          className={tooltipClassName ?? 'bg-slate-900 dark:bg-slate-700 text-white px-3 py-2 rounded-lg shadow-lg min-w-[200px]'}
        >
          {content}
        </div>,
        document.body
      )}
    </>
  );
}
