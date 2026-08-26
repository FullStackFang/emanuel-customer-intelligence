## Why

The current Insights PDF export relies on print CSS to reveal charts from hidden tabs at the instant WebView2 snapshots the live page. That coupling can leave aggregate report visuals missing, unmeasured, or malformed, making the exported report less reliable than the Insights views staff see.

## What Changes

- Add a deterministic aggregate-report render lifecycle that lays out every report section and chart at printable dimensions before `PrintToPdf` runs.
- Make PDF readiness observable to the export action so it waits for the report layout rather than capturing a hidden-tab screen state.
- Preserve the current report scope: Overview, Jobs, Renewal & Engagement, and aggregate Risk content are included; named Watch List households remain excluded.
- Add automated coverage for report readiness and a visual/export verification path that checks all aggregate sections and chart-bearing cards in the generated PDF.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `membership-lifecycle-insights`: Require PDF export to contain a fully laid-out visual rendering of all aggregate Insights sections and charts, independent of the selected screen tab.

## Impact

- Affects `src/pages/InsightsPage.tsx`, Insights print/report styling, and the `export_insights_pdf` Tauri command contract.
- Retains the existing audited aggregate PDF export and approved export-directory behavior.
- Does not add Salesforce access, new source data, or named-household data to PDFs.
