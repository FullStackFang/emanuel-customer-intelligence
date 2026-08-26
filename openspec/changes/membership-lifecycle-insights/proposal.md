## Why

The current Insights risk logic can label a household from one stale historical reason, such as Religious School ending many years ago, even when the household remains a member. Staff need longitudinal membership insights that distinguish current risk evidence from history, separate addressable churn from structural exits, and test joining "jobs" against observed retention and engagement.

## What Changes

- Add a household-by-fiscal-year analytical foundation combining membership spells, dues billing, school enrollment, and committee participation.
- Replace single-label resignation grouping with multi-label exit evidence and explicit Addressable Churn, Structural Exit, Conversion Loss, and Administrative or Unknown outcomes.
- Organize Insights into Overview, Jobs, Renewal & Engagement, and Risk views derived from the same analytical foundation.
- Treat stated joining reasons as Entry Job evidence and observed billing, school, and committee participation as distinct Relationship Anchors.
- Add capability-aware unavailable states when optional Salesforce objects have not been selected and synced.
- Add rolling, leakage-controlled churn model validation and suppress household rankings unless quality gates pass.
- Require a passing model plus two independent classes of recent evidence before a household can appear on the named Watch List.
- Keep household names out of aggregate PDFs and audit all named Watch List access and export.
- Add OpenSpec as a repository development dependency and make OpenSpec changes the default artifact for future feature and behavior specifications.

## Capabilities

### New Capabilities

- `membership-lifecycle-insights`: Longitudinal membership, Entry Job, renewal, engagement, and Exit Outcome analysis with source-capability and freshness handling.
- `validated-membership-risk`: Historically validated Addressable Churn factors, predictive evaluation, and an evidence-gated named Watch List.

### Modified Capabilities

None. The repository has no existing OpenSpec capability specifications; this change supersedes the standalone membership lifecycle design.

## Impact

- Rust Insights mart construction, aggregation, export, audit, and command payloads.
- React Insights navigation, charts, tables, unavailable states, report mode, and PDF composition.
- Local encrypted mirror tables derived from `Account`, `BillingStatement__c`, `BillingStatementLine__c`, `Class_Enrolment__c`, and `Committee_Membership__c`.
- A maintained Rust logistic-regression dependency selected during implementation planning and the local OpenSpec CLI development dependency.
- Repository workflow instructions and retirement of the superseded standalone lifecycle design document.
