import React from 'react';

const SIZES = {
  sm: 'var(--btn-height-sm)',
  md: 'var(--btn-height-md)',
  lg: 'var(--btn-height-lg)',
};

/**
 * IconButton — a square, icon-only button. Same variants as Button, sized to a
 * single glyph. Always pass an aria-label for accessibility.
 */
export function IconButton({
  variant = 'ghost',
  size = 'md',
  disabled = false,
  onClick,
  children,
  'aria-label': ariaLabel,
  style = {},
  ...rest
}) {
  const [hover, setHover] = React.useState(false);
  const dim = SIZES[size] || SIZES.md;

  const VAR = {
    ghost: { base: { background: 'transparent', color: 'var(--text-secondary)' }, hover: { background: 'var(--bg-secondary)', color: 'var(--text-primary)' } },
    secondary: { base: { background: 'var(--bg-primary)', color: 'var(--text-secondary)', border: '1px solid var(--border-default)' }, hover: { background: 'var(--bg-secondary)', color: 'var(--text-primary)', borderColor: 'var(--border-strong)' } },
    primary: { base: { background: 'var(--color-primary-500)', color: 'var(--text-inverse)', boxShadow: 'var(--shadow-sm)' }, hover: { background: 'var(--color-primary-600)', boxShadow: 'var(--shadow-md)' } },
    'danger-outline': { base: { background: 'var(--bg-primary)', color: 'var(--color-error-500)', border: '1px solid var(--color-error-500)' }, hover: { background: 'var(--color-error-50)' } },
  }[variant] || {};

  return (
    <button
      type="button"
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: dim,
        height: dim,
        borderRadius: 'var(--radius-lg)',
        border: '1px solid transparent',
        cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.5 : 1,
        transition: 'var(--transition-all)',
        ...(VAR.base || {}),
        ...(hover && !disabled ? VAR.hover : null),
        ...style,
      }}
      {...rest}
    >
      {children}
    </button>
  );
}
