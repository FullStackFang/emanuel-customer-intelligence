import React from 'react';
import { Icon } from '../core/Icon.jsx';

/**
 * EmptyState — centered placeholder for empty lists and zero-result views.
 * Optional Lucide icon, title, message, and an action (usually a Button).
 *
 * Ported adaptation: the design project rendered the icon from Lucide's UMD
 * build via window.lucide; here it delegates to the Icon wrapper
 * (lucide-react) with the same kebab-case `icon` name prop.
 */
export function EmptyState({ icon, title, message, action, style = {} }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', textAlign: 'center', padding: 'var(--space-10)', ...style }}>
      {icon && (
        <Icon
          name={icon}
          size={56}
          strokeWidth={1.25}
          color="var(--color-neutral-300)"
          style={{ marginBottom: 'var(--space-4)' }}
        />
      )}
      {title && <div style={{ fontFamily: 'var(--font-display)', fontSize: 'var(--text-lg)', fontWeight: 'var(--font-semibold)', color: 'var(--text-secondary)', marginBottom: 'var(--space-2)' }}>{title}</div>}
      {message && <div style={{ fontSize: 'var(--text-sm)', color: 'var(--text-tertiary)', maxWidth: 400, marginBottom: action ? 'var(--space-6)' : 0, lineHeight: 'var(--leading-normal)' }}>{message}</div>}
      {action}
    </div>
  );
}
