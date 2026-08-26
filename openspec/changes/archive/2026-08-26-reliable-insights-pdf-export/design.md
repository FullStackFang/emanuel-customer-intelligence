## Context

Insights currently keeps the non-selected tab sections in the DOM with `display: none`, then relies on print media CSS to reveal them while the Rust `export_insights_pdf` command calls WebView2 `PrintToPdf`. Recharts measures responsive containers while their ancestors are hidden, and the native print call has no readiness handshake with React. The observed export is a multi-page PDF, but its chart visuals cannot be relied on to have been measured and painted before capture.

The report remains aggregate-only. Existing audit behavior, the restricted exports directory, and exclusion of named Watch List households are mandatory constraints.

## Goals / Non-Goals

**Goals:**

- Give PDF export a distinct report-rendering lifecycle that mounts every aggregate report card at a measurable printable size before native capture.
- Wait for every report chart to finish its layout pass, with a bounded failure path instead of exporting a partially rendered report.
- Keep the screen tab experience independent from report rendering and preserve the current aggregate data definitions.
- Add automated readiness coverage and an end-to-end PDF verification procedure for all aggregate sections and chart-bearing cards.

**Non-Goals:**

- Redesigning Insights charts, changing analytic calculations, or adding new data sources.
- Including household names, the named Watch List, or its CSV export in PDFs.
- Replacing WebView2 `PrintToPdf` with a separate reporting engine.

## Decisions

### Render an explicit off-screen report surface before capture

The React page will own a report-export state that renders every aggregate report section in a dedicated, measurable surface rather than revealing the hidden interactive tabs only through `@media print`. The surface will be visually suppressed during ordinary use without using `display: none`; export styles will place it at the printable width and keep it out of the interactive layout. Its cards will use the same aggregate data and chart components as Insights so the report does not duplicate analytical logic.

The export action will request this state first and wait until the surface reports that all chart containers have non-zero dimensions after a browser layout frame. Only then will it invoke the existing native command. When the report cannot become ready within a bounded timeout, the UI will show a clear export error and will not claim that a PDF was created.

Alternative: force every interactive tab visible only in print CSS. Rejected because it retains the zero-size measurement race that produced the regression. Alternative: generate charts and PDF entirely in Rust. Rejected because it duplicates presentation logic and adds a separate rendering stack for an existing WebView2 export path.

### Keep native output, audit, and privacy boundaries unchanged

`export_insights_pdf` will continue to audit the aggregate export and write only inside the application exports directory. The webview will not receive any new filesystem permission. The report surface will contain aggregate Risk content only; named Watch List content remains screen-only and is not mounted into the report surface.

Alternative: reuse the named-list export path to build a report data payload. Rejected because it would broaden named-data exposure and violate the existing aggregate/named separation.

### Test the readiness contract at two layers

Frontend tests will prove that starting an export renders all aggregate report sections, waits for non-zero chart dimensions, and does not call the native export command early. A desktop/manual integration check will retain the generated PDF artifact and verify that it contains all aggregate section headings and chart-bearing pages, including an export begun from each screen tab.

Alternative: test only the CSS selectors. Rejected because selector presence did not prove that responsive charts had measured or painted before capture.

## Risks / Trade-offs

- [The report surface increases DOM work during export] → It is mounted only for the export lifecycle and is removed after success or failure.
- [A chart can remain unready because of a rendering regression] → Readiness uses a bounded timeout and returns an actionable error instead of writing a misleading PDF.
- [The native WebView2 call remains asynchronous] → The existing command continues to await the native completion callback after frontend readiness has completed.
- [A duplicate report surface could drift from screen content] → Both surfaces compose existing shared cards/chart components from the same aggregate Insights payload.

## Migration Plan

Ship the new lifecycle behind the existing “Download PDF report” menu item; no data migration or external API change is required. Verify exports from every Insights tab using a populated local mirror before release. If a WebView2-specific issue appears, revert to the previous release; no persisted data, schema, or export-path format changes need rollback.

## Open Questions

None. The report remains Letter-sized WebView2 output using the current application-export directory and aggregate-only data scope.
