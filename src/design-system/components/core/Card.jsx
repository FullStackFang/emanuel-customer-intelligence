import React from 'react';

/**
 * Card — the standard surface: white, hairline border, soft shadow, generous
 * 12px radius. Optional `hoverable` lifts it 2px with a deeper shadow.
 * Compose with CardHeader / CardBody / CardFooter, or drop children straight in.
 */
export function Card({ hoverable = false, padded = true, children, style = {}, ...rest }) {
  const [hover, setHover] = React.useState(false);
  return (
    <div
      onMouseEnter={hoverable ? () => setHover(true) : undefined}
      onMouseLeave={hoverable ? () => setHover(false) : undefined}
      style={{
        background: 'var(--bg-primary)',
        border: '1px solid var(--border-default)',
        borderRadius: 'var(--card-radius)',
        boxShadow: hover ? 'var(--card-shadow-hover)' : 'var(--card-shadow)',
        padding: padded ? 'var(--card-padding)' : 0,
        transition: 'var(--transition-all)',
        transform: hover ? 'translateY(-2px)' : 'none',
        ...style,
      }}
      {...rest}
    >
      {children}
    </div>
  );
}

export function CardHeader({ children, style = {}, ...rest }) {
  return (
    <div
      style={{
        paddingBottom: 'var(--space-4)',
        marginBottom: 'var(--space-4)',
        borderBottom: '1px solid var(--border-subtle)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 'var(--space-3)',
        ...style,
      }}
      {...rest}
    >
      {children}
    </div>
  );
}

export function CardTitle({ children, style = {}, ...rest }) {
  return (
    <h3
      style={{
        fontFamily: 'var(--font-display)',
        fontSize: 'var(--text-lg)',
        fontWeight: 'var(--font-semibold)',
        color: 'var(--text-primary)',
        letterSpacing: 'var(--tracking-tight)',
        margin: 0,
        ...style,
      }}
      {...rest}
    >
      {children}
    </h3>
  );
}

export function CardFooter({ children, style = {}, ...rest }) {
  return (
    <div
      style={{
        paddingTop: 'var(--space-4)',
        marginTop: 'var(--space-4)',
        borderTop: '1px solid var(--border-subtle)',
        display: 'flex',
        justifyContent: 'flex-end',
        gap: 'var(--space-3)',
        ...style,
      }}
      {...rest}
    >
      {children}
    </div>
  );
}
