import React from 'react';

const TONES = {
  primary: { background: 'var(--color-primary-100)', color: 'var(--color-primary-700)' },
  success: { background: 'var(--color-success-100)', color: 'var(--color-success-700)' },
  warning: { background: 'var(--color-warning-100)', color: 'var(--color-warning-700)' },
  error:   { background: 'var(--color-error-100)',   color: 'var(--color-error-700)' },
  info:    { background: 'var(--color-info-100)',    color: 'var(--color-info-700)' },
  neutral: { background: 'var(--color-neutral-100)', color: 'var(--color-neutral-700)' },
};

const SOLID = {
  primary: { background: 'var(--color-primary-500)', color: 'var(--text-inverse)' },
  success: { background: 'var(--color-success-500)', color: 'var(--text-inverse)' },
  warning: { background: 'var(--color-warning-500)', color: 'var(--text-inverse)' },
  error:   { background: 'var(--color-error-500)',   color: 'var(--text-inverse)' },
};

/**
 * Badge — a small pill for counts, tags, and labels. Soft tinted by default,
 * or `solid` for stronger emphasis.
 */
export function Badge({ tone = 'neutral', solid = false, children, style = {}, ...rest }) {
  const palette = solid ? (SOLID[tone] || SOLID.primary) : (TONES[tone] || TONES.neutral);
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 'var(--space-1)',
        padding: 'var(--badge-padding)',
        fontSize: 'var(--badge-font-size)',
        fontWeight: 'var(--font-semibold)',
        borderRadius: 'var(--badge-radius)',
        lineHeight: 1,
        whiteSpace: 'nowrap',
        ...palette,
        ...style,
      }}
      {...rest}
    >
      {children}
    </span>
  );
}
