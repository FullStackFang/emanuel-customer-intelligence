import React from 'react';

/**
 * Table — declarative data table with the brand chrome: uppercase muted header
 * on warm-stone fill, hairline row separators, gold-free row hover. Define
 * `columns` and pass `rows`; each column may supply a `render(row)`.
 *
 * `renderExpanded(row)` optionally returns detail content for a row; returning
 * a falsy value leaves the row collapsed. The caller owns the expanded state
 * and the control that toggles it.
 */
export function Table({ columns = [], rows = [], getRowKey, onRowClick, empty, renderExpanded, style = {} }) {
  const [hover, setHover] = React.useState(-1);

  return (
    <table style={{ width: '100%', borderCollapse: 'collapse', fontFamily: 'var(--font-body)', ...style }}>
      <thead>
        <tr>
          {columns.map((c, i) => (
            <th
              key={c.key || i}
              style={{
                padding: 'var(--space-3) var(--space-4)',
                textAlign: c.align || 'left',
                fontSize: 'var(--text-xs)',
                fontWeight: 'var(--font-semibold)',
                color: 'var(--text-tertiary)',
                textTransform: 'uppercase',
                letterSpacing: 'var(--tracking-wider)',
                background: 'var(--table-header-bg)',
                borderBottom: '1px solid var(--table-border)',
                whiteSpace: 'nowrap',
                width: c.width,
              }}
            >
              {c.header}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && empty && (
          <tr><td colSpan={columns.length} style={{ padding: 'var(--space-10)', textAlign: 'center', color: 'var(--text-tertiary)' }}>{empty}</td></tr>
        )}
        {rows.map((row, ri) => {
          const key = getRowKey ? getRowKey(row, ri) : ri;
          const detail = renderExpanded ? renderExpanded(row, ri) : null;
          return (
            <React.Fragment key={key}>
              <tr
                onMouseEnter={() => setHover(ri)}
                onMouseLeave={() => setHover(-1)}
                onClick={onRowClick ? () => onRowClick(row, ri) : undefined}
                style={{
                  background: hover === ri ? 'var(--table-row-hover)' : 'transparent',
                  cursor: onRowClick ? 'pointer' : 'default',
                  transition: 'background var(--duration-fast) var(--ease-in-out)',
                }}
              >
                {columns.map((c, ci) => (
                  <td
                    key={c.key || ci}
                    style={{
                      padding: 'var(--space-4)',
                      fontSize: 'var(--text-sm)',
                      color: 'var(--text-primary)',
                      textAlign: c.align || 'left',
                      borderBottom: ri === rows.length - 1 && !detail ? 'none' : '1px solid var(--border-subtle)',
                      verticalAlign: 'middle',
                    }}
                  >
                    {c.render ? c.render(row, ri) : row[c.key]}
                  </td>
                ))}
              </tr>
              {detail && (
                <tr>
                  <td colSpan={columns.length}
                    style={{ padding: 0, borderBottom: ri === rows.length - 1 ? 'none' : '1px solid var(--border-subtle)' }}>
                    {detail}
                  </td>
                </tr>
              )}
            </React.Fragment>
          );
        })}
      </tbody>
    </table>
  );
}
