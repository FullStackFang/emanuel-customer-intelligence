/* Grant Management UI kit — shared chrome & primitives.
   Ported adaptation: the design project exposed these on window for its
   preview environment; here they are ES-module exports using the ported
   design-system components. Visual code is otherwise verbatim. */

import React from 'react';
import { Icon } from '../../components/core/Icon.jsx';
import { Menu } from '../../components/overlay/Menu.jsx';
import logoUrl from '../../../assets/emanuel_logo.png';

/* ── App frame: sapphire gradient header + white top nav ────────────────── */
export function AppFrame({ nav, active, onNav, user, children }) {
  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column', background: 'var(--bg-secondary)' }}>
      <header style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        background: 'var(--gradient-brand)', height: 'var(--header-height)',
        padding: '0 var(--space-6)', boxShadow: 'var(--shadow-md)', flexShrink: 0,
        position: 'relative', zIndex: 'var(--z-sticky)',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
          <img src={logoUrl} alt="Temple Emanu-El"
            style={{ width: 40, height: 40, borderRadius: 'var(--radius-md)', background: '#fff', padding: 2, boxShadow: '0 0 0 2px var(--color-accent-400)' }} />
          <div style={{ lineHeight: 1.1 }}>
            <div style={{ color: '#fff', fontFamily: 'var(--font-display)', fontSize: 'var(--text-base)', fontWeight: 'var(--font-semibold)', letterSpacing: 'var(--tracking-tight)' }}>Temple Emanu-El</div>
            <div style={{ color: 'var(--color-accent-300)', fontSize: 'var(--text-2xs)', letterSpacing: '0.18em', textTransform: 'uppercase', fontWeight: 'var(--font-medium)' }}>Customer Intelligence</div>
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)', background: 'rgba(255,255,255,0.12)', borderRadius: 'var(--radius-lg)', padding: '6px 12px' }}>
            <div style={{ width: 26, height: 26, borderRadius: 'var(--radius-full)', background: 'var(--color-accent-400)', color: 'var(--color-primary-900)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 'var(--text-xs)', fontWeight: 'var(--font-bold)' }}>{user.initials}</div>
            <div style={{ color: '#fff', fontSize: 'var(--text-sm)', fontWeight: 'var(--font-medium)' }}>{user.name}</div>
            <span style={{ color: 'rgba(255,255,255,0.6)', fontSize: 'var(--text-2xs)', border: '1px solid rgba(255,255,255,0.3)', borderRadius: 'var(--radius-full)', padding: '1px 7px', textTransform: 'uppercase', letterSpacing: '0.06em' }}>{user.role}</span>
          </div>
        </div>
      </header>

      <nav style={{
        background: 'var(--bg-primary)', borderBottom: '1px solid var(--border-default)',
        height: 'var(--nav-height)', display: 'flex', alignItems: 'center',
        padding: '0 var(--space-6)', gap: 'var(--space-1)', boxShadow: 'var(--shadow-xs)', flexShrink: 0,
      }}>
        {nav.map((n) => (
          n.children
            ? <NavMenu key={n.key} item={n} active={active} onNav={onNav} />
            : <NavButton key={n.key} item={n} active={n.key === active} onNav={onNav} />
        ))}
      </nav>

      {/* No width cap: admin pages are table-heavy and the 1200px column left
          dead margin on any real monitor. A page whose content genuinely reads
          better narrow (a single-column form) caps itself. */}
      <main style={{ flex: 1, overflowY: 'auto', padding: '0 var(--space-8) var(--space-8)' }}>
        {/* The top gap lives on this inner wrapper, not on <main> itself, so it scrolls away
            with the content. If it sat on the scroll container, a `position: sticky; top: 0`
            child would pin one space-8 below the scrollport edge and let content bleed through
            the uncovered band above it (see the Insights section tabs). */}
        <div style={{ paddingTop: 'var(--space-8)' }}>
          {children}
        </div>
      </main>
    </div>
  );
}

const navButtonStyle = (isActive) => ({
  position: 'relative', display: 'inline-flex', alignItems: 'center', gap: 'var(--space-2)',
  height: 40, padding: '0 var(--space-4)', border: 'none', cursor: 'pointer',
  borderRadius: 'var(--radius-lg)', fontFamily: 'var(--font-body)',
  fontSize: 'var(--text-sm)', fontWeight: isActive ? 'var(--font-semibold)' : 'var(--font-medium)',
  color: isActive ? 'var(--color-primary-600)' : 'var(--text-secondary)',
  background: isActive ? 'var(--color-primary-50)' : 'transparent',
  transition: 'var(--transition-all)',
});

function NavButton({ item, active, onNav }) {
  return (
    <button onClick={() => onNav(item.key)} style={navButtonStyle(active)}>
      <Icon name={item.icon} size={16} />
      {item.label}
      {item.count != null && <StatusChipCount active={active} count={item.count} />}
      {active && <span style={{ position: 'absolute', bottom: -1, left: 'var(--space-2)', right: 'var(--space-2)', height: 2, background: 'var(--color-primary-500)', borderRadius: 'var(--radius-full)' }} />}
    </button>
  );
}

/* Nav item with a dropdown of sub-pages (e.g. Admin → User Management). */
function NavMenu({ item, active, onNav }) {
  const isActive = item.children.some((c) => c.key === active);
  const items = item.children.map((c) => ({ key: c.key, label: c.label, icon: c.icon, active: c.key === active, onSelect: () => onNav(c.key) }));

  return (
    <Menu items={items} trigger={({ open, toggle }) => (
      <button onClick={toggle} aria-expanded={open} aria-haspopup="menu" style={navButtonStyle(isActive)}>
        <Icon name={item.icon} size={16} />
        {item.label}
        <Icon name="chevron-down" size={14} />
        {isActive && <span style={{ position: 'absolute', bottom: -1, left: 'var(--space-2)', right: 'var(--space-2)', height: 2, background: 'var(--color-primary-500)', borderRadius: 'var(--radius-full)' }} />}
      </button>
    )} />
  );
}

export function StatusChipCount({ active, count }) {
  return <span style={{
    fontSize: 'var(--text-2xs)', fontWeight: 'var(--font-semibold)', lineHeight: 1.5,
    padding: '1px 7px', borderRadius: 'var(--radius-full)',
    background: active ? 'var(--color-primary-100)' : 'var(--color-warning-500)',
    color: active ? 'var(--color-primary-700)' : '#fff',
  }}>{count}</span>;
}

/* ── Page title row ─────────────────────────────────────────────────────── */
export function PageTitle({ eyebrow, title, actions }) {
  return (
    <div style={{ display: 'flex', alignItems: 'flex-end', justifyContent: 'space-between', gap: 'var(--space-4)', marginBottom: 'var(--space-6)' }}>
      <div>
        {eyebrow && <div style={{ fontSize: 'var(--text-2xs)', letterSpacing: 'var(--tracking-wider)', textTransform: 'uppercase', color: 'var(--text-tertiary)', fontWeight: 'var(--font-semibold)', marginBottom: 'var(--space-1)' }}>{eyebrow}</div>}
        <h1 style={{ margin: 0, fontFamily: 'var(--font-display)', fontSize: 'var(--text-3xl)', fontWeight: 'var(--font-semibold)', letterSpacing: 'var(--tracking-tight)', color: 'var(--text-primary)' }}>{title}</h1>
      </div>
      {actions && <div style={{ display: 'flex', gap: 'var(--space-3)' }}>{actions}</div>}
    </div>
  );
}

/* ── Stat tile ──────────────────────────────────────────────────────────── */
export function Stat({ label, value, sub, tone = 'primary', icon }) {
  const toneColor = { primary: 'var(--color-primary-600)', accent: 'var(--color-accent-700)', success: 'var(--color-success-600)', neutral: 'var(--text-secondary)' }[tone];
  const toneBg = { primary: 'var(--color-primary-50)', accent: 'var(--color-accent-50)', success: 'var(--color-success-50)', neutral: 'var(--bg-tertiary)' }[tone];
  return (
    <div style={{ background: 'var(--bg-primary)', border: '1px solid var(--border-default)', borderRadius: 'var(--radius-xl)', boxShadow: 'var(--shadow-sm)', padding: 'var(--space-5)' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 'var(--space-3)' }}>
        <span style={{ fontSize: 'var(--text-xs)', color: 'var(--text-tertiary)', fontWeight: 'var(--font-medium)', textTransform: 'uppercase', letterSpacing: 'var(--tracking-wide)' }}>{label}</span>
        {icon && <span style={{ display: 'inline-flex', width: 30, height: 30, borderRadius: 'var(--radius-lg)', background: toneBg, color: toneColor, alignItems: 'center', justifyContent: 'center' }}><Icon name={icon} size={16} /></span>}
      </div>
      <div style={{ fontFamily: 'var(--font-display)', fontSize: 'var(--text-3xl)', fontWeight: 'var(--font-semibold)', color: 'var(--text-primary)', letterSpacing: 'var(--tracking-tight)', lineHeight: 1 }}>{value}</div>
      {sub && <div style={{ fontSize: 'var(--text-xs)', color: 'var(--text-tertiary)', marginTop: 'var(--space-2)' }}>{sub}</div>}
    </div>
  );
}

/* ── Cycle status stepper ───────────────────────────────────────────────── */
const CYCLE_STEPS = [
  { key: 'planning', label: 'Planning' },
  { key: 'invitations_sent', label: 'Invitations Sent' },
  { key: 'submissions_open', label: 'Submissions Open' },
  { key: 'under_review', label: 'Under Review' },
  { key: 'awarded', label: 'Awarded' },
  { key: 'closed', label: 'Closed' },
];
export function CycleStepper({ current }) {
  const idx = CYCLE_STEPS.findIndex((s) => s.key === current);
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
      {CYCLE_STEPS.map((s, i) => {
        const done = i < idx, active = i === idx;
        const color = done ? 'var(--color-primary-500)' : active ? 'var(--color-primary-600)' : 'var(--color-neutral-300)';
        return (
          <React.Fragment key={s.key}>
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 6, minWidth: 92 }}>
              <div style={{ width: 26, height: 26, borderRadius: 'var(--radius-full)', display: 'flex', alignItems: 'center', justifyContent: 'center',
                background: active ? 'var(--color-primary-600)' : done ? 'var(--color-primary-100)' : 'var(--bg-tertiary)',
                border: `2px solid ${color}`, color: active ? '#fff' : done ? 'var(--color-primary-600)' : 'var(--text-tertiary)' }}>
                {done ? <Icon name="check" size={14} color="var(--color-primary-600)" /> : <span style={{ fontSize: 'var(--text-xs)', fontWeight: 'var(--font-bold)' }}>{i + 1}</span>}
              </div>
              <span style={{ fontSize: 'var(--text-2xs)', fontWeight: active ? 'var(--font-semibold)' : 'var(--font-medium)', color: active ? 'var(--color-primary-700)' : 'var(--text-tertiary)', textAlign: 'center', whiteSpace: 'nowrap' }}>{s.label}</span>
            </div>
            {i < CYCLE_STEPS.length - 1 && <div style={{ flex: 1, height: 2, background: i < idx ? 'var(--color-primary-400)' : 'var(--color-neutral-200)', marginTop: -18, minWidth: 12 }} />}
          </React.Fragment>
        );
      })}
    </div>
  );
}
