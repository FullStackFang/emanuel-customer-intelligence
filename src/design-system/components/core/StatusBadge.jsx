import React from 'react';

// Lifecycle states shared across the calendar app and grant management.
// Mirrors the source app's status-badge palette; extended with grant states.
const STATUS = {
  // calendar / reservations
  pending:      { label: 'Pending',      bg: 'var(--color-warning-100)', fg: 'var(--color-warning-700)' },
  published:    { label: 'Published',    bg: 'var(--color-success-100)', fg: 'var(--color-success-700)' },
  approved:     { label: 'Approved',     bg: 'var(--color-success-100)', fg: 'var(--color-success-700)' },
  rejected:     { label: 'Rejected',     bg: 'var(--color-error-100)',   fg: 'var(--color-error-700)' },
  draft:        { label: 'Draft',        bg: 'var(--color-neutral-100)', fg: 'var(--color-neutral-600)', dashed: true },
  // grant proposals
  submitted:    { label: 'Submitted',    bg: 'var(--color-primary-100)', fg: 'var(--color-primary-700)' },
  under_review: { label: 'Under Review', bg: 'var(--color-warning-100)', fg: 'var(--color-warning-700)' },
  deferred:     { label: 'Deferred',     bg: 'var(--color-info-100)',    fg: 'var(--color-info-700)' },
  abandoned:    { label: 'Abandoned',    bg: 'var(--color-neutral-100)', fg: 'var(--color-neutral-500)', dashed: true },
  // org / user / delivery
  active:       { label: 'Active',       bg: 'transparent', fg: 'var(--color-success-700)', outline: 'var(--color-success-400)' },
  inactive:     { label: 'Inactive',     bg: 'transparent', fg: 'var(--color-neutral-600)', outline: 'var(--color-neutral-300)' },
  delivered:    { label: 'Delivered',    bg: 'transparent', fg: 'var(--color-success-700)', outline: 'var(--color-success-400)' },
  bounced:      { label: 'Bounced',      bg: 'transparent', fg: 'var(--color-error-700)',   outline: 'var(--color-error-300)' },
  sent:         { label: 'Sent',         bg: 'var(--color-info-100)',    fg: 'var(--color-info-700)' },
  // reviewer progress
  not_started:  { label: 'Not Started',  bg: 'var(--color-neutral-100)', fg: 'var(--color-neutral-600)' },
  in_progress:  { label: 'In Progress',  bg: 'var(--color-warning-100)', fg: 'var(--color-warning-700)' },
  complete:     { label: 'Complete',     bg: 'var(--color-success-100)', fg: 'var(--color-success-700)' },
};

/**
 * StatusBadge — a semantic pill for a lifecycle state (reservation, proposal,
 * org, delivery, reviewer progress). Pass the machine value in `status`; the
 * label and color are derived. Outline variants (active/inactive/delivered/
 * bounced) use a colored border with transparent fill.
 */
export function StatusBadge({ status, label, style = {}, ...rest }) {
  const s = STATUS[status] || { label: status, bg: 'var(--color-neutral-100)', fg: 'var(--color-neutral-600)' };
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 'var(--space-1)',
        padding: 'var(--space-1) var(--space-3)',
        borderRadius: 'var(--radius-full)',
        fontSize: 'var(--text-xs)',
        fontWeight: 'var(--font-medium)',
        lineHeight: 1.4,
        background: s.bg,
        color: s.fg,
        border: s.outline ? `1px solid ${s.outline}` : (s.dashed ? '1px dashed var(--color-neutral-300)' : '1px solid transparent'),
        whiteSpace: 'nowrap',
        ...style,
      }}
      {...rest}
    >
      {label || s.label}
    </span>
  );
}
