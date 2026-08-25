import React from 'react';

const SIZES = {
  sm: { height: 'var(--btn-height-sm)', padding: 'var(--btn-padding-sm)', fontSize: 'var(--text-xs)', radius: 'var(--radius-md)' },
  md: { height: 'var(--btn-height-md)', padding: 'var(--btn-padding-md)', fontSize: 'var(--text-sm)', radius: 'var(--radius-lg)' },
  lg: { height: 'var(--btn-height-lg)', padding: 'var(--btn-padding-lg)', fontSize: 'var(--text-base)', radius: 'var(--radius-lg)' },
};

// [rest, hover] background / color / border / extra
const VARIANTS = {
  primary: {
    base: { background: 'var(--color-primary-500)', color: 'var(--text-inverse)', border: '1px solid transparent', boxShadow: 'var(--shadow-sm)' },
    hover: { background: 'var(--color-primary-600)', transform: 'translateY(-1px)', boxShadow: 'var(--shadow-md)' },
  },
  secondary: {
    base: { background: 'var(--bg-primary)', color: 'var(--text-secondary)', border: '1px solid var(--border-default)' },
    hover: { background: 'var(--bg-secondary)', color: 'var(--text-primary)', borderColor: 'var(--border-strong)' },
  },
  ghost: {
    base: { background: 'transparent', color: 'var(--text-secondary)', border: '1px solid transparent' },
    hover: { background: 'var(--bg-secondary)', color: 'var(--text-primary)' },
  },
  danger: {
    base: { background: 'var(--color-error-500)', color: 'var(--text-inverse)', border: '1px solid transparent' },
    hover: { background: 'var(--color-error-600)', transform: 'translateY(-1px)' },
  },
  'danger-outline': {
    base: { background: 'var(--bg-primary)', color: 'var(--color-error-500)', border: '1px solid var(--color-error-500)' },
    hover: { background: 'var(--color-error-50)', borderColor: 'var(--color-error-600)' },
  },
  success: {
    base: { background: 'var(--color-success-500)', color: 'var(--text-inverse)', border: '1px solid transparent' },
    hover: { background: 'var(--color-success-600)', transform: 'translateY(-1px)' },
  },
  warning: {
    base: { background: 'var(--color-warning-500)', color: 'var(--text-inverse)', border: '1px solid transparent' },
    hover: { background: 'var(--color-warning-600)', transform: 'translateY(-1px)' },
  },
};

/**
 * Button — the primary interactive control. Sapphire primary lifts 1px on hover
 * with a deepening shadow; destructive actions use the outlined red variant.
 */
export function Button({
  variant = 'primary',
  size = 'md',
  fullWidth = false,
  disabled = false,
  iconLeft = null,
  iconRight = null,
  type = 'button',
  onClick,
  children,
  style = {},
  ...rest
}) {
  const [hover, setHover] = React.useState(false);
  const [active, setActive] = React.useState(false);
  const sz = SIZES[size] || SIZES.md;
  const v = VARIANTS[variant] || VARIANTS.primary;

  const composed = {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 'var(--space-2)',
    fontFamily: 'var(--font-body)',
    fontWeight: 'var(--font-medium)',
    lineHeight: 'var(--leading-normal)',
    whiteSpace: 'nowrap',
    cursor: disabled ? 'not-allowed' : 'pointer',
    opacity: disabled ? 0.5 : 1,
    width: fullWidth ? '100%' : 'auto',
    height: sz.height,
    padding: sz.padding,
    fontSize: sz.fontSize,
    borderRadius: sz.radius,
    transition: 'var(--transition-all)',
    ...v.base,
    ...(hover && !disabled ? v.hover : null),
    ...(active && !disabled ? { transform: 'translateY(0)', boxShadow: 'var(--shadow-sm)' } : null),
    ...style,
  };

  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => { setHover(false); setActive(false); }}
      onMouseDown={() => setActive(true)}
      onMouseUp={() => setActive(false)}
      style={composed}
      {...rest}
    >
      {iconLeft}
      {children}
      {iconRight}
    </button>
  );
}
