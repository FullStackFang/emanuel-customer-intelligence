## MODIFIED Requirements

### Requirement: Four analytical views
The system SHALL provide Overview, Jobs, Renewal & Engagement, and Risk views and SHALL compose every aggregate section and its chart visuals in a measurable report surface before PDF capture, regardless of the selected screen tab. The report surface SHALL exclude the named household Watch List.

#### Scenario: Staff navigate Insights
- **WHEN** staff select an Insights tab
- **THEN** the selected analytical view is displayed without changing the underlying fiscal-year definitions

#### Scenario: Staff create a PDF from a hidden-tab view
- **WHEN** staff export the Insights report while Jobs, Renewal & Engagement, or Risk is not the selected screen tab
- **THEN** the system lays out every aggregate report card and chart at non-zero printable dimensions before creating the PDF

#### Scenario: Staff create a PDF
- **WHEN** staff export the Insights report from any selected tab
- **THEN** the PDF contains every aggregate section and its chart visuals and no named household Watch List

#### Scenario: Report layout does not become ready
- **WHEN** an aggregate chart or report section has not reached a measurable printable layout before the export readiness timeout
- **THEN** the system reports that the PDF could not be rendered and does not report a successful export path
