## 1. Report-export regression coverage

- [x] 1.1 Add a frontend test seam for Insights PDF export that can observe report-surface mounting, chart dimensions, and the native export invocation.
- [x] 1.2 Add failing tests showing that export from each non-selected aggregate tab must wait for every chart-bearing report card to have non-zero dimensions.
- [x] 1.3 Add failing tests showing that an unready report surface produces an export error and does not invoke or report success from the native PDF command.

## 2. Deterministic report rendering

- [x] 2.1 Extract or compose the aggregate Insights cards into a report surface that shares the existing aggregate payload and chart components without mounting named Watch List content.
- [x] 2.2 Implement the export lifecycle that mounts the printable report surface, waits through a layout frame for all required charts to be measurable, then invokes the existing PDF command.
- [x] 2.3 Add bounded readiness failure handling and cleanup so the report surface is removed after a successful or failed export.
- [x] 2.4 Update print/report styling so the dedicated surface uses printable dimensions while ordinary screen tabs retain their current behavior.

## 3. Native export and privacy verification

- [x] 3.1 Preserve the existing aggregate PDF audit event and restricted exports-directory behavior without adding webview filesystem access.
- [x] 3.2 Verify no named Watch List household data is mounted into or emitted by the report surface.
- [x] 3.3 Verify the existing native PDF command is called only after frontend readiness and continues to surface native rendering failures.

## 4. End-to-end verification

- [x] 4.1 Run the focused frontend tests and the full TypeScript test suite.
- [ ] 4.2 Build and run the desktop app against a populated local mirror; export PDFs starting from Overview, Jobs, Renewal & Engagement, and Risk.
- [ ] 4.3 Inspect each generated PDF to confirm all aggregate section headings and chart visuals render, and confirm no named household Watch List appears.
