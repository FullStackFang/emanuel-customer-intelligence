## Context

Insights currently derives membership aggregates and a rule-based at-risk list from a narrow Account mart. A household can be labeled from one stale historical condition even when its current membership spell continues. The local mirror also contains enrollment, committee, and billing-line data, while billing statements must still be selected and synced to link lines to households.

This change spans the encrypted SQLite analytical layer, Rust commands and exports, React presentation, report/PDF behavior, and local predictive evaluation. The webview remains untrusted, Salesforce remains read-only, and household names require audited access.

## Goals / Non-Goals

**Goals:**

- Create one longitudinal household-by-fiscal-year foundation for all aggregate and predictive analysis.
- Separate stated Entry Jobs, observed Relationship Anchors, and Exit Outcomes.
- Distinguish Addressable Churn from Structural Exit, Conversion Loss, and Administrative or Unknown Exit.
- Add billing, enrollment, and committee insights without interpreting unsynced or incomplete data as member behavior.
- Validate predictive signal on historical fiscal years before ranking current households.
- Require two independent recent evidence classes for the named Watch List.
- Preserve local-only processing, audit boundaries, aggregate PDF privacy, and source freshness.

**Non-Goals:**

- Contact-level or sensitive pastoral analysis.
- Causal claims about why a household joined or left.
- Automated outreach, Salesforce writes, or cloud model training.
- User-authored risk rules or editable validation thresholds.
- Treating absent dues billing as proof that membership ended.

## Decisions

### Use Membership Household and fiscal year as the analytical grain

`_m_household_fy` will contain one row per Membership Household and fiscal year. It will carry spell state, tenure, Entry Job flags, exit labels, dues evidence, enrollment, committee activity, Relationship Anchors, and source missingness. All aggregate views and model rows will read this mart.

The Account membership spell remains authoritative for membership history. Billing is Renewal Evidence and a consistency signal, not automatic membership truth.

Alternative considered: run direct aggregate queries over each Salesforce mirror table. This avoids a mart but duplicates derivation logic, repeatedly scans wide encrypted tables, and risks different definitions between charts and modeling.

### Rebuild both marts transactionally

The rebuild pipeline is:

```text
synced mirror tables
  -> normalized source rows
  -> _m_household
  -> _m_household_fy
  -> aggregate views and model dataset
```

Both marts and their freshness/capability metadata rebuild in one SQLite transaction. A failure rolls back to the previous valid analytical state. The existing `insights.rs` remains the command-facing facade; new normalization, mart, view, risk, and export logic is placed in focused Insights submodules.

Alternative considered: replace the current Insights module wholesale. Keeping the facade reduces command/API churn and avoids disturbing unrelated report and export work already staged.

### Gate optional source capabilities independently

Membership requires `Account`. Renewal requires both `BillingStatement__c` and `BillingStatementLine__c`; school requires `Class_Enrolment__c`; committee requires `Committee_Membership__c`. Missing optional sources disable only dependent content and produce a specific selection/sync instruction.

Billing lines join through their parent statement to Account. Qualifying membership dues exclude security fees, gifts, tuition, events, sales, and other non-dues products. Historical balance and received values are labeled eventual settlement because the mirror does not preserve historical as-of snapshots.

Only confirmed enrollment creates a school anchor. Withdrawn enrollment remains a separate outcome. Committee history uses valid membership dates, treats known far-future end placeholders as open-ended, and uses `IsActive__c` as the current-state authority.

### Preserve multi-label exit evidence

Resignation reasons become independent flags. A primary reporting outcome is assigned in this order:

1. Structural Exit: moved, deceased, elderly/ill.
2. Conversion Loss: introductory or age-limited tier ended without conversion.
3. Addressable Churn: non-payment, financial hardship, no longer engaged, displeased, or joined another synagogue.
4. Administrative or Unknown Exit: other, administrative, ambiguous, or uncoded.

Precedence controls reporting and model eligibility but never deletes secondary labels. A current household's old resignation date or reason belongs to an earlier membership spell and is not current risk evidence.

### Separate Entry Jobs from Relationship Anchors

`Join_Reason__c` provides multi-label Entry Job evidence. Confirmed school enrollment, dues renewal, and committee service provide observed Relationship Anchors. Insights compares retention and exit outcomes across both dimensions but describes them as associations and proxies, not direct knowledge of intent.

### Organize Insights into four views

The screen has Overview, Jobs, Renewal & Engagement, and Risk tabs. PDF/report mode composes every aggregate section regardless of the selected tab. Household names remain screen-only and are excluded from PDF output.

Overview covers membership flows, exit composition, and cohorts. Jobs covers Entry Job retention, multi-job relationships, school progression, and churn by years since Religious School. Renewal & Engagement covers dues state and observed anchors. Risk covers historical factors, model backtests, calibration, and the explicitly loaded named Watch List.

### Use validation-gated regularized logistic regression

Each model row represents a household active at the end of fiscal year N. The target is Addressable Churn in N+1. Structural, conversion, administrative, and unknown exits are excluded rather than labeled as negative retention outcomes.

Only evidence available by the end of N is eligible. Current resignation fields, future enrollment, post-cutoff payments, and final balance snapshots without defensible cutoff lineage are prohibited. The implementation will use a maintained Rust logistic-regression crate rather than a custom optimizer.

Rolling validation must include at least three completed test years. Every test year must contain at least 200 eligible households and 20 target exits. Optional feature families need at least 70% coverage in training and scoring with no more than a 15 percentage-point shift. A scoring model also requires ROC-AUC >= 0.65, top-decile lift >= 2.0, and a Brier score better than the constant baseline-rate predictor.

If an optional feature family fails coverage, it is removed and the complete backtest reruns. If the remaining model fails, aggregate Risk content remains available but household scores and names are suppressed.

### Add a second evidence gate for named households

A current household reaches the Watch List only when it is in a passing model's top risk decile and has at least two independent classes of current or recent Risk Evidence. Multiple fields derived from one event count once. Entry Job alone, missing billing coverage, a historical resignation reason, or Religious School ending more than two completed fiscal years ago cannot satisfy the evidence rule.

This intentionally favors a smaller review queue over false certainty. The model may identify aggregate associations that do not qualify any named household.

### Preserve command, audit, and export boundaries

The aggregate Insights payload gains the four view datasets, source capabilities, mart/model freshness, and validation results. Aggregate reads remain unaudited because they contain no names. Named Watch List load/export remains a separate audited command. CSV path restrictions remain unchanged, and PDF/report mode excludes the named list.

### Make OpenSpec the repository specification default

The official OpenSpec CLI is a pinned development dependency. A root `AGENTS.md` will require OpenSpec changes for new features, behavior changes, and architectural work, with small isolated bug fixes exempt unless the user requests a spec. The standalone lifecycle design is removed after equivalent OpenSpec artifacts validate.

## Risks / Trade-offs

- [Billing products are inconsistently named] -> Centralize dues classification, test observed families/names, and expose billing coverage.
- [Historical balances can leak future information] -> Label them eventual settlement and exclude them from modeling unless cutoff lineage is provable.
- [Sparse optional sources reduce model usefulness] -> Remove failing feature families, rerun validation, and suppress rankings when quality gates fail.
- [A model score can be mistaken for certainty] -> Require two evidence classes, show comparison and confidence, and label the list as a review queue.
- [Outcome precedence hides nuance] -> Retain all raw reason flags and use precedence only for primary display and model eligibility.
- [Transactional rebuild becomes slower] -> Normalize once into a narrow mart so subsequent view reads remain fast; retain the prior mart on failure.
- [OpenSpec adds process overhead] -> Exempt small isolated bug fixes and pin the local CLI for reproducibility.

## Migration Plan

1. Add normalization and mart tests that fail against the current behavior.
2. Extend the transactional rebuild to create `_m_household_fy` and capability metadata while preserving `_m_household` consumers.
3. Add aggregate payloads and source unavailable states behind the existing Insights command facade.
4. Add predictive dataset construction, leakage tests, rolling validation, and ranking suppression before exposing names.
5. Add the four-tab frontend and aggregate report/PDF composition while preserving existing exports.
6. Select and sync `BillingStatement__c`, rebuild, and compare aggregate counts against read-only mirror queries.
7. Remove compatibility fields only after frontend and export callers no longer consume them.

Rollback consists of reverting the new command payload consumers and rebuilding the prior `_m_household`; failed mart rebuilds already preserve the last valid analytical state.

## Open Questions

No product or modeling decisions remain open. The implementation plan will choose the maintained Rust logistic-regression crate after a dependency compatibility probe without changing the specified model behavior or validation gates.
