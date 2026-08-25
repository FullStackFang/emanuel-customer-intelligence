import React from 'react';
import { Icon } from '../core/Icon.jsx';

const TONES = {
  info:    { bg: 'var(--color-info-50)',    border: 'var(--color-info-200, #bae6fd)',    fg: 'var(--color-info-700)',    icon: 'info' },
  success: { bg: 'var(--color-success-50)', border: 'var(--color-success-200, #a7f3d0)', fg: 'var(--color-success-700)', icon: 'check-circle' },
  warning: { bg: 'var(--color-warning-50)', border: 'var(--color-warning-200, #fde68a)', fg: 'var(--color-warning-700)', icon: 'alert-triangle' },
  error:   { bg: 'var(--color-error-50)',   border: 'var(--color-error-300)',             fg: 'var(--color-error-700)',   icon: 'alert-circle' },
};

/**
 * Alert — an inline banner for contextual messages. Soft tinted background,
 * matching border, leading icon (from Lucide). Give it a title and/or body.
 *
 * Ported adaptation: icon rendering delegates to the Icon wrapper
 * (lucide-react) instead of the design project's window.lucide UMD build.
 */
export function Alert({ tone = 'info', title, icon, children, style = {}, ...rest }) {
  const t = TONES[tone] || TONES.info;

  return (
    <div
      role="status"
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: 'var(--space-3)',
        padding: 'var(--space-4)',
        borderRadius: 'var(--radius-lg)',
        background: t.bg,
        border: `1px solid ${t.border}`,
        color: t.fg,
        ...style,
      }}
      {...rest}
    >
      <Icon name={icon || t.icon} size={20} strokeWidth={1.5} style={{ marginTop: 1 }} />
      <div style={{ flex: 1 }}>
        {title && <div style={{ fontWeight: 'var(--font-semibold)', marginBottom: children ? 'var(--space-1)' : 0 }}>{title}</div>}
        {children && <div style={{ fontSize: 'var(--text-sm)', lineHeight: 'var(--leading-normal)' }}>{children}</div>}
      </div>
    </div>
  );
}
