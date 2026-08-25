import React from 'react';

const WIDTHS = { sm: 'var(--modal-width-sm)', md: 'var(--modal-width-md)', lg: 'var(--modal-width-lg)', xl: 'var(--modal-width-xl)' };

/**
 * Modal — centered dialog over a dimmed backdrop. Header with title + close,
 * scrollable body, optional footer action row. Closes on backdrop click and Esc.
 */
export function Modal({ open = true, onClose, title, size = 'md', footer, children, style = {} }) {
  React.useEffect(() => {
    if (!open) return;
    const onKey = (e) => { if (e.key === 'Escape' && onClose) onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed', inset: 0, zIndex: 'var(--z-modal)',
        background: 'rgb(28 25 23 / 0.45)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        padding: 'var(--space-6)',
        animation: 'emuFade var(--duration-fast) var(--ease-out)',
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        style={{
          width: '100%', maxWidth: WIDTHS[size] || WIDTHS.md, maxHeight: '90vh',
          display: 'flex', flexDirection: 'column',
          background: 'var(--bg-primary)',
          borderRadius: 'var(--modal-radius)',
          boxShadow: 'var(--modal-shadow)',
          overflow: 'hidden',
          ...style,
        }}
      >
        <div style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          gap: 'var(--space-3)', padding: 'var(--space-5) var(--space-6)',
          borderBottom: '1px solid var(--border-subtle)',
        }}>
          <h3 style={{ margin: 0, fontFamily: 'var(--font-display)', fontSize: 'var(--text-xl)', fontWeight: 'var(--font-semibold)', color: 'var(--text-primary)', letterSpacing: 'var(--tracking-tight)' }}>{title}</h3>
          {onClose && (
            <button
              type="button" aria-label="Close" onClick={onClose}
              style={{ display: 'inline-flex', alignItems: 'center', justifyContent: 'center', width: 32, height: 32, borderRadius: 'var(--radius-lg)', border: 'none', background: 'transparent', color: 'var(--text-tertiary)', fontSize: 22, lineHeight: 1, cursor: 'pointer' }}
            >×</button>
          )}
        </div>
        <div style={{ padding: 'var(--space-6)', overflowY: 'auto', flex: 1 }}>{children}</div>
        {footer && (
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 'var(--space-3)', padding: 'var(--space-4) var(--space-6)', borderTop: '1px solid var(--border-subtle)', background: 'var(--bg-secondary)' }}>{footer}</div>
        )}
      </div>
    </div>
  );
}
