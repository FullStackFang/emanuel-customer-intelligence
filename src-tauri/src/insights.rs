//! Membership insights: fiscal-year math, the household mart, and the views
//! the Insights page renders. Reads the mirror only; never Salesforce.

use crate::progress::{noop, Reporter};
use crate::store::{ident, Store};
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

/// First month of the fiscal year (June). FY is labeled by the calendar year it ends in.
pub const FY_START_MONTH: u32 = 6;
/// Dates outside this window are placeholders (2199-…, 2991-…) or garbage.
const MIN_YEAR: i32 = 1900;
const MAX_YEAR: i32 = 2035;

pub fn fy_from_ymd(y: i32, m: u32) -> i32 {
    if m >= FY_START_MONTH {
        y + 1
    } else {
        y
    }
}

/// Fiscal year of a Salesforce date/datetime string (`YYYY-MM-DD…`), or None if
/// unparsable or outside the plausible window.
pub fn fy_of(date: &str) -> Option<i32> {
    let d = date.get(0..10)?;
    let nd = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
    let y = chrono::Datelike::year(&nd);
    if !(MIN_YEAR..=MAX_YEAR).contains(&y) {
        return None;
    }
    Some(fy_from_ymd(y, chrono::Datelike::month(&nd)))
}

pub fn current_fy() -> i32 {
    let today = chrono::Utc::now().date_naive();
    fy_from_ymd(
        chrono::Datelike::year(&today),
        chrono::Datelike::month(&today),
    )
}

/// (mart column key, phrase to look for in `Join_Reason__c`). Order is stable: it is
/// the index into `channel_flags` and the mart's `ch_*` columns.
pub const CHANNELS: [(&str, &str); 12] = [
    ("religious_school", "religious school"),
    ("nursery_school", "nursery school"),
    ("affiliation", "affiliation"),
    ("life_cycle", "life cycle event"),
    ("family", "to be with family"),
    ("young_professionals", "young professionals"),
    ("community", "community"),
    ("hhd_tickets", "high holy day"),
    ("streicker", "streicker"),
    ("clergy", "clergy"),
    ("worship", "worship"),
    ("move", "move"),
];

pub fn channel_flags(join_reason: Option<&str>) -> [bool; 12] {
    let mut out = [false; 12];
    if let Some(jr) = join_reason {
        let l = jr.to_lowercase();
        for (i, (_, phrase)) in CHANNELS.iter().enumerate() {
            out[i] = l.contains(phrase);
        }
    }
    out
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum ExitOutcome {
    Structural,
    Conversion,
    Addressable,
    Administrative,
}

const EXIT_REASON_RULES: [(&str, &str, ExitOutcome); 10] = [
    ("moved", "Moved", ExitOutcome::Structural),
    ("deceased", "Deceased", ExitOutcome::Structural),
    ("elderly", "Elderly / ill", ExitOutcome::Structural),
    ("aged out", "Aged out", ExitOutcome::Conversion),
    (
        "introductory",
        "Introductory tier ended",
        ExitOutcome::Conversion,
    ),
    ("non-payment", "Non-payment", ExitOutcome::Addressable),
    (
        "no longer engaged",
        "No longer engaged",
        ExitOutcome::Addressable,
    ),
    ("financial", "Financial hardship", ExitOutcome::Addressable),
    ("displeased", "Displeased", ExitOutcome::Addressable),
    (
        "another synagogue",
        "Joined another synagogue",
        ExitOutcome::Addressable,
    ),
];

fn exit_outcome_label(outcome: ExitOutcome) -> &'static str {
    match outcome {
        ExitOutcome::Structural => "Structural Exit",
        ExitOutcome::Conversion => "Conversion Loss",
        ExitOutcome::Addressable => "Addressable Churn",
        ExitOutcome::Administrative => "Administrative or Unknown Exit",
    }
}

pub fn exit_labels(raw: Option<&str>) -> Vec<&'static str> {
    let Some(r) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let l = r.to_lowercase();
    EXIT_REASON_RULES
        .iter()
        .filter_map(|(needle, label, _)| l.contains(needle).then_some(*label))
        .collect()
}

/// Primary Exit Outcome for a resignation reason. Structural Exit wins, then
/// Conversion Loss, then Addressable Churn, then Administrative or Unknown Exit.
pub fn reason_group(raw: Option<&str>) -> &'static str {
    let labels = exit_labels(raw);
    let outcome = if labels
        .iter()
        .any(|label| matches!(*label, "Moved" | "Deceased" | "Elderly / ill"))
    {
        ExitOutcome::Structural
    } else if labels
        .iter()
        .any(|label| matches!(*label, "Aged out" | "Introductory tier ended"))
    {
        ExitOutcome::Conversion
    } else if labels.iter().any(|label| {
        matches!(
            *label,
            "Non-payment"
                | "No longer engaged"
                | "Financial hardship"
                | "Displeased"
                | "Joined another synagogue"
        )
    }) {
        ExitOutcome::Addressable
    } else {
        ExitOutcome::Administrative
    };
    exit_outcome_label(outcome)
}

/// Presentation label for a resignation that cannot be tied to a specific,
/// actionable exit reason: deaths, uncoded, and administrative resignations.
pub const OTHER_EXIT: &str = "Other / not actionable";

/// Precedence for choosing the single primary fine-grained exit reason from a
/// multi-label resignation. Structural reasons win first (matching the family
/// precedence in `reason_group`), then Conversion, then Addressable — resolved
/// down to the specific reason. `Deceased`, uncoded, and administrative
/// resignations fold into `OTHER_EXIT`.
const EXIT_REASON_PRECEDENCE: [&str; 10] = [
    "Moved",
    "Elderly / ill",
    "Deceased",
    "Aged out",
    "Introductory tier ended",
    "Non-payment",
    "Financial hardship",
    "No longer engaged",
    "Displeased",
    "Joined another synagogue",
];

/// The single fine-grained exit reason surfaced for a resignation. Unlike
/// `reason_group` (the four coarse Exit Outcomes the churn model depends on),
/// this keeps the specific reason so staff can tell affordability from
/// disengagement. `Deceased`, uncoded, and administrative reasons become
/// `OTHER_EXIT`.
pub fn exit_reason_primary(raw: Option<&str>) -> &'static str {
    let labels = exit_labels(raw);
    for candidate in EXIT_REASON_PRECEDENCE {
        if labels.contains(&candidate) {
            return if candidate == "Deceased" { OTHER_EXIT } else { candidate };
        }
    }
    OTHER_EXIT
}

/// `LastYearAttendedRS__c` is "2025-2026" or "2007"; take the last 4-digit year.
pub fn parse_rs_year(s: Option<&str>) -> Option<i32> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    s.rsplit('-').next()?.trim().parse::<i32>().ok()
}

/// Billing products are classified before testing for dues so security fees and other
/// non-dues charges cannot accidentally become renewal evidence.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DuesClass {
    Membership,
    SecurityFee,
    Gift,
    Tuition,
    Event,
    Sale,
    Other,
}

pub fn dues_class(product_family: Option<&str>, product_name: Option<&str>) -> DuesClass {
    let text = [product_family, product_name]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if text.contains("security") {
        DuesClass::SecurityFee
    } else if text.contains("tuition") {
        DuesClass::Tuition
    } else if text.contains("event") || text.contains("ticket") {
        DuesClass::Event
    } else if text.contains("sale") || text.contains("merchandise") || text.contains("shop") {
        DuesClass::Sale
    } else if text.contains("gift") || text.contains("donation") {
        DuesClass::Gift
    } else if text.contains("dues") || text.contains("membership") {
        DuesClass::Membership
    } else {
        DuesClass::Other
    }
}

/// Minimal normalized billing statement: the household link and issue date that place
/// its lines in a household-year. Settlement lives on the lines, not here.
#[derive(Debug, Copy, Clone)]
pub struct BillingStatement<'a> {
    pub id: &'a str,
    pub household_id: Option<&'a str>,
    pub issued_at: Option<&'a str>,
}

#[derive(Debug, Copy, Clone)]
pub struct BillingStatementLine<'a> {
    pub statement_id: Option<&'a str>,
    pub product_family: Option<&'a str>,
    pub product_name: Option<&'a str>,
    pub amount: Option<f64>,
    /// Per-line eventual received amount and balance, so dues settlement is measured on
    /// the dues line alone rather than on a statement total that mixes in other products.
    pub received: Option<f64>,
    pub balance: Option<f64>,
}

impl<'a> BillingStatementLine<'a> {
    #[cfg(test)]
    fn dues(
        statement_id: &'a str,
        amount: f64,
        received: Option<f64>,
        balance: Option<f64>,
    ) -> Self {
        Self {
            statement_id: Some(statement_id),
            product_family: Some("Membership"),
            product_name: Some("Dues"),
            amount: Some(amount),
            received,
            balance,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BillingCoverage {
    Present,
    Missing,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SettlementState {
    Settled,
    PartiallySettled,
    Unsettled,
    Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DuesEvidence {
    pub coverage: BillingCoverage,
    pub dues_billed: f64,
    pub settlement: SettlementState,
}

impl DuesEvidence {
    pub fn settlement_label(self) -> &'static str {
        match self.settlement {
            SettlementState::Settled => "Eventual settlement: settled",
            SettlementState::PartiallySettled => "Eventual settlement: partially settled",
            SettlementState::Unsettled => "Eventual settlement: unsettled",
            SettlementState::Unknown => "Eventual settlement: unknown",
        }
    }
}

/// Eventual settlement of one dues line from its own received amount and balance.
fn line_settlement(line: &BillingStatementLine<'_>) -> SettlementState {
    match (line.received, line.balance) {
        (_, Some(balance)) if balance <= 0.0 => SettlementState::Settled,
        (Some(received), Some(_)) if received > 0.0 => SettlementState::PartiallySettled,
        (_, Some(_)) => SettlementState::Unsettled,
        (Some(received), None) if received > 0.0 => SettlementState::PartiallySettled,
        _ => SettlementState::Unknown,
    }
}

fn combined_settlement(states: impl Iterator<Item = SettlementState>) -> SettlementState {
    let states: Vec<_> = states.collect();
    if states.is_empty() || states.contains(&SettlementState::Unknown) {
        SettlementState::Unknown
    } else if states
        .iter()
        .all(|state| *state == SettlementState::Settled)
    {
        SettlementState::Settled
    } else if states
        .iter()
        .all(|state| *state == SettlementState::Unsettled)
    {
        SettlementState::Unsettled
    } else {
        SettlementState::PartiallySettled
    }
}

/// Derive household-year dues evidence only through a statement that identifies the
/// household. Settlement is read from the qualifying dues lines alone — never from the
/// statement total, which also covers security fees, tuition, and gifts. Final mirror
/// balances and received amounts are expressly eventual states.
pub fn dues_evidence(
    household_id: &str,
    fiscal_year: i32,
    statements: &[BillingStatement<'_>],
    lines: &[BillingStatementLine<'_>],
) -> DuesEvidence {
    DuesIndex::build(statements, lines).evidence(household_id, fiscal_year)
}

/// Membership dues lines grouped by household-year, built once per rebuild so each
/// household-year is one hash lookup instead of a scan of every statement and line.
/// The evidence is identical to scanning: the same lines qualify, in the same order.
pub struct DuesIndex<'a> {
    /// Qualifying (membership-class) lines per `(household_id, fy)`, in original line order.
    qualifying: std::collections::HashMap<(&'a str, i32), Vec<BillingStatementLine<'a>>>,
}

impl<'a> DuesIndex<'a> {
    pub fn build(statements: &[BillingStatement<'a>], lines: &[BillingStatementLine<'a>]) -> Self {
        // Statement id -> the household-years it places lines in. A line matches a
        // statement by exact id, and `""` is a legal id. Statements that name no household
        // or carry no parseable date never match anything, so they are left out.
        let mut household_years: std::collections::HashMap<&'a str, Vec<(&'a str, i32)>> =
            std::collections::HashMap::with_capacity(statements.len());
        for statement in statements {
            let (Some(household_id), Some(fy)) = (
                statement.household_id,
                statement.issued_at.and_then(fy_of),
            ) else {
                continue;
            };
            let targets = household_years.entry(statement.id).or_default();
            if !targets.contains(&(household_id, fy)) {
                targets.push((household_id, fy));
            }
        }
        let mut qualifying: std::collections::HashMap<(&'a str, i32), Vec<BillingStatementLine<'a>>> =
            std::collections::HashMap::new();
        for line in lines {
            let Some(targets) = line.statement_id.and_then(|id| household_years.get(id)) else {
                continue;
            };
            if dues_class(line.product_family, line.product_name) != DuesClass::Membership {
                continue;
            }
            for key in targets {
                qualifying.entry(*key).or_default().push(*line);
            }
        }
        Self { qualifying }
    }

    pub fn evidence(&self, household_id: &str, fiscal_year: i32) -> DuesEvidence {
        let Some(qualifying) = self.qualifying.get(&(household_id, fiscal_year)) else {
            return DuesEvidence {
                coverage: BillingCoverage::Missing,
                dues_billed: 0.0,
                settlement: SettlementState::Unknown,
            };
        };
        DuesEvidence {
            coverage: BillingCoverage::Present,
            dues_billed: qualifying.iter().filter_map(|line| line.amount).sum(),
            settlement: combined_settlement(qualifying.iter().map(line_settlement)),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SchoolType {
    Nursery,
    Religious,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RelationshipAnchor {
    NurserySchool,
    ReligiousSchool,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum EnrollmentOutcome {
    Confirmed,
    Withdrawn,
    Other,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct NormalizedEnrollment {
    pub school: Option<SchoolType>,
    pub outcome: EnrollmentOutcome,
    pub anchor: Option<RelationshipAnchor>,
}

/// Normalize a school enrollment without treating a pending or withdrawn record as
/// observed participation. The mirror adapter supplies the source-specific labels.
pub fn normalize_enrollment(
    school_name: Option<&str>,
    status: Option<&str>,
) -> NormalizedEnrollment {
    let school = school_name.map(str::to_lowercase).and_then(|name| {
        if name.contains("nursery") {
            Some(SchoolType::Nursery)
        } else if name.contains("religious") {
            Some(SchoolType::Religious)
        } else {
            None
        }
    });
    let outcome = match status.map(str::trim).filter(|status| !status.is_empty()) {
        Some(status)
            if status.eq_ignore_ascii_case("confirmed")
                || status.eq_ignore_ascii_case("enrolled") =>
        {
            EnrollmentOutcome::Confirmed
        }
        Some(status) if status.eq_ignore_ascii_case("withdrawn") => EnrollmentOutcome::Withdrawn,
        _ => EnrollmentOutcome::Other,
    };
    let anchor = match (school, outcome) {
        (Some(SchoolType::Nursery), EnrollmentOutcome::Confirmed) => {
            Some(RelationshipAnchor::NurserySchool)
        }
        (Some(SchoolType::Religious), EnrollmentOutcome::Confirmed) => {
            Some(RelationshipAnchor::ReligiousSchool)
        }
        _ => None,
    };
    NormalizedEnrollment {
        school,
        outcome,
        anchor,
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct NormalizedCommittee {
    pub start_fy: Option<i32>,
    pub end_fy: Option<i32>,
    pub open_ended: bool,
    pub current_active: bool,
}

fn is_far_future_placeholder(date: &str) -> bool {
    date.get(0..4)
        .and_then(|year| year.parse::<i32>().ok())
        .is_some_and(|year| year >= 2100)
}

/// `IsActive__c` is the source of truth for current committee activity. End-date
/// placeholders remain historical context only and are normalized to open-ended.
pub fn normalize_committee(
    started_at: Option<&str>,
    ended_at: Option<&str>,
    is_active: Option<&str>,
) -> NormalizedCommittee {
    let open_ended = ended_at.is_some_and(is_far_future_placeholder);
    NormalizedCommittee {
        start_fy: started_at.and_then(fy_of),
        end_fy: (!open_ended).then(|| ended_at.and_then(fy_of)).flatten(),
        open_ended,
        current_active: matches!(is_active, Some("true") | Some("1")),
    }
}

pub const MART: &str = "_m_household";
pub const MART_FY: &str = "_m_household_fy";

/// Bumped whenever the mart column layout changes so that existing databases with an
/// older `_m_household_fy`/`_m_household` schema are rebuilt rather than read as-is.
pub const MART_SCHEMA_VERSION: i64 = 6;

/// Account columns the mart derives from. A missing one nulls what depends on it and is
/// reported in `RebuildInfo::unavailable`; `Type` and `IsATempleMember__c` are mandatory.
pub const REQUIRED_COLUMNS: [&str; 16] = [
    "Id",
    "Name",
    "Type",
    "IsATempleMember__c",
    "IsResigned__c",
    "Join_Date__c",
    "OriginalJoinDate__c",
    "LastResignDate__c",
    "Sub_Type__c",
    "Member_Category__c",
    "Join_Reason__c",
    "Resign_Reason__c",
    "FormerReligiousSchoolStudents__c",
    "ActiveReligiousSchoolStudents__c",
    "WasEverNSAffiliated__c",
    "LastYearAttendedRS__c",
];

#[derive(Serialize, Debug, Clone)]
pub struct RebuildInfo {
    pub households: usize,
    pub unavailable: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct SourceCapability {
    pub key: String,
    pub available: bool,
    pub required_objects: Vec<String>,
    pub mirrored_columns: Vec<String>,
    pub last_synced_at: Option<String>,
    pub unavailable_reason: Option<String>,
}

/// Every mirror object the mart reads (`rebuild_with` and the `apply_*` adapters). A sync
/// of any other object cannot change the mart, so it neither marks it stale nor triggers a
/// rebuild. Keep in step with the `mirror_rows`/`mirror_columns` calls in this module.
pub const MART_SOURCE_OBJECTS: &[&str] = &[
    "Account",
    "BillingStatement__c",
    "BillingStatementLine__c",
    "Class_Enrolment__c",
    "Committee_Membership__c",
];

const SOURCE_CAPABILITIES: [(&str, &[&str]); 4] = [
    ("membership", &["Account"]),
    (
        "renewal",
        &["BillingStatement__c", "BillingStatementLine__c"],
    ),
    ("school", &["Class_Enrolment__c"]),
    ("committee", &["Committee_Membership__c"]),
];

/// Optional sources are usable only after every source table required by that
/// capability has been synced. A selected-but-unsynced object is not evidence.
pub fn source_capabilities(store: &Store) -> Result<Vec<SourceCapability>> {
    let objects = store.list_objects()?;
    let mut capabilities: Vec<SourceCapability> = SOURCE_CAPABILITIES
        .iter()
        .map(|(key, required)| {
            let source_rows: Vec<_> = required
                .iter()
                .filter_map(|name| objects.iter().find(|object| object.name == *name))
                .collect();
            let available = source_rows.len() == required.len()
                && source_rows
                    .iter()
                    .all(|object| object.last_synced_at.is_some());
            let last_synced_at = available
                .then(|| {
                    source_rows
                        .iter()
                        .filter_map(|object| object.last_synced_at.clone())
                        .min()
                })
                .flatten();
            let required_objects = required.iter().map(|name| (*name).to_string()).collect();
            let mirrored_columns = required
                .iter()
                .map(|name| store.mirror_columns(name))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
            Ok(SourceCapability {
                key: (*key).to_string(),
                available,
                required_objects,
                mirrored_columns,
                last_synced_at,
                unavailable_reason: (!available)
                    .then(|| format!("Select and sync {}", required.join(" and "))),
            })
        })
        .collect::<Result<_>>()?;
    let account = objects.iter().find(|object| object.name == "Account");
    let account_field_available = account.is_some_and(|object| object.last_synced_at.is_some())
        && store.mirror_columns("Account")?.iter().any(|column| column == "BillingPostalCode")
        && store.allowed_fields("Account")?.contains("BillingPostalCode");
    let account_zip_available = account_field_available && {
        let mut statement = store.conn().prepare("SELECT \"BillingPostalCode\" FROM \"Account\" WHERE \"Type\" = 'Member Family'")?;
        let values = statement.query_map([], |row| row.get::<_, Option<String>>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values.iter().any(|value| normalize_zip(value.as_deref()).is_some())
    };
    let statement_zip_available = !billing_statement_zips(store)?.is_empty();
    let geo_available = statement_zip_available || account_zip_available;
    let statement_object = objects.iter().find(|object| object.name == "BillingStatement__c");
    let mut mirrored_columns = Vec::new();
    if statement_zip_available {
        mirrored_columns.push("BillingStatement__c.AddressPostalCode__c".into());
    }
    if account_zip_available {
        mirrored_columns.push("Account.BillingPostalCode".into());
    }
    capabilities.push(SourceCapability {
        key: "zip_attrition".into(), available: geo_available,
        required_objects: vec!["BillingStatement__c".into(), "Account".into()],
        mirrored_columns,
        last_synced_at: geo_available.then(|| {
            [statement_object, account]
                .into_iter()
                .flatten()
                .filter_map(|object| object.last_synced_at.clone())
                .max()
        }).flatten(),
        unavailable_reason: (!geo_available).then(|| if account_field_available
            || statement_object.is_some_and(|object| object.last_synced_at.is_some()) {
            "Billing statement and Account postal sources have no normalizable five-digit ZIP values for member households.".into()
        } else {
            "Select and sync BillingStatement__c AddressPostalCode__c or Account BillingPostalCode to enable ZIP attrition.".into()
        }),
    });
    Ok(capabilities)
}

/// One household from the mart. Everything the views need, nothing else.
#[derive(Debug, Clone, Default)]
pub struct Hh {
    pub account_id: String,
    pub name: Option<String>,
    pub is_current: bool,
    pub is_resigned: bool,
    pub join_fy: Option<i32>,
    pub cohort_fy: Option<i32>,
    pub resign_fy: Option<i32>,
    pub resigned_unknown_date: bool,
    pub bad_join_date: bool,
    pub rejoined: bool,
    pub tier: Option<String>,
    pub category: Option<String>,
    pub join_reason: Option<String>,
    /// Normalized five-digit ZIP derived locally from a billing statement, with Account fallback. Never raw postal data.
    pub zip: Option<String>,
    pub ch: [bool; 12],
    pub rs_family: bool,
    pub ns_family: bool,
    pub active_rs_students: i64,
    pub last_rs_year: Option<i32>,
    pub resign_reason_group: String,
    /// Single primary fine-grained exit reason for presentation (see
    /// `exit_reason_primary`). "(not coded)" for current members. Distinct from
    /// `resign_reason_group`, which the churn model owns.
    pub exit_reason: String,
}

fn mart_ddl() -> String {
    let flags = CHANNELS
        .iter()
        .map(|(k, _)| format!("ch_{k} INTEGER NOT NULL DEFAULT 0"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE TABLE {MART}(
           account_id TEXT PRIMARY KEY, name TEXT,
           is_current INTEGER NOT NULL, is_resigned INTEGER NOT NULL,
           join_fy INTEGER, cohort_fy INTEGER, resign_fy INTEGER,
           resigned_unknown_date INTEGER NOT NULL, bad_join_date INTEGER NOT NULL, rejoined INTEGER NOT NULL,
           tier TEXT, category TEXT, join_reason TEXT, zip TEXT, {flags},
           rs_family INTEGER NOT NULL, ns_family INTEGER NOT NULL, active_rs_students INTEGER NOT NULL,
           last_rs_year INTEGER, resign_reason_group TEXT NOT NULL, exit_reason TEXT NOT NULL)"
    )
}

fn as_bool(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("true") | Some("1"))
}
fn as_num(v: &Option<String>) -> f64 {
    v.as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn normalize_zip(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let zip = value.get(0..5)?;
    let suffix = value.get(5..).unwrap_or_default();
    (zip.bytes().all(|byte| byte.is_ascii_digit())
        && (suffix.is_empty() || (suffix.len() == 5 && suffix.starts_with('-') && suffix[1..].bytes().all(|byte| byte.is_ascii_digit()))))
        .then(|| zip.to_string())
}

/// Derive one mart row from the raw Account values (positional per REQUIRED_COLUMNS).
fn derive(raw: &[Option<String>; 16], zip: Option<String>) -> Hh {
    let [id, name, _ty, is_member, is_resigned, join, orig, resign, tier, category, reason, resign_reason, former_rs, active_rs, ever_ns, last_rs] =
        raw;
    let is_current = as_bool(is_member);
    let is_resigned = as_bool(is_resigned);
    let join_fy = join.as_deref().and_then(fy_of);
    let bad_join_date = join.is_some() && join_fy.is_none();
    let orig_fy = orig.as_deref().and_then(fy_of);
    let cohort_fy = orig_fy.or(join_fy);
    let resign_fy_raw = resign.as_deref().and_then(fy_of);
    // A current member's LastResignDate is a past spell, not a resignation.
    let resign_fy = if is_current { None } else { resign_fy_raw };
    let resigned_unknown_date = !is_current && is_resigned && resign_fy.is_none();
    // ISO dates compare lexically, so string order is date order.
    let rejoined = matches!((orig.as_deref(), join.as_deref()), (Some(o), Some(j)) if o < j);
    Hh {
        account_id: id.clone().unwrap_or_default(),
        name: name.clone(),
        is_current,
        is_resigned,
        join_fy,
        cohort_fy,
        resign_fy,
        resigned_unknown_date,
        bad_join_date,
        rejoined,
        tier: tier.clone(),
        category: category.clone(),
        join_reason: reason.clone().filter(|s| !s.trim().is_empty()),
        zip,
        ch: channel_flags(reason.as_deref()),
        rs_family: as_num(former_rs) > 0.0 || as_num(active_rs) > 0.0,
        ns_family: as_bool(ever_ns),
        active_rs_students: as_num(active_rs) as i64,
        last_rs_year: parse_rs_year(last_rs.as_deref()),
        resign_reason_group: if is_current {
            "(not coded)".into()
        } else {
            reason_group(resign_reason.as_deref()).to_string()
        },
        exit_reason: if is_current {
            "(not coded)".into()
        } else {
            exit_reason_primary(resign_reason.as_deref()).to_string()
        },
    }
}

fn mart_fy_ddl() -> String {
    let entry_jobs = CHANNELS
        .iter()
        .map(|(key, _)| format!("entry_job_{key} INTEGER NOT NULL DEFAULT 0"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("CREATE TABLE _m_household_fy(
       account_id TEXT NOT NULL, fy INTEGER NOT NULL,
       active_end_of_fy INTEGER NOT NULL, joined_this_fy INTEGER NOT NULL, resigned_this_fy INTEGER NOT NULL,
       tenure_years INTEGER, exit_reason TEXT, entry_job_count INTEGER NOT NULL, {entry_jobs},
       anchor_dues INTEGER NOT NULL DEFAULT 0, anchor_nursery INTEGER NOT NULL DEFAULT 0,
       anchor_religious INTEGER NOT NULL DEFAULT 0, anchor_committee INTEGER NOT NULL DEFAULT 0,
       dues_coverage_missing INTEGER NOT NULL DEFAULT 0, dues_settlement TEXT,
       renewal_observed INTEGER NOT NULL DEFAULT 0, school_observed INTEGER NOT NULL DEFAULT 0,
       committee_observed INTEGER NOT NULL DEFAULT 0,
       PRIMARY KEY(account_id, fy))")
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HhFy {
    pub account_id: String,
    pub fy: i32,
    pub active_end_of_fy: bool,
    pub joined_this_fy: bool,
    pub resigned_this_fy: bool,
    pub tenure_years: Option<i32>,
    pub exit_reason: Option<String>,
    pub entry_job_count: i64,
    pub entry_jobs: [bool; 12],
    // Relationship Anchors observed in this fiscal year (populated by the optional
    // mirror sources; all false when a source is unavailable).
    pub anchor_dues: bool,
    pub anchor_nursery: bool,
    pub anchor_religious: bool,
    pub anchor_committee: bool,
    /// Active this fiscal year with no qualifying dues line while Renewal is available:
    /// billing coverage is missing, which is not evidence of non-renewal.
    pub dues_coverage_missing: bool,
    /// Eventual-settlement label for this fiscal year's dues, when billed.
    pub dues_settlement: Option<String>,
    /// Whether each optional source carried any rows for this fiscal year. Set by the
    /// anchor adapters; false for a year the source has no data for (e.g. billing
    /// statements before FY2023), which the churn model treats as uncovered rather than
    /// as a zero anchor. Independent of `dues_coverage_missing`, which is a real signal
    /// on a renewal-observed year.
    pub renewal_observed: bool,
    pub school_observed: bool,
    pub committee_observed: bool,
}

impl HhFy {
    /// Distinct Relationship Anchors this household held in this fiscal year.
    pub fn anchor_count(&self) -> i64 {
        [
            self.anchor_dues,
            self.anchor_nursery,
            self.anchor_religious,
            self.anchor_committee,
        ]
        .into_iter()
        .filter(|held| *held)
        .count() as i64
    }
}

fn household_year_rows(hh: &[Hh], through_fy: i32) -> Vec<HhFy> {
    hh.iter()
        .flat_map(|household| {
            let Some(join_fy) = household.join_fy else {
                return Vec::new();
            };
            let end_fy = household.resign_fy.unwrap_or(through_fy).min(through_fy);
            (join_fy..=end_fy)
                .map(|fy| HhFy {
                    account_id: household.account_id.clone(),
                    fy,
                    active_end_of_fy: member_in(household, fy),
                    joined_this_fy: household.join_fy == Some(fy),
                    resigned_this_fy: household.resign_fy == Some(fy),
                    tenure_years: Some(fy - join_fy + 1),
                    exit_reason: household
                        .resign_fy
                        .filter(|resign_fy| *resign_fy == fy)
                        .map(|_| household.exit_reason.clone()),
                    entry_job_count: household.ch.iter().filter(|flag| **flag).count() as i64,
                    entry_jobs: household.ch,
                    // Anchors come from the optional mirror sources, applied after this base.
                    ..Default::default()
                })
                .collect()
        })
        .collect()
}

// ── Relationship Anchor ingestion ───────────────────────────────────────────
//
// The optional Salesforce objects (BillingStatement__c, BillingStatementLine__c,
// Class_Enrolment__c, Committee_Membership__c) were synced and their real schema
// confirmed against the mirror on 2026-08-26 (task 8.2). The `*_FIELDS` lists below
// are pinned to the confirmed columns; each `Cands` slice holds the real column name.
// Each source stays capability-gated, so an unsynced object leaves its anchors empty
// and dependent views report unavailable. If Salesforce renames one of these columns,
// re-verify against `store.mirror_columns(object)` and update the matching list.

type Cands = &'static [&'static str];

/// Candidate columns for a billing statement. `household` links to `Account.Id`.
struct StatementFields {
    id: Cands,
    household: Cands,
    date: Cands,
    postal: Cands,
}
const STATEMENT_FIELDS: StatementFields = StatementFields {
    id: &["Id"],
    household: &["Account__c"],
    date: &["Date__c"],
    postal: &["AddressPostalCode__c"],
};

/// Candidate columns for a billing statement line. `parent` links to the statement Id.
struct LineFields {
    parent: Cands,
    product_family: Cands,
    product_name: Cands,
    amount: Cands,
    received: Cands,
    balance: Cands,
}
const LINE_FIELDS: LineFields = LineFields {
    parent: &["BillingStatement__c"],
    // Both family and name feed `dues_class`: the family groups the line ("Dues",
    // "Gift", "Nursery"…) and the name separates security fees and tuition that also
    // sit under the "Dues" family, so both must be the real product columns — not the
    // record `Name`, which carries no product text.
    product_family: &["Billing_PrimaryProductFamily__c"],
    product_name: &["Billing_PrimaryProductName__c"],
    amount: &["Charges__c"],
    // Per-line eventual settlement. Dues settlement is read from the dues lines alone;
    // the statement-level totals also cover security fees, tuition, and gifts.
    received: &["Billing_ReceivedAmount__c"],
    balance: &["Billing_Balance__c"],
};

/// Candidate columns for a class enrolment. The school type comes from the source's
/// authoritative `IsNursery__c` / `IsReligious__c` flags rather than a free-text name.
struct EnrolmentFields {
    household: Cands,
    is_nursery: Cands,
    is_religious: Cands,
    status: Cands,
    year: Cands,
}
const ENROLMENT_FIELDS: EnrolmentFields = EnrolmentFields {
    household: &["Account__c"],
    is_nursery: &["IsNursery__c"],
    is_religious: &["IsReligious__c"],
    status: &["Status__c"],
    // "2024-2025" school-year strings; `parse_rs_year` takes the end year.
    year: &["Academic_Year__c"],
};

/// Candidate columns for a committee membership.
struct CommitteeFields {
    household: Cands,
    start: Cands,
    end: Cands,
    is_active: Cands,
}
const COMMITTEE_FIELDS: CommitteeFields = CommitteeFields {
    household: &["Account__c"],
    start: &["Member_From__c"],
    end: &["Member_To__c"],
    is_active: &["IsActive__c"],
};

/// First mirror column (case-insensitive) matching any candidate name.
fn resolve_col(columns: &[String], candidates: &[&str]) -> Option<String> {
    columns
        .iter()
        .find(|col| candidates.iter().any(|cand| col.eq_ignore_ascii_case(cand)))
        .cloned()
}

fn cap_available(caps: &[SourceCapability], key: &str) -> bool {
    caps.iter().any(|c| c.key == key && c.available)
}

/// A mirror boolean is stored as text; treat "true"/"1" as set.
fn is_flag_set(value: Option<&str>) -> bool {
    matches!(value, Some("true") | Some("1"))
}

/// Read a resolved field from a mirror row map.
fn field<'a>(
    row: &'a std::collections::HashMap<String, Option<String>>,
    col: &Option<String>,
) -> Option<&'a str> {
    col.as_ref()
        .and_then(|c| row.get(c))
        .and_then(|v| v.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The date component of a statement issue timestamp. Unlike `fy_of`, this accepts
/// any valid statement date because source ordering must not depend on the reporting
/// fiscal-year window.
fn statement_issue_date(value: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(value.get(0..10)?, "%Y-%m-%d").ok()
}

/// Latest normalizable billing-statement ZIP by its direct Account link. The
/// BilledToOtherAccountId__c field is intentionally never read: it is a bill-to
/// routing attribute, not a household-geography relationship.
fn billing_statement_zips(store: &Store) -> Result<std::collections::HashMap<String, String>> {
    let columns = store.mirror_columns("BillingStatement__c")?;
    let household = resolve_col(&columns, STATEMENT_FIELDS.household);
    let date = resolve_col(&columns, STATEMENT_FIELDS.date);
    let postal = resolve_col(&columns, STATEMENT_FIELDS.postal);
    let id = resolve_col(&columns, STATEMENT_FIELDS.id);
    let allowed = store.allowed_fields("BillingStatement__c")?;
    let usable = [household.as_ref(), date.as_ref(), postal.as_ref(), id.as_ref()]
        .into_iter()
        .flatten()
        .all(|column| allowed.contains(column));
    if !usable || household.is_none() || date.is_none() || postal.is_none() || id.is_none() {
        return Ok(Default::default());
    }

    let mut latest: std::collections::HashMap<String, (chrono::NaiveDate, String, String)> = Default::default();
    for row in store.mirror_rows("BillingStatement__c")? {
        let (Some(account_id), Some(issued_at), Some(zip), Some(id)) = (
            field(&row, &household),
            field(&row, &date).and_then(statement_issue_date),
            normalize_zip(field(&row, &postal)),
            field(&row, &id),
        ) else {
            continue;
        };
        let candidate = (issued_at, id.to_string(), zip);
        let replace = latest
            .get(account_id)
            .is_none_or(|current| (candidate.0, candidate.1.as_str()) > (current.0, current.1.as_str()));
        if replace {
            latest.insert(account_id.to_string(), candidate);
        }
    }
    Ok(latest.into_iter().map(|(account_id, (_, _, zip))| (account_id, zip)).collect())
}

fn apply_billing_statement_zips(store: &Store, households: &mut [Hh]) -> Result<()> {
    let billing_zips = billing_statement_zips(store)?;
    for household in households {
        if let Some(zip) = billing_zips.get(&household.account_id) {
            household.zip = Some(zip.clone());
        }
    }
    Ok(())
}

/// Populate Relationship Anchors on the household-year rows from the optional mirror
/// sources. Capability-gated: an unsynced source leaves its anchors empty.
fn apply_anchor_sources(
    store: &Store,
    rows: &mut [HhFy],
    progress: &mut Reporter<'_>,
) -> Result<()> {
    let caps = source_capabilities(store)?;
    let through = current_fy();
    if cap_available(&caps, "renewal") {
        apply_dues(store, rows, progress)?;
    }
    if cap_available(&caps, "school") {
        apply_school(store, rows, progress)?;
    }
    if cap_available(&caps, "committee") {
        apply_committee(store, rows, through, progress)?;
    }
    Ok(())
}

/// (account_id, fy) -> row index, so a mirror row can find its household-year.
fn row_index(rows: &[HhFy]) -> std::collections::HashMap<(String, i32), usize> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| ((r.account_id.clone(), r.fy), i))
        .collect()
}

/// Mark every household-year in a fiscal year the source carried rows for as observed
/// for that family. A year with no source rows stays unobserved, so the churn model's
/// coverage gate can tell "no data that year" from "data present, anchor absent".
fn mark_observed(
    rows: &mut [HhFy],
    observed_fys: &std::collections::HashSet<i32>,
    flag: impl Fn(&mut HhFy) -> &mut bool,
) {
    for row in rows.iter_mut() {
        if observed_fys.contains(&row.fy) {
            *flag(row) = true;
        }
    }
}

fn apply_dues(store: &Store, rows: &mut [HhFy], progress: &mut Reporter<'_>) -> Result<()> {
    let statement_cols = store.mirror_columns("BillingStatement__c")?;
    let line_cols = store.mirror_columns("BillingStatementLine__c")?;
    let s_id = resolve_col(&statement_cols, STATEMENT_FIELDS.id);
    let s_hh = resolve_col(&statement_cols, STATEMENT_FIELDS.household);
    let s_date = resolve_col(&statement_cols, STATEMENT_FIELDS.date);
    let l_parent = resolve_col(&line_cols, LINE_FIELDS.parent);
    let l_fam = resolve_col(&line_cols, LINE_FIELDS.product_family);
    let l_name = resolve_col(&line_cols, LINE_FIELDS.product_name);
    let l_amt = resolve_col(&line_cols, LINE_FIELDS.amount);
    let l_recv = resolve_col(&line_cols, LINE_FIELDS.received);
    let l_bal = resolve_col(&line_cols, LINE_FIELDS.balance);
    // Without the household and parent join keys there is no defensible link to a
    // household-year; leave dues anchors empty rather than guess.
    if s_id.is_none() || s_hh.is_none() || l_parent.is_none() {
        return Ok(());
    }
    let statement_rows = store.mirror_rows("BillingStatement__c")?;
    let line_rows = store.mirror_rows("BillingStatementLine__c")?;
    let statements: Vec<BillingStatement<'_>> = statement_rows
        .iter()
        .map(|r| BillingStatement {
            id: field(r, &s_id).unwrap_or_default(),
            household_id: field(r, &s_hh),
            issued_at: field(r, &s_date),
        })
        .collect();
    let lines: Vec<BillingStatementLine<'_>> = line_rows
        .iter()
        .map(|r| BillingStatementLine {
            statement_id: field(r, &l_parent),
            product_family: field(r, &l_fam),
            product_name: field(r, &l_name),
            amount: field(r, &l_amt).and_then(|v| v.parse().ok()),
            received: field(r, &l_recv).and_then(|v| v.parse().ok()),
            balance: field(r, &l_bal).and_then(|v| v.parse().ok()),
        })
        .collect();
    // A fiscal year is renewal-observed when any statement was issued in it. This is
    // independent of `dues_coverage_missing` below, which stays a per-household signal.
    let observed_fys: std::collections::HashSet<i32> = statements
        .iter()
        .filter_map(|s| s.issued_at.and_then(fy_of))
        .collect();
    mark_observed(rows, &observed_fys, |r| &mut r.renewal_observed);
    let index = DuesIndex::build(&statements, &lines);
    let total = rows.len() as u64;
    progress.tick(0, Some(total));
    for (i, row) in rows.iter_mut().enumerate() {
        progress.tick(i as u64 + 1, Some(total));
        let evidence = index.evidence(&row.account_id, row.fy);
        match evidence.coverage {
            BillingCoverage::Present => {
                row.anchor_dues = true;
                row.dues_settlement = Some(evidence.settlement_label().to_string());
            }
            // Active membership with no qualifying dues line is missing coverage, which
            // the spec forbids interpreting as non-renewal.
            BillingCoverage::Missing if row.active_end_of_fy => {
                row.dues_coverage_missing = true;
            }
            BillingCoverage::Missing => {}
        }
    }
    Ok(())
}

fn apply_school(store: &Store, rows: &mut [HhFy], progress: &mut Reporter<'_>) -> Result<()> {
    let cols = store.mirror_columns("Class_Enrolment__c")?;
    let hh = resolve_col(&cols, ENROLMENT_FIELDS.household);
    let is_nursery = resolve_col(&cols, ENROLMENT_FIELDS.is_nursery);
    let is_religious = resolve_col(&cols, ENROLMENT_FIELDS.is_religious);
    let status = resolve_col(&cols, ENROLMENT_FIELDS.status);
    let year = resolve_col(&cols, ENROLMENT_FIELDS.year);
    if hh.is_none() || year.is_none() {
        return Ok(());
    }
    let index = row_index(rows);
    let mut observed_fys = std::collections::HashSet::new();
    let enrolment_rows = store.mirror_rows("Class_Enrolment__c")?;
    let total = enrolment_rows.len() as u64;
    for (i, r) in enrolment_rows.into_iter().enumerate() {
        progress.tick(i as u64 + 1, Some(total));
        let Some(account_id) = field(&r, &hh) else {
            continue;
        };
        // Enrolment school-year strings run "2024-2025"; the fiscal year is the end year.
        let Some(fy) = parse_rs_year(field(&r, &year)) else {
            continue;
        };
        observed_fys.insert(fy);
        // The source's own flags label the school type; hand the normalizer a canonical
        // label instead of parsing a free-text class name.
        let school_label = if is_flag_set(field(&r, &is_nursery)) {
            Some("nursery")
        } else if is_flag_set(field(&r, &is_religious)) {
            Some("religious")
        } else {
            None
        };
        let enrolment = normalize_enrollment(school_label, field(&r, &status));
        let Some(idx) = index.get(&(account_id.to_string(), fy)) else {
            continue;
        };
        match enrolment.anchor {
            Some(RelationshipAnchor::NurserySchool) => rows[*idx].anchor_nursery = true,
            Some(RelationshipAnchor::ReligiousSchool) => rows[*idx].anchor_religious = true,
            None => {}
        }
    }
    mark_observed(rows, &observed_fys, |r| &mut r.school_observed);
    Ok(())
}

fn apply_committee(
    store: &Store,
    rows: &mut [HhFy],
    through: i32,
    progress: &mut Reporter<'_>,
) -> Result<()> {
    let cols = store.mirror_columns("Committee_Membership__c")?;
    let hh = resolve_col(&cols, COMMITTEE_FIELDS.household);
    let start = resolve_col(&cols, COMMITTEE_FIELDS.start);
    let end = resolve_col(&cols, COMMITTEE_FIELDS.end);
    let is_active = resolve_col(&cols, COMMITTEE_FIELDS.is_active);
    if hh.is_none() {
        return Ok(());
    }
    let index = row_index(rows);
    let mut observed_fys = std::collections::HashSet::new();
    let mut mark = |account_id: &str, fy: i32| {
        observed_fys.insert(fy);
        if let Some(idx) = index.get(&(account_id.to_string(), fy)) {
            rows[*idx].anchor_committee = true;
        }
    };
    let committee_rows = store.mirror_rows("Committee_Membership__c")?;
    let total = committee_rows.len() as u64;
    for (i, r) in committee_rows.into_iter().enumerate() {
        progress.tick(i as u64 + 1, Some(total));
        let Some(account_id) = field(&r, &hh) else {
            continue;
        };
        let committee =
            normalize_committee(field(&r, &start), field(&r, &end), field(&r, &is_active));
        if let Some(start_fy) = committee.start_fy {
            let end_fy = committee.end_fy.unwrap_or(through).min(through);
            for fy in start_fy..=end_fy {
                mark(account_id, fy);
            }
        } else if committee.current_active {
            mark(account_id, through);
        }
    }
    mark_observed(rows, &observed_fys, |r| &mut r.committee_observed);
    Ok(())
}

/// Rebuild `_m_household` from the Account mirror. One transaction; drop + create + insert.
/// Rebuild the analytical mart with no progress reporting.
pub fn rebuild(store: &mut Store) -> Result<RebuildInfo> {
    let mut sink = noop();
    let mut progress = Reporter::new("rebuild", REBUILD_STEPS, &mut sink);
    rebuild_with(store, &mut progress)
}

/// Number of phases `rebuild_with` reports (the `Reporter` must be built with this).
pub const REBUILD_STEPS: u32 = 5;

/// Rebuild the analytical mart, reporting phase and row-count progress through `progress`
/// so a long post-sync rebuild is visibly working rather than appearing hung. Progress is
/// a pure side effect: it never changes what is built.
pub fn rebuild_with(store: &mut Store, progress: &mut Reporter<'_>) -> Result<RebuildInfo> {
    let present = store.mirror_columns("Account")?;
    if present.is_empty() {
        anyhow::bail!("Account is not synced; sync it before building insights");
    }
    let allowed = store.allowed_fields("Account")?;
    let have = |c: &str| present.iter().any(|p| p == c) && allowed.contains(c);
    for mandatory in ["Type", "IsATempleMember__c"] {
        if !have(mandatory) {
            anyhow::bail!("Account mirror is missing {mandatory}, which insights require");
        }
    }
    let unavailable: Vec<String> = REQUIRED_COLUMNS
        .iter()
        .filter(|c| !have(c))
        .map(|c| c.to_string())
        .collect();

    // SELECT every required column, substituting NULL for absent ones, so `derive`
    // always sees the same positional shape. Identifiers are validated by `ident`.
    let select_list = REQUIRED_COLUMNS
        .iter()
        .map(|c| {
            if have(c) {
                ident(c)
            } else {
                Ok(format!("NULL AS {}", ident(c)?))
            }
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let postal_select = if have("BillingPostalCode") { ident("BillingPostalCode")? } else { "NULL AS BillingPostalCode".into() };
    let sql = format!("SELECT {select_list}, {postal_select} FROM \"Account\" WHERE \"Type\" = 'Member Family'");

    progress.phase("Reading membership records");
    let total_rows: u64 = store.conn().query_row(
        "SELECT COUNT(*) FROM \"Account\" WHERE \"Type\" = 'Member Family'",
        [],
        |r| r.get::<_, i64>(0),
    )? as u64;
    let rows: Vec<Hh> = {
        let mut st = store.conn().prepare(&sql)?;
        let mut read: u64 = 0;
        let it = st.query_map([], |r| {
            let mut raw: [Option<String>; 16] = Default::default();
            for (i, slot) in raw.iter_mut().enumerate() {
                *slot = r.get::<_, Option<String>>(i)?;
            }
            Ok(derive(&raw, normalize_zip(r.get::<_, Option<String>>(16)?.as_deref())))
        })?;
        let mut out = Vec::with_capacity(total_rows as usize);
        progress.tick(0, Some(total_rows));
        for h in it {
            out.push(h?);
            read += 1;
            progress.tick(read, Some(total_rows));
        }
        out
    };
    let mut rows = rows;
    apply_billing_statement_zips(store, &mut rows)?;

    progress.phase("Building yearly membership history");
    let mut household_fy = household_year_rows(&rows, current_fy());
    // Apply optional Relationship Anchor sources before opening the write transaction
    // (they read the mirror tables). Each source is capability-gated and contributes
    // nothing when its objects are not synced.
    progress.phase("Applying engagement sources");
    apply_anchor_sources(store, &mut household_fy, progress)?;
    progress.phase("Writing analysis tables");
    let write_total = (rows.len() + household_fy.len()) as u64;
    let mut written: u64 = 0;
    let tx = store.conn_mut().transaction()?;
    tx.execute_batch(&format!(
        "DROP TABLE IF EXISTS {MART}; DROP TABLE IF EXISTS {MART_FY}"
    ))?;
    tx.execute_batch(&mart_ddl())?;
    tx.execute_batch(&mart_fy_ddl())?;
    {
        let flag_cols = CHANNELS
            .iter()
            .map(|(k, _)| format!("ch_{k}"))
            .collect::<Vec<_>>()
            .join(", ");
        let flag_marks = vec!["?"; 12].join(", ");
        let mut st = tx.prepare(&format!(
            "INSERT INTO {MART}(account_id, name, is_current, is_resigned, join_fy, cohort_fy, resign_fy,
               resigned_unknown_date, bad_join_date, rejoined, tier, category, join_reason, zip, {flag_cols},
               rs_family, ns_family, active_rs_students, last_rs_year, resign_reason_group, exit_reason)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,{flag_marks},?,?,?,?,?,?)"
        ))?;
        for h in &rows {
            let mut vals: Vec<rusqlite::types::Value> = vec![
                h.account_id.clone().into(),
                h.name.clone().into(),
                (h.is_current as i64).into(),
                (h.is_resigned as i64).into(),
                h.join_fy.into(),
                h.cohort_fy.into(),
                h.resign_fy.into(),
                (h.resigned_unknown_date as i64).into(),
                (h.bad_join_date as i64).into(),
                (h.rejoined as i64).into(),
                h.tier.clone().into(),
                h.category.clone().into(),
                h.join_reason.clone().into(),
                h.zip.clone().into(),
            ];
            vals.extend(h.ch.iter().map(|b| rusqlite::types::Value::from(*b as i64)));
            vals.extend([
                (h.rs_family as i64).into(),
                (h.ns_family as i64).into(),
                h.active_rs_students.into(),
                h.last_rs_year.into(),
                h.resign_reason_group.clone().into(),
                h.exit_reason.clone().into(),
            ]);
            st.execute(rusqlite::params_from_iter(vals.iter()))?;
            written += 1;
            progress.tick(written, Some(write_total));
        }
    }
    {
        let flag_cols = CHANNELS
            .iter()
            .map(|(key, _)| format!("entry_job_{key}"))
            .collect::<Vec<_>>()
            .join(", ");
        let flag_marks = vec!["?"; CHANNELS.len()].join(", ");
        let mut st = tx.prepare(&format!(
            "INSERT INTO _m_household_fy(account_id, fy, active_end_of_fy, joined_this_fy, resigned_this_fy,
             tenure_years, exit_reason, entry_job_count, {flag_cols},
             anchor_dues, anchor_nursery, anchor_religious, anchor_committee,
             dues_coverage_missing, dues_settlement,
             renewal_observed, school_observed, committee_observed)
             VALUES(?,?,?,?,?,?,?,?,{flag_marks},?,?,?,?,?,?,?,?,?)"
        ))?;
        for row in &household_fy {
            let mut values: Vec<rusqlite::types::Value> = vec![
                row.account_id.clone().into(),
                row.fy.into(),
                (row.active_end_of_fy as i64).into(),
                (row.joined_this_fy as i64).into(),
                (row.resigned_this_fy as i64).into(),
                row.tenure_years.into(),
                row.exit_reason.clone().into(),
                row.entry_job_count.into(),
            ];
            values.extend(row.entry_jobs.iter().map(|flag| (*flag as i64).into()));
            values.extend([
                (row.anchor_dues as i64).into(),
                (row.anchor_nursery as i64).into(),
                (row.anchor_religious as i64).into(),
                (row.anchor_committee as i64).into(),
                (row.dues_coverage_missing as i64).into(),
                row.dues_settlement.clone().into(),
                (row.renewal_observed as i64).into(),
                (row.school_observed as i64).into(),
                (row.committee_observed as i64).into(),
            ]);
            st.execute(rusqlite::params_from_iter(values.iter()))?;
            written += 1;
            progress.tick(written, Some(write_total));
        }
    }
    progress.phase("Finalizing");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES('insights_built_at', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![now],
    )?;
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES('insights_unavailable', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![serde_json::to_string(&unavailable)?],
    )?;
    tx.execute(
        "INSERT INTO _meta(key, value) VALUES('insights_schema_version', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![MART_SCHEMA_VERSION.to_string()],
    )?;
    tx.commit()?;
    progress.finish();
    Ok(RebuildInfo {
        households: rows.len(),
        unavailable,
    })
}

/// Read the mart back into memory (the views are computed from this).
pub fn load(store: &Store) -> Result<Vec<Hh>> {
    let flag_cols = CHANNELS
        .iter()
        .map(|(k, _)| format!("ch_{k}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut st = store.conn().prepare(&format!(
        "SELECT account_id, name, is_current, is_resigned, join_fy, cohort_fy, resign_fy,
                resigned_unknown_date, bad_join_date, rejoined, tier, category, join_reason, zip, {flag_cols},
                rs_family, ns_family, active_rs_students, last_rs_year, resign_reason_group, exit_reason
         FROM {MART}"
    ))?;
    let rows = st.query_map([], |r| {
        let mut ch = [false; 12];
        for (i, f) in ch.iter_mut().enumerate() {
            *f = r.get::<_, i64>(14 + i)? != 0;
        }
        Ok(Hh {
            account_id: r.get(0)?,
            name: r.get(1)?,
            is_current: r.get::<_, i64>(2)? != 0,
            is_resigned: r.get::<_, i64>(3)? != 0,
            join_fy: r.get(4)?,
            cohort_fy: r.get(5)?,
            resign_fy: r.get(6)?,
            resigned_unknown_date: r.get::<_, i64>(7)? != 0,
            bad_join_date: r.get::<_, i64>(8)? != 0,
            rejoined: r.get::<_, i64>(9)? != 0,
            tier: r.get(10)?,
            category: r.get(11)?,
            join_reason: r.get(12)?,
            zip: r.get(13)?,
            ch,
            rs_family: r.get::<_, i64>(26)? != 0,
            ns_family: r.get::<_, i64>(27)? != 0,
            active_rs_students: r.get(28)?,
            last_rs_year: r.get(29)?,
            resign_reason_group: r.get(30)?,
            exit_reason: r.get(31)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

// ── views ───────────────────────────────────────────────────────────────────

const FIRST_TREND_FY: i32 = 2005;
const FIRST_COHORT_FY: i32 = 2010;
const MAX_K: i32 = 8;
const CHANNEL_MIN_N: usize = 20;

#[derive(Serialize, Debug, Clone)]
pub struct TrendRow {
    pub fy: i32,
    pub joins: i64,
    pub resigns: i64,
    pub active_end_of_fy: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct CohortYear1 {
    pub cohort: i32,
    pub n: i64,
    pub pct_retained: f64,
}
#[derive(Serialize, Debug, Clone)]
pub struct CohortCell {
    pub cohort: i32,
    pub n: i64,
    pub k: i32,
    pub pct_retained: f64,
}
#[derive(Serialize, Debug, Clone)]
pub struct ChannelRow {
    pub key: String,
    pub label: String,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
    pub avg_tenure: f64,
    pub left_within_2y: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct SchoolRow {
    pub group: String,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
}
#[derive(Serialize, Debug, Clone)]
pub struct ReasonCell {
    pub fy: i32,
    pub reason: String,
    pub n: i64,
}
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ZipAttritionCell {
    pub fy: i32,
    pub zip: String,
    pub start_households: i64,
    pub exits: i64,
    pub attrition_rate: f64,
}
/// Retention for households grouped by how many Entry Jobs they stated.
#[derive(Serialize, Debug, Clone)]
pub struct MultiJobRow {
    pub bucket: String,
    pub jobs: i64,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
    pub avg_tenure: f64,
}
/// Primary Exit Outcome counts within a tenure-at-exit band.
#[derive(Serialize, Debug, Clone)]
pub struct OutcomeByTenureRow {
    pub tenure_bucket: String,
    pub outcome: String,
    pub n: i64,
}
/// Retention by completed fiscal years since Religious School ended.
#[derive(Serialize, Debug, Clone)]
pub struct SchoolGapRow {
    pub bucket: String,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
}
/// Dues state for one fiscal year. Settlement counts are eventual states, not as-of
/// history. Coverage-missing means active-with-no-dues-line, never proven non-renewal.
#[derive(Serialize, Debug, Clone)]
pub struct DuesRow {
    pub fy: i32,
    pub active: i64,
    pub billed: i64,
    pub coverage_missing: i64,
    pub settled: i64,
    pub partially_settled: i64,
    pub unsettled: i64,
}
/// Retention of households that recently held one kind of Relationship Anchor.
#[derive(Serialize, Debug, Clone)]
pub struct AnchorTypeRow {
    pub key: String,
    pub label: String,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
}
/// Retention by how many distinct Relationship Anchors a household recently held.
#[derive(Serialize, Debug, Clone)]
pub struct AnchorCountRow {
    pub anchors: i64,
    pub label: String,
    pub n: i64,
    pub still_members: i64,
    pub pct: f64,
}
#[derive(Serialize, Debug, Clone)]
pub struct Kpis {
    pub members_now: i64,
    pub net_vs_prior_fy: i64,
    pub joins_this_fy: i64,
    pub resigns_this_fy: i64,
    pub year1_cohort: i32,
    pub year1_pct: f64,
    pub year1_baseline_pct: f64,
    pub at_risk_count: i64,
}
#[derive(Serialize, Debug, Clone)]
pub struct AtRiskRow {
    pub account_id: String,
    pub name: String,
    pub tier: Option<String>,
    pub join_fy: Option<i32>,
    pub rules: Vec<String>,
}
#[derive(Serialize, Debug, Clone)]
pub struct Insights {
    pub built_at: Option<String>,
    pub newest_source_sync_at: Option<String>,
    pub stale: bool,
    pub capabilities: Vec<SourceCapability>,
    pub current_fy: i32,
    pub unavailable: Vec<String>,
    pub kpis: Kpis,
    pub trend: Vec<TrendRow>,
    pub year1: Vec<CohortYear1>,
    pub cohort_matrix: Vec<CohortCell>,
    pub channels: Vec<ChannelRow>,
    pub school: Vec<SchoolRow>,
    pub reasons: Vec<ReasonCell>,
    pub multi_job: Vec<MultiJobRow>,
    pub outcome_by_tenure: Vec<OutcomeByTenureRow>,
    pub school_progression: Vec<SchoolRow>,
    pub school_gap: Vec<SchoolGapRow>,
    pub dues: Vec<DuesRow>,
    pub anchor_type: Vec<AnchorTypeRow>,
    pub anchor_count: Vec<AnchorCountRow>,
    pub zip_attrition: Vec<ZipAttritionCell>,
}

/// Spell rule: a member in `fy` if joined by then and not resigned before/in it.
/// A resigned household with no resign date counts only in its join year.
pub fn member_in(h: &Hh, fy: i32) -> bool {
    let Some(j) = h.join_fy else { return false };
    if j > fy {
        return false;
    }
    if h.resigned_unknown_date {
        return fy == j;
    }
    match h.resign_fy {
        Some(r) => r > fy,
        None => true,
    }
}

fn pct(num: i64, den: i64) -> f64 {
    if den == 0 {
        0.0
    } else {
        (1000.0 * num as f64 / den as f64).round() / 10.0
    }
}

pub fn zip_attrition(households: &[Hh], cur: i32) -> Vec<ZipAttritionCell> {
    let mut counts: std::collections::BTreeMap<(i32, String), (i64, i64)> = Default::default();
    for fy in cur - 5..cur {
        for household in households {
            let Some(zip) = household.zip.as_ref() else { continue };
            let cell = counts.entry((fy, zip.clone())).or_default();
            if member_in(household, fy - 1) { cell.0 += 1; }
            if household.resign_fy == Some(fy) { cell.1 += 1; }
        }
    }
    counts.into_iter().filter_map(|((fy, zip), (start_households, exits))| {
        (start_households >= 5).then(|| ZipAttritionCell { fy, zip, start_households, exits, attrition_rate: pct(exits, start_households) })
    }).collect()
}

pub fn trend(hh: &[Hh], cur: i32) -> Vec<TrendRow> {
    (FIRST_TREND_FY..=cur)
        .map(|fy| TrendRow {
            fy,
            joins: hh.iter().filter(|h| h.join_fy == Some(fy)).count() as i64,
            resigns: hh.iter().filter(|h| h.resign_fy == Some(fy)).count() as i64,
            active_end_of_fy: hh.iter().filter(|h| member_in(h, fy)).count() as i64,
        })
        .collect()
}

pub fn trend_from_household_years(rows: &[HhFy], cur: i32) -> Vec<TrendRow> {
    (FIRST_TREND_FY..=cur)
        .map(|fy| TrendRow {
            fy,
            joins: rows
                .iter()
                .filter(|row| row.fy == fy && row.joined_this_fy)
                .count() as i64,
            resigns: rows
                .iter()
                .filter(|row| row.fy == fy && row.resigned_this_fy)
                .count() as i64,
            active_end_of_fy: rows
                .iter()
                .filter(|row| row.fy == fy && row.active_end_of_fy)
                .count() as i64,
        })
        .collect()
}

pub fn year1(hh: &[Hh], cur: i32) -> Vec<CohortYear1> {
    (FIRST_COHORT_FY..cur)
        .filter_map(|c| {
            let cohort: Vec<&Hh> = hh.iter().filter(|h| h.join_fy == Some(c)).collect();
            if cohort.is_empty() {
                return None;
            }
            let kept = cohort.iter().filter(|h| member_in(h, c + 1)).count() as i64;
            Some(CohortYear1 {
                cohort: c,
                n: cohort.len() as i64,
                pct_retained: pct(kept, cohort.len() as i64),
            })
        })
        .collect()
}

/// Household-year rows grouped by account, built once per `views` call so the cohort and
/// channel views find a household's years by hash lookup instead of scanning every row
/// per member. Rows keep their input order within an account, so "any year matching" and
/// "latest year" resolve exactly as the full scans did.
pub struct HouseholdYearIndex<'a> {
    by_account: std::collections::HashMap<&'a str, Vec<&'a HhFy>>,
}

impl<'a> HouseholdYearIndex<'a> {
    pub fn build(rows: &'a [HhFy]) -> Self {
        let mut by_account: std::collections::HashMap<&str, Vec<&HhFy>> =
            std::collections::HashMap::new();
        for row in rows {
            by_account
                .entry(row.account_id.as_str())
                .or_default()
                .push(row);
        }
        Self { by_account }
    }

    /// Every row for `account_id`, in input order.
    fn account_years(&self, account_id: &str) -> &[&'a HhFy] {
        self.by_account
            .get(account_id)
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
    }

    /// Whether `account_id` was active at the end of fiscal year `fy`.
    fn active_in(&self, account_id: &str, fy: i32) -> bool {
        self.account_years(account_id)
            .iter()
            .any(|row| row.fy == fy && row.active_end_of_fy)
    }
}

pub fn year1_from_household_years(rows: &[HhFy], cur: i32) -> Vec<CohortYear1> {
    year1_indexed(rows, &HouseholdYearIndex::build(rows), cur)
}

pub fn year1_indexed(rows: &[HhFy], index: &HouseholdYearIndex<'_>, cur: i32) -> Vec<CohortYear1> {
    (FIRST_COHORT_FY..cur)
        .filter_map(|cohort| {
            let members: Vec<_> = rows
                .iter()
                .filter(|row| row.fy == cohort && row.joined_this_fy)
                .collect();
            if members.is_empty() {
                return None;
            }
            let kept = members
                .iter()
                .filter(|member| index.active_in(&member.account_id, cohort + 1))
                .count() as i64;
            Some(CohortYear1 {
                cohort,
                n: members.len() as i64,
                pct_retained: pct(kept, members.len() as i64),
            })
        })
        .collect()
}

pub fn cohort_matrix(hh: &[Hh], cur: i32) -> Vec<CohortCell> {
    let mut out = Vec::new();
    for c in FIRST_COHORT_FY..cur {
        let cohort: Vec<&Hh> = hh.iter().filter(|h| h.join_fy == Some(c)).collect();
        if cohort.is_empty() {
            continue;
        }
        for k in 1..=MAX_K {
            if c + k > cur {
                break;
            }
            let kept = cohort.iter().filter(|h| member_in(h, c + k)).count() as i64;
            out.push(CohortCell {
                cohort: c,
                n: cohort.len() as i64,
                k,
                pct_retained: pct(kept, cohort.len() as i64),
            });
        }
    }
    out
}

pub fn cohort_matrix_from_household_years(rows: &[HhFy], cur: i32) -> Vec<CohortCell> {
    cohort_matrix_indexed(rows, &HouseholdYearIndex::build(rows), cur)
}

pub fn cohort_matrix_indexed(
    rows: &[HhFy],
    index: &HouseholdYearIndex<'_>,
    cur: i32,
) -> Vec<CohortCell> {
    let mut out = Vec::new();
    for cohort in FIRST_COHORT_FY..cur {
        let members: Vec<_> = rows
            .iter()
            .filter(|row| row.fy == cohort && row.joined_this_fy)
            .collect();
        if members.is_empty() {
            continue;
        }
        for k in 1..=MAX_K {
            if cohort + k > cur {
                break;
            }
            let kept = members
                .iter()
                .filter(|member| index.active_in(&member.account_id, cohort + k))
                .count() as i64;
            out.push(CohortCell {
                cohort,
                n: members.len() as i64,
                k,
                pct_retained: pct(kept, members.len() as i64),
            });
        }
    }
    out
}

/// Joiners old enough to judge: at least four full membership years, at most twelve.
fn judgeable(h: &Hh, cur: i32) -> bool {
    matches!(h.join_fy, Some(j) if j >= cur - 12 && j <= cur - 4)
}

fn tenure_years(h: &Hh, cur: i32) -> f64 {
    let j = h.join_fy.unwrap_or(cur) as f64;
    if h.is_current {
        cur as f64 - j
    } else if h.resigned_unknown_date {
        1.0
    } else {
        h.resign_fy.map(|r| r as f64 - j).unwrap_or(1.0)
    }
}

pub fn channels(hh: &[Hh], cur: i32) -> Vec<ChannelRow> {
    let base: Vec<&Hh> = hh
        .iter()
        .filter(|h| judgeable(h, cur) && h.join_reason.is_some())
        .collect();
    let mut out: Vec<ChannelRow> = CHANNELS
        .iter()
        .enumerate()
        .filter_map(|(i, (key, _))| {
            let members: Vec<&&Hh> = base.iter().filter(|h| h.ch[i]).collect();
            if members.len() < CHANNEL_MIN_N {
                return None;
            }
            let n = members.len() as i64;
            let still = members.iter().filter(|h| h.is_current).count() as i64;
            let tenure: f64 = members.iter().map(|h| tenure_years(h, cur)).sum::<f64>() / n as f64;
            let left2 = members
                .iter()
                .filter(|h| !h.is_current && tenure_years(h, cur) <= 2.0)
                .count() as i64;
            Some(ChannelRow {
                key: key.to_string(),
                label: channel_label(key),
                n,
                still_members: still,
                pct: pct(still, n),
                avg_tenure: (tenure * 10.0).round() / 10.0,
                left_within_2y: left2,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.pct
            .partial_cmp(&a.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn channel_label(key: &str) -> String {
    match key {
        "religious_school" => "Religious school",
        "nursery_school" => "Nursery school",
        "affiliation" => "Affiliation",
        "life_cycle" => "Life cycle event",
        "family" => "To be with family",
        "young_professionals" => "Young professionals",
        "community" => "Community",
        "hhd_tickets" => "High Holy Day tickets",
        "streicker" => "Streicker",
        "clergy" => "Clergy",
        "worship" => "Worship services",
        "move" => "Move or relocation",
        other => other,
    }
    .to_string()
}

pub fn school(hh: &[Hh], cur: i32) -> Vec<SchoolRow> {
    const GROUPS: [&str; 4] = [
        "Both nursery and religious school",
        "Religious school family",
        "Nursery school family",
        "No school history",
    ];
    let group_of = |h: &Hh| match (h.rs_family, h.ns_family) {
        (true, true) => GROUPS[0],
        (true, false) => GROUPS[1],
        (false, true) => GROUPS[2],
        (false, false) => GROUPS[3],
    };
    let base: Vec<&Hh> = hh.iter().filter(|h| judgeable(h, cur)).collect();
    GROUPS
        .iter()
        .map(|g| {
            let m: Vec<&&Hh> = base.iter().filter(|h| group_of(h) == *g).collect();
            let n = m.len() as i64;
            let still = m.iter().filter(|h| h.is_current).count() as i64;
            SchoolRow {
                group: g.to_string(),
                n,
                still_members: still,
                pct: pct(still, n),
            }
        })
        .collect()
}

pub fn reasons(hh: &[Hh], cur: i32) -> Vec<ReasonCell> {
    let mut counts: std::collections::BTreeMap<(i32, String), i64> = Default::default();
    for h in hh.iter().filter(|h| !h.is_current && h.is_resigned) {
        if let Some(fy) = h.resign_fy {
            if fy >= cur - 5 && fy <= cur {
                *counts
                    .entry((fy, h.resign_reason_group.clone()))
                    .or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .map(|((fy, reason), n)| ReasonCell { fy, reason, n })
        .collect()
}

// ── Jobs views ──────────────────────────────────────────────────────────────

/// Fixed tenure bands, indexed so ordering stays stable across rebuilds.
const TENURE_BUCKETS: [&str; 4] = ["1-2y", "3-5y", "6-10y", "11+y"];

fn tenure_bucket_index(years: f64) -> usize {
    if years <= 2.0 {
        0
    } else if years <= 5.0 {
        1
    } else if years <= 10.0 {
        2
    } else {
        3
    }
}

/// Count the recognized Entry Jobs a household stated.
fn entry_job_count(h: &Hh) -> usize {
    h.ch.iter().filter(|flag| **flag).count()
}

/// Retention by number of stated Entry Jobs. A household with more than one
/// recognized joining reason is an association, not proof of stronger intent.
pub fn multi_job(hh: &[Hh], cur: i32) -> Vec<MultiJobRow> {
    let base: Vec<&Hh> = hh
        .iter()
        .filter(|h| judgeable(h, cur) && h.join_reason.is_some())
        .collect();
    [(1, "1 job"), (2, "2 jobs"), (3, "3+ jobs")]
        .into_iter()
        .filter_map(|(jobs, label)| {
            let members: Vec<&&Hh> = base
                .iter()
                .filter(|h| {
                    let count = entry_job_count(h);
                    if jobs == 3 {
                        count >= 3
                    } else {
                        count == jobs as usize
                    }
                })
                .collect();
            if members.is_empty() {
                return None;
            }
            let n = members.len() as i64;
            let still = members.iter().filter(|h| h.is_current).count() as i64;
            let tenure = members.iter().map(|h| tenure_years(h, cur)).sum::<f64>() / n as f64;
            Some(MultiJobRow {
                bucket: label.to_string(),
                jobs,
                n,
                still_members: still,
                pct: pct(still, n),
                avg_tenure: (tenure * 10.0).round() / 10.0,
            })
        })
        .collect()
}

/// Primary Exit Outcome composition by tenure at exit, so churn among short-tenure
/// households can be compared against longer relationships.
pub fn outcome_by_tenure(hh: &[Hh], cur: i32) -> Vec<OutcomeByTenureRow> {
    let mut counts: std::collections::BTreeMap<(usize, String), i64> = Default::default();
    for h in hh.iter().filter(|h| !h.is_current && h.is_resigned) {
        let bucket = tenure_bucket_index(tenure_years(h, cur));
        *counts
            .entry((bucket, h.exit_reason.clone()))
            .or_default() += 1;
    }
    counts
        .into_iter()
        .map(|((bucket, outcome), n)| OutcomeByTenureRow {
            tenure_bucket: TENURE_BUCKETS[bucket].to_string(),
            outcome,
            n,
        })
        .collect()
}

/// Nursery-to-Religious-School progression: of Nursery School families, how many
/// also became Religious School families, and how each group retained.
pub fn school_progression(hh: &[Hh], cur: i32) -> Vec<SchoolRow> {
    let base: Vec<&Hh> = hh
        .iter()
        .filter(|h| judgeable(h, cur) && h.ns_family)
        .collect();
    let group = |label: &str, members: Vec<&&Hh>| {
        let n = members.len() as i64;
        let still = members.iter().filter(|h| h.is_current).count() as i64;
        SchoolRow {
            group: label.to_string(),
            n,
            still_members: still,
            pct: pct(still, n),
        }
    };
    vec![
        group(
            "Nursery → Religious school",
            base.iter().filter(|h| h.rs_family).collect(),
        ),
        group(
            "Nursery school only",
            base.iter().filter(|h| !h.rs_family).collect(),
        ),
    ]
}

/// Retention by completed fiscal years since a Religious School family's last active
/// year. Only households whose Religious School has ended (no active students) qualify.
pub fn school_gap(hh: &[Hh], cur: i32) -> Vec<SchoolGapRow> {
    const BUCKETS: [(&str, i32, i32); 4] = [
        ("0-1y", 0, 1),
        ("2-3y", 2, 3),
        ("4-6y", 4, 6),
        ("7+y", 7, i32::MAX),
    ];
    let base: Vec<&Hh> = hh
        .iter()
        .filter(|h| h.rs_family && h.active_rs_students == 0 && h.last_rs_year.is_some())
        .collect();
    BUCKETS
        .into_iter()
        .filter_map(|(label, lo, hi)| {
            let members: Vec<&&Hh> = base
                .iter()
                .filter(|h| {
                    let gap = cur - h.last_rs_year.unwrap();
                    gap >= lo && gap <= hi
                })
                .collect();
            if members.is_empty() {
                return None;
            }
            let n = members.len() as i64;
            let still = members.iter().filter(|h| h.is_current).count() as i64;
            Some(SchoolGapRow {
                bucket: label.to_string(),
                n,
                still_members: still,
                pct: pct(still, n),
            })
        })
        .collect()
}

// ── Renewal & Engagement views ──────────────────────────────────────────────

/// The recent-anchor window: households anchored in these completed fiscal years,
/// measured against whether they are still active in the current fiscal year.
const ANCHOR_WINDOW: i32 = 8;

/// Dues state by fiscal year, over active households. Settlement labels are eventual,
/// so counts describe final settlement, not the household's state during the year.
pub fn dues(rows: &[HhFy], cur: i32) -> Vec<DuesRow> {
    (cur - 5..=cur)
        .filter_map(|fy| {
            let year: Vec<_> = rows
                .iter()
                .filter(|r| r.fy == fy && r.active_end_of_fy)
                .collect();
            if year.is_empty() {
                return None;
            }
            let label_count = |label: &str| {
                year.iter()
                    .filter(|r| r.dues_settlement.as_deref() == Some(label))
                    .count() as i64
            };
            Some(DuesRow {
                fy,
                active: year.len() as i64,
                billed: year.iter().filter(|r| r.anchor_dues).count() as i64,
                coverage_missing: year.iter().filter(|r| r.dues_coverage_missing).count() as i64,
                settled: label_count("Eventual settlement: settled"),
                partially_settled: label_count("Eventual settlement: partially settled"),
                unsettled: label_count("Eventual settlement: unsettled"),
            })
        })
        .collect()
}

/// Account ids active at the end of the current fiscal year.
fn active_now(rows: &[HhFy], cur: i32) -> std::collections::HashSet<&str> {
    rows.iter()
        .filter(|r| r.fy == cur && r.active_end_of_fy)
        .map(|r| r.account_id.as_str())
        .collect()
}

/// Retention by anchor type: of households that held an anchor in the recent window,
/// how many are still active now. Only anchor types with an available source appear.
pub fn anchor_type(rows: &[HhFy], cur: i32, caps: &[SourceCapability]) -> Vec<AnchorTypeRow> {
    let active = active_now(rows, cur);
    let defs: [(&str, &str, &str, fn(&HhFy) -> bool); 4] = [
        ("dues", "Dues renewal", "renewal", |r| r.anchor_dues),
        ("nursery", "Nursery school", "school", |r| r.anchor_nursery),
        ("religious", "Religious school", "school", |r| {
            r.anchor_religious
        }),
        ("committee", "Committee", "committee", |r| {
            r.anchor_committee
        }),
    ];
    defs.into_iter()
        .filter(|(_, _, cap, _)| cap_available(caps, cap))
        .filter_map(|(key, label, _, held)| {
            let holders: std::collections::HashSet<&str> = rows
                .iter()
                .filter(|r| r.fy >= cur - ANCHOR_WINDOW && r.fy <= cur - 1 && held(r))
                .map(|r| r.account_id.as_str())
                .collect();
            if holders.is_empty() {
                return None;
            }
            let n = holders.len() as i64;
            let still = holders.iter().filter(|id| active.contains(*id)).count() as i64;
            Some(AnchorTypeRow {
                key: key.to_string(),
                label: label.to_string(),
                n,
                still_members: still,
                pct: pct(still, n),
            })
        })
        .collect()
}

/// Retention by anchor count: bucket households by the most anchors they held in any
/// recent fiscal year, then measure how many are still active now.
pub fn anchor_count(rows: &[HhFy], cur: i32) -> Vec<AnchorCountRow> {
    let active = active_now(rows, cur);
    let mut depth: std::collections::HashMap<&str, i64> = Default::default();
    for r in rows
        .iter()
        .filter(|r| r.fy >= cur - ANCHOR_WINDOW && r.fy <= cur - 1)
    {
        let entry = depth.entry(r.account_id.as_str()).or_insert(0);
        *entry = (*entry).max(r.anchor_count());
    }
    [
        (0, "0 anchors"),
        (1, "1 anchor"),
        (2, "2 anchors"),
        (3, "3+ anchors"),
    ]
    .into_iter()
    .filter_map(|(anchors, label)| {
        let members: Vec<&&str> = depth
            .iter()
            .filter(|(_, held)| {
                if anchors == 3 {
                    **held >= 3
                } else {
                    **held == anchors
                }
            })
            .map(|(id, _)| id)
            .collect();
        if members.is_empty() {
            return None;
        }
        let n = members.len() as i64;
        let still = members.iter().filter(|id| active.contains(**id)).count() as i64;
        Some(AnchorCountRow {
            anchors,
            label: label.to_string(),
            n,
            still_members: still,
            pct: pct(still, n),
        })
    })
    .collect()
}

pub fn kpis(hh: &[Hh], cur: i32, at_risk_count: i64) -> Kpis {
    let active = |fy: i32| hh.iter().filter(|h| member_in(h, fy)).count() as i64;
    let y1 = year1(hh, cur);
    // The current-FY cohort (cur-1) is still mid-first-year; only present a cohort
    // whose first year is fully complete as the headline "first-year retention".
    let latest = y1.iter().filter(|r| r.cohort <= cur - 2).last();
    let baseline: Vec<&CohortYear1> = y1.iter().filter(|r| r.cohort <= cur - 3).collect();
    let baseline_pct = if baseline.is_empty() {
        0.0
    } else {
        (10.0 * baseline.iter().map(|r| r.pct_retained).sum::<f64>() / baseline.len() as f64)
            .round()
            / 10.0
    };
    Kpis {
        members_now: hh.iter().filter(|h| h.is_current).count() as i64,
        net_vs_prior_fy: active(cur) - active(cur - 1),
        joins_this_fy: hh.iter().filter(|h| h.join_fy == Some(cur)).count() as i64,
        resigns_this_fy: hh.iter().filter(|h| h.resign_fy == Some(cur)).count() as i64,
        year1_cohort: latest.map(|r| r.cohort).unwrap_or(cur - 1),
        year1_pct: latest.map(|r| r.pct_retained).unwrap_or(0.0),
        year1_baseline_pct: baseline_pct,
        at_risk_count,
    }
}

// ── at-risk rules (fixed in code; tuning is a code change on purpose) ───────

/// `year1` is the already-computed `year1_indexed` view for the same rows and `cur`.
pub fn kpis_from_household_years(
    rows: &[HhFy],
    year1: &[CohortYear1],
    cur: i32,
    at_risk_count: i64,
) -> Kpis {
    let latest = year1.iter().filter(|row| row.cohort <= cur - 2).last();
    let baseline: Vec<_> = year1.iter().filter(|row| row.cohort <= cur - 3).collect();
    let year1_baseline_pct = if baseline.is_empty() {
        0.0
    } else {
        (10.0 * baseline.iter().map(|row| row.pct_retained).sum::<f64>() / baseline.len() as f64)
            .round()
            / 10.0
    };
    let count = |fy: i32, predicate: fn(&HhFy) -> bool| {
        rows.iter()
            .filter(|row| row.fy == fy && predicate(row))
            .count() as i64
    };
    Kpis {
        members_now: count(cur, |row| row.active_end_of_fy),
        net_vs_prior_fy: count(cur, |row| row.active_end_of_fy)
            - count(cur - 1, |row| row.active_end_of_fy),
        joins_this_fy: count(cur, |row| row.joined_this_fy),
        resigns_this_fy: count(cur, |row| row.resigned_this_fy),
        year1_cohort: latest.map(|row| row.cohort).unwrap_or(cur - 1),
        year1_pct: latest.map(|row| row.pct_retained).unwrap_or(0.0),
        year1_baseline_pct,
        at_risk_count,
    }
}

pub fn channels_from_household_years(rows: &[HhFy], cur: i32) -> Vec<ChannelRow> {
    channels_indexed(rows, &HouseholdYearIndex::build(rows), cur)
}

pub fn channels_indexed(
    rows: &[HhFy],
    index: &HouseholdYearIndex<'_>,
    cur: i32,
) -> Vec<ChannelRow> {
    let joiners: Vec<_> = rows
        .iter()
        .filter(|row| row.joined_this_fy && row.fy >= cur - 12 && row.fy <= cur - 4)
        .collect();
    let mut out: Vec<_> = CHANNELS
        .iter()
        .enumerate()
        .filter_map(|(channel, (key, _))| {
            let members: Vec<_> = joiners.iter().filter(|row| row.entry_jobs[channel]).collect();
            if members.len() < CHANNEL_MIN_N {
                return None;
            }
            // The household's latest year; on a tied `fy`, `max_by_key` keeps the last
            // row in input order, exactly as the full scan did.
            let outcomes: Vec<_> = members
                .iter()
                .map(|member| {
                    index
                        .account_years(&member.account_id)
                        .iter()
                        .max_by_key(|row| row.fy)
                        .copied()
                })
                .collect();
            let n = members.len() as i64;
            let still_members = outcomes
                .iter()
                .filter(|row| row.is_some_and(|row| row.active_end_of_fy))
                .count() as i64;
            let avg_tenure = outcomes
                .iter()
                .filter_map(|row| row.and_then(|row| row.tenure_years))
                .map(f64::from)
                .sum::<f64>()
                / n as f64;
            let left_within_2y = outcomes
                .iter()
                .filter(|row| {
                    row.is_some_and(|row| {
                        row.resigned_this_fy && row.tenure_years.unwrap_or(i32::MAX) <= 2
                    })
                })
                .count() as i64;
            Some(ChannelRow {
                key: (*key).to_string(),
                label: channel_label(key),
                n,
                still_members,
                pct: pct(still_members, n),
                avg_tenure: (avg_tenure * 10.0).round() / 10.0,
                left_within_2y,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.pct
            .partial_cmp(&a.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

pub const INTRO_TIERS: [&str; 3] = ["Young Adult Member", "Young Professionals", "Downtown"];

pub fn at_risk_rows(hh: &[Hh], cur: i32) -> Vec<AtRiskRow> {
    let idx = |k: &str| {
        CHANNELS
            .iter()
            .position(|(key, _)| *key == k)
            .expect("channel")
    };
    let (ns, rs) = (idx("nursery_school"), idx("religious_school"));
    let mut out: Vec<AtRiskRow> = hh
        .iter()
        .filter(|h| h.is_current)
        .filter_map(|h| {
            let mut rules = Vec::new();
            if h.join_fy == Some(cur - 1) {
                rules.push("first_year");
            }
            if matches!(h.join_fy, Some(j) if j >= cur - 2) && h.ch[ns] && !h.ch[rs] && !h.rs_family
            {
                rules.push("new_ns_only");
            }
            if h.tier
                .as_deref()
                .map_or(false, |t| INTRO_TIERS.contains(&t))
                && matches!(h.join_fy, Some(j) if cur - j >= 2)
            {
                rules.push("intro_tier_aging");
            }
            if h.rs_family
                && h.active_rs_students == 0
                && matches!(h.last_rs_year, Some(y) if y >= cur - 2 && y <= cur - 1)
            {
                rules.push("rs_ended");
            }
            if rules.len() < 2 {
                return None;
            }
            Some(AtRiskRow {
                account_id: h.account_id.clone(),
                name: h.name.clone().unwrap_or_default(),
                tier: h.tier.clone(),
                join_fy: h.join_fy,
                rules: rules.into_iter().map(String::from).collect(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.rules.len().cmp(&a.rules.len()).then(a.name.cmp(&b.name)));
    out
}

pub fn at_risk(store: &Store, cur: i32) -> Result<Vec<AtRiskRow>> {
    Ok(at_risk_rows(&load(store)?, cur))
}

// ── CSV ─────────────────────────────────────────────────────────────────────

pub const VIEWS: [&str; 14] = [
    "trend",
    "year1",
    "cohort_matrix",
    "channels",
    "school",
    "reasons",
    "at_risk",
    "multi_job",
    "outcome_by_tenure",
    "school_progression",
    "school_gap",
    "dues",
    "anchor_type",
    "anchor_count",
];

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv(header: &[&str], rows: Vec<Vec<String>>) -> (String, usize) {
    let mut out = header.join(",");
    out.push('\n');
    let n = rows.len();
    for r in rows {
        out.push_str(&r.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    (out, n)
}

/// Render one view as CSV text. Returns (text, number of data rows).
pub fn to_csv(view: &str, ins: &Insights, at_risk: &[AtRiskRow]) -> Result<(String, usize)> {
    let s = |v: &dyn std::fmt::Display| v.to_string();
    Ok(match view {
        "trend" => csv(
            &["fy", "joins", "resigns", "active_end_of_fy"],
            ins.trend
                .iter()
                .map(|r| vec![s(&r.fy), s(&r.joins), s(&r.resigns), s(&r.active_end_of_fy)])
                .collect(),
        ),
        "year1" => csv(
            &["cohort", "n", "pct_retained_1y"],
            ins.year1
                .iter()
                .map(|r| vec![s(&r.cohort), s(&r.n), s(&r.pct_retained)])
                .collect(),
        ),
        "cohort_matrix" => csv(
            &["cohort", "n", "years_after", "pct_retained"],
            ins.cohort_matrix
                .iter()
                .map(|r| vec![s(&r.cohort), s(&r.n), s(&r.k), s(&r.pct_retained)])
                .collect(),
        ),
        "channels" => csv(
            &[
                "channel",
                "households",
                "still_members",
                "pct",
                "avg_tenure_years",
                "left_within_2y",
            ],
            ins.channels
                .iter()
                .map(|r| {
                    vec![
                        r.label.clone(),
                        s(&r.n),
                        s(&r.still_members),
                        s(&r.pct),
                        s(&r.avg_tenure),
                        s(&r.left_within_2y),
                    ]
                })
                .collect(),
        ),
        "school" => csv(
            &["group", "households", "still_members", "pct"],
            ins.school
                .iter()
                .map(|r| vec![r.group.clone(), s(&r.n), s(&r.still_members), s(&r.pct)])
                .collect(),
        ),
        "reasons" => csv(
            &["fy", "reason", "n"],
            ins.reasons
                .iter()
                .map(|r| vec![s(&r.fy), r.reason.clone(), s(&r.n)])
                .collect(),
        ),
        "multi_job" => csv(
            &[
                "bucket",
                "jobs",
                "households",
                "still_members",
                "pct",
                "avg_tenure_years",
            ],
            ins.multi_job
                .iter()
                .map(|r| {
                    vec![
                        r.bucket.clone(),
                        s(&r.jobs),
                        s(&r.n),
                        s(&r.still_members),
                        s(&r.pct),
                        s(&r.avg_tenure),
                    ]
                })
                .collect(),
        ),
        "outcome_by_tenure" => csv(
            &["tenure_bucket", "outcome", "n"],
            ins.outcome_by_tenure
                .iter()
                .map(|r| vec![r.tenure_bucket.clone(), r.outcome.clone(), s(&r.n)])
                .collect(),
        ),
        "school_progression" => csv(
            &["group", "households", "still_members", "pct"],
            ins.school_progression
                .iter()
                .map(|r| vec![r.group.clone(), s(&r.n), s(&r.still_members), s(&r.pct)])
                .collect(),
        ),
        "school_gap" => csv(
            &["bucket", "households", "still_members", "pct"],
            ins.school_gap
                .iter()
                .map(|r| vec![r.bucket.clone(), s(&r.n), s(&r.still_members), s(&r.pct)])
                .collect(),
        ),
        "dues" => csv(
            &[
                "fy",
                "active",
                "billed",
                "coverage_missing",
                "settled",
                "partially_settled",
                "unsettled",
            ],
            ins.dues
                .iter()
                .map(|r| {
                    vec![
                        s(&r.fy),
                        s(&r.active),
                        s(&r.billed),
                        s(&r.coverage_missing),
                        s(&r.settled),
                        s(&r.partially_settled),
                        s(&r.unsettled),
                    ]
                })
                .collect(),
        ),
        "anchor_type" => csv(
            &["anchor", "households", "still_members", "pct"],
            ins.anchor_type
                .iter()
                .map(|r| vec![r.label.clone(), s(&r.n), s(&r.still_members), s(&r.pct)])
                .collect(),
        ),
        "anchor_count" => csv(
            &["anchors", "households", "still_members", "pct"],
            ins.anchor_count
                .iter()
                .map(|r| vec![r.label.clone(), s(&r.n), s(&r.still_members), s(&r.pct)])
                .collect(),
        ),
        "at_risk" => csv(
            &["account_id", "name", "tier", "join_fy", "rules"],
            at_risk
                .iter()
                .map(|r| {
                    vec![
                        r.account_id.clone(),
                        r.name.clone(),
                        r.tier.clone().unwrap_or_default(),
                        r.join_fy.map(|v| v.to_string()).unwrap_or_default(),
                        r.rules.join(";"),
                    ]
                })
                .collect(),
        ),
        other => anyhow::bail!("unknown insights view: {other}"),
    })
}

pub fn views(store: &Store, cur: i32) -> Result<Insights> {
    let hh = load(store)?;
    let household_years = load_household_years(store)?;
    let index = HouseholdYearIndex::build(&household_years);
    let built_at = store.get_meta("insights_built_at")?;
    // Only the objects the mart reads can make it stale; this mirrors the rebuild decision
    // in `commands::ensure_fresh_with`, so the "stale" badge and the rebuild agree.
    let newest_source_sync_at = store.newest_mart_source_sync_at()?;
    let stale = matches!((&built_at, &newest_source_sync_at), (Some(built), Some(source)) if source > built);
    let unavailable: Vec<String> = store
        .get_meta("insights_unavailable")?
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let at_risk = at_risk_rows(&hh, cur).len() as i64;
    let capabilities = source_capabilities(store)?;
    // Renewal & Engagement views read optional anchor sources; keep them empty when the
    // source is unavailable so absent data is never shown as household behavior.
    let dues_view = if cap_available(&capabilities, "renewal") {
        dues(&household_years, cur)
    } else {
        Vec::new()
    };
    let any_anchor = ["renewal", "school", "committee"]
        .iter()
        .any(|key| cap_available(&capabilities, key));
    let anchor_count_view = if any_anchor {
        anchor_count(&household_years, cur)
    } else {
        Vec::new()
    };
    let year1 = year1_indexed(&household_years, &index, cur);
    Ok(Insights {
        built_at,
        newest_source_sync_at,
        stale,
        current_fy: cur,
        unavailable,
        kpis: kpis_from_household_years(&household_years, &year1, cur, at_risk),
        trend: trend_from_household_years(&household_years, cur),
        cohort_matrix: cohort_matrix_indexed(&household_years, &index, cur),
        channels: channels_indexed(&household_years, &index, cur),
        year1,
        school: school(&hh, cur),
        reasons: reasons_from_household_years(&household_years, cur),
        multi_job: multi_job(&hh, cur),
        outcome_by_tenure: outcome_by_tenure(&hh, cur),
        school_progression: school_progression(&hh, cur),
        school_gap: school_gap(&hh, cur),
        dues: dues_view,
        anchor_type: anchor_type(&household_years, cur, &capabilities),
        anchor_count: anchor_count_view,
        zip_attrition: if cap_available(&capabilities, "zip_attrition") { zip_attrition(&hh, cur) } else { Vec::new() },
        capabilities,
    })
}

/// True only if `path` exists and canonicalizes to a location inside `dir`.
pub fn path_is_inside(path: &std::path::Path, dir: &std::path::Path) -> bool {
    match (std::fs::canonicalize(path), std::fs::canonicalize(dir)) {
        (Ok(p), Ok(d)) => p.starts_with(&d) && p != d,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_only_five_digit_us_zip_codes() {
        assert_eq!(normalize_zip(Some("10024")), Some("10024".to_string()));
        assert_eq!(normalize_zip(Some("10024-1234")), Some("10024".to_string()));
        assert_eq!(normalize_zip(Some(" 10024-1234 ")), Some("10024".to_string()));
        assert_eq!(normalize_zip(Some("1002")), None);
        assert_eq!(normalize_zip(Some("ABCDE")), None);
        assert_eq!(normalize_zip(None), None);
    }

    #[test]
    fn zip_capability_requires_an_allowed_field_with_at_least_one_normalizable_value() {
        let (_d, mut s) = mem();
        let mut rows = fixture();
        for row in &mut rows {
            row.insert("BillingPostalCode".into(), serde_json::Value::String("not a ZIP".into()));
        }
        let mut cols = ACCT_COLS.to_vec();
        cols.push("BillingPostalCode");
        seed_account(&mut s, &rows, &cols);

        let zip = source_capabilities(&s).unwrap().into_iter().find(|capability| capability.key == "zip_attrition").unwrap();
        assert!(!zip.available);
        assert!(zip.unavailable_reason.unwrap().contains("normalizable"));

        s.conn().execute("UPDATE \"Account\" SET \"BillingPostalCode\" = '10024-1234' WHERE \"Id\" = '001A'", []).unwrap();
        let zip = source_capabilities(&s).unwrap().into_iter().find(|capability| capability.key == "zip_attrition").unwrap();
        assert!(zip.available);

        s.conn().execute("UPDATE _fields SET withheld = 1 WHERE object='Account' AND field='BillingPostalCode'", []).unwrap();
        let zip = source_capabilities(&s).unwrap().into_iter().find(|capability| capability.key == "zip_attrition").unwrap();
        assert!(!zip.available);
    }

    #[test]
    fn billing_statement_zip_uses_the_latest_linked_statement_and_falls_back_to_account() {
        let (_d, mut s) = mem();
        let mut accounts = fixture();
        accounts[0].insert("BillingPostalCode".into(), serde_json::Value::String("10001".into()));
        accounts[1].insert("BillingPostalCode".into(), serde_json::Value::String("10002".into()));
        let mut account_cols = ACCT_COLS.to_vec();
        account_cols.push("BillingPostalCode");
        seed_account(&mut s, &accounts, &account_cols);
        seed_object(
            &mut s,
            "BillingStatement__c",
            &["Id", "Account__c", "Date__c", "AddressPostalCode__c", "BilledToOtherAccountId__c"],
            &[
                row(&[("Id", "old"), ("Account__c", "001A"), ("Date__c", "2024-06-01"), ("AddressPostalCode__c", "11224")]),
                row(&[("Id", "new"), ("Account__c", "001A"), ("Date__c", "2025-06-01"), ("AddressPostalCode__c", "11235"), ("BilledToOtherAccountId__c", "001B")]),
                row(&[("Id", "undated"), ("Account__c", "001B"), ("AddressPostalCode__c", "10038")]),
            ],
        );

        let zips = billing_statement_zips(&s).unwrap();
        assert_eq!(zips.get("001A"), Some(&"11235".to_string()));
        assert!(!zips.contains_key("001B"));

        rebuild(&mut s).unwrap();
        let a_zip: Option<String> = s.conn().query_row(
            "SELECT zip FROM _m_household WHERE account_id = '001A'", [], |row| row.get(0),
        ).unwrap();
        let b_zip: Option<String> = s.conn().query_row(
            "SELECT zip FROM _m_household WHERE account_id = '001B'", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(a_zip.as_deref(), Some("11235"));
        assert_eq!(b_zip.as_deref(), Some("10002"));

        s.conn().execute("UPDATE _fields SET withheld = 1 WHERE object='Account' AND field='BillingPostalCode'", []).unwrap();
        let zip = source_capabilities(&s).unwrap().into_iter().find(|capability| capability.key == "zip_attrition").unwrap();
        assert!(zip.available);
    }

    #[test]
    fn zip_attrition_uses_starting_households_and_suppresses_small_cells() {
        let households = (0..5).map(|i| Hh {
            account_id: format!("ny-{i}"), zip: Some("10024".to_string()), join_fy: Some(2024),
            resign_fy: (i == 0).then_some(2026), ..Default::default()
        }).chain(std::iter::once(Hh {
            account_id: "outside".into(), zip: Some("02108".to_string()), join_fy: Some(2024),
            ..Default::default()
        })).chain((0..4).map(|i| Hh {
            account_id: format!("suppressed-{i}"), zip: Some("10025".to_string()), join_fy: Some(2024),
            ..Default::default()
        })).collect::<Vec<_>>();

        assert_eq!(zip_attrition(&households, 2027), vec![
            ZipAttritionCell { fy: 2025, zip: "10024".into(), start_households: 5, exits: 0, attrition_rate: 0.0 },
            ZipAttritionCell { fy: 2026, zip: "10024".into(), start_households: 5, exits: 1, attrition_rate: 20.0 },
        ]);
    }

    #[test]
    fn zip_attrition_retains_eligible_non_new_york_zips_for_the_ui_to_label_unmapped() {
        let households = (0..5).map(|index| Hh {
            account_id: format!("ma-{index}"), zip: Some("02108".to_string()), join_fy: Some(2024),
            ..Default::default()
        }).collect::<Vec<_>>();
        assert!(zip_attrition(&households, 2027).iter().any(|cell| cell.fy == 2026 && cell.zip == "02108" && cell.start_households == 5));
    }

    #[test]
    fn zip_views_expose_only_aggregate_cells_and_become_unavailable_when_withheld() {
        let (_d, mut s) = mem();
        let mut rows = Vec::new();
        for index in 0..5 {
            let mut row = fixture()[0].clone();
            row.insert("Id".into(), serde_json::Value::String(format!("zip-{index}")));
            row.insert("BillingPostalCode".into(), serde_json::Value::String("10024-1234".into()));
            rows.push(row);
        }
        let mut cols = ACCT_COLS.to_vec();
        cols.push("BillingPostalCode");
        seed_account(&mut s, &rows, &cols);
        rebuild(&mut s).unwrap();
        let view = views(&s, 2027).unwrap();
        assert!(view.zip_attrition.iter().any(|cell| cell.zip == "10024" && cell.start_households == 5));
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("10024-1234") && !json.contains("zip-0"));

        s.conn().execute("UPDATE _fields SET withheld = 1 WHERE object='Account' AND field='BillingPostalCode'", []).unwrap();
        let view = views(&s, 2027).unwrap();
        assert!(view.zip_attrition.is_empty());
        assert!(!view.capabilities.iter().find(|capability| capability.key == "zip_attrition").unwrap().available);
    }

    #[test]
    fn fiscal_year_starts_june_first_and_is_labeled_by_end_year() {
        assert_eq!(fy_from_ymd(2024, 5), 2024); // May 2024 -> FY2024 (Jun 2023 – May 2024)
        assert_eq!(fy_from_ymd(2024, 6), 2025); // Jun 2024 -> FY2025
        assert_eq!(fy_of("2024-05-31"), Some(2024));
        assert_eq!(fy_of("2024-06-01"), Some(2025));
        assert_eq!(fy_of("2001-03-28T00:00:00Z"), Some(2001));
    }

    #[test]
    fn fy_of_rejects_placeholder_and_garbage_dates() {
        assert_eq!(fy_of("2199-06-02"), None);
        assert_eq!(fy_of("2991-01-01"), None);
        assert_eq!(fy_of("1899-12-31"), None);
        assert_eq!(fy_of(""), None);
        assert_eq!(fy_of("not a date"), None);
        assert_eq!(fy_of("2024-13-01"), None);
    }

    #[test]
    fn channel_flags_split_the_multipicklist_case_insensitively() {
        let f = channel_flags(Some(
            "Nursery School and Religious School;To be with Family",
        ));
        let idx = |k: &str| CHANNELS.iter().position(|(key, _)| *key == k).unwrap();
        assert!(f[idx("religious_school")]);
        assert!(f[idx("nursery_school")]);
        assert!(f[idx("family")]);
        assert!(!f[idx("clergy")]);
        assert_eq!(channel_flags(None), [false; 12]);
        assert!(channel_flags(Some("high holy day tickets"))[idx("hhd_tickets")]);
    }

    #[test]
    fn reason_group_buckets_in_priority_order() {
        assert_eq!(
            exit_labels(Some("Moved; No Longer Engaged")),
            vec!["Moved", "No longer engaged"]
        );
        assert_eq!(reason_group(Some("Moved; Non-payment")), "Structural Exit");
        assert_eq!(reason_group(Some("CJM / AM Aged Out")), "Conversion Loss");
        assert_eq!(
            reason_group(Some("Joined Another Synagogue")),
            "Addressable Churn"
        );
        assert_eq!(reason_group(Some("Elderly / Ill")), "Structural Exit");
        assert_eq!(
            reason_group(Some("Something new")),
            "Administrative or Unknown Exit"
        );
        assert_eq!(reason_group(Some("")), "Administrative or Unknown Exit");
        assert_eq!(reason_group(None), "Administrative or Unknown Exit");
    }
    #[test]
    fn exit_outcomes_need_multi_label_primary_precedence() {
        assert_eq!(
            exit_labels(Some("Moved; Non-payment")),
            vec!["Moved", "Non-payment"]
        );
        assert_eq!(reason_group(Some("Moved; Non-payment")), "Structural Exit");
        assert_eq!(reason_group(Some("Aged out")), "Conversion Loss");
        assert_eq!(reason_group(Some("")), "Administrative or Unknown Exit");
    }

    #[test]
    fn exit_reason_primary_keeps_the_fine_reason_with_structural_precedence() {
        // Affordability and disengagement stay distinct — the whole point of the split.
        assert_eq!(exit_reason_primary(Some("Financial Hardship")), "Financial hardship");
        assert_eq!(exit_reason_primary(Some("No Longer Engaged")), "No longer engaged");
        // Conversion reasons are told apart from each other.
        assert_eq!(exit_reason_primary(Some("Aged out")), "Aged out");
        assert_eq!(exit_reason_primary(Some("Introductory tier")), "Introductory tier ended");
        // Structural wins on a mixed reason, resolved to the specific reason.
        assert_eq!(exit_reason_primary(Some("Moved; Non-payment")), "Moved");
        assert_eq!(exit_reason_primary(Some("Elderly / Ill")), "Elderly / ill");
        // Deceased, uncoded, and administrative all fold into the not-actionable tail.
        assert_eq!(exit_reason_primary(Some("Deceased")), OTHER_EXIT);
        assert_eq!(exit_reason_primary(Some("Administrative")), OTHER_EXIT);
        assert_eq!(exit_reason_primary(Some("")), OTHER_EXIT);
        assert_eq!(exit_reason_primary(None), OTHER_EXIT);
    }

    #[test]
    fn parse_rs_year_takes_the_end_year_of_a_school_year() {
        assert_eq!(parse_rs_year(Some("2025-2026")), Some(2026));
        assert_eq!(parse_rs_year(Some("2007")), Some(2007));
        assert_eq!(parse_rs_year(Some("")), None);
        assert_eq!(parse_rs_year(None), None);
    }

    #[test]
    fn membership_dues_exclude_non_dues_product_families() {
        assert_eq!(
            dues_class(Some("Membership"), Some("Annual Membership Dues")),
            DuesClass::Membership
        );
        assert_eq!(
            dues_class(Some("Membership"), Some("Membership Security Fee")),
            DuesClass::SecurityFee
        );
        assert_eq!(
            dues_class(Some("Gift"), Some("High Holiday Gift")),
            DuesClass::Gift
        );
        assert_eq!(
            dues_class(Some("School"), Some("Religious School Tuition")),
            DuesClass::Tuition
        );
        assert_eq!(
            dues_class(Some("Events"), Some("Gala Tickets")),
            DuesClass::Event
        );
        assert_eq!(
            dues_class(Some("Sales"), Some("Gift Shop Sale")),
            DuesClass::Sale
        );
        assert_eq!(dues_class(None, Some("Parking")), DuesClass::Other);
    }

    /// Reference implementation: the original per-household-year linear scan over every
    /// statement and line. `DuesIndex` must reproduce it exactly, including the f64 sum
    /// order of `dues_billed`.
    fn dues_evidence_naive(
        household_id: &str,
        fiscal_year: i32,
        statements: &[BillingStatement<'_>],
        lines: &[BillingStatementLine<'_>],
    ) -> DuesEvidence {
        let matching: Vec<_> = statements
            .iter()
            .filter(|statement| {
                statement.household_id == Some(household_id)
                    && statement.issued_at.and_then(fy_of) == Some(fiscal_year)
            })
            .collect();
        let qualifying: Vec<_> = lines
            .iter()
            .filter(|line| {
                matching
                    .iter()
                    .any(|statement| line.statement_id == Some(statement.id))
                    && dues_class(line.product_family, line.product_name) == DuesClass::Membership
            })
            .collect();
        if qualifying.is_empty() {
            return DuesEvidence {
                coverage: BillingCoverage::Missing,
                dues_billed: 0.0,
                settlement: SettlementState::Unknown,
            };
        }
        DuesEvidence {
            coverage: BillingCoverage::Present,
            dues_billed: qualifying.iter().filter_map(|line| line.amount).sum(),
            settlement: combined_settlement(qualifying.iter().map(|line| line_settlement(line))),
        }
    }

    #[test]
    fn dues_index_matches_the_naive_scan_exactly() {
        let stmt = |id: &'static str, hh: Option<&'static str>, at: Option<&'static str>| {
            BillingStatement {
                id,
                household_id: hh,
                issued_at: at,
            }
        };
        let statements = [
            stmt("s1", Some("hh-1"), Some("2024-07-01")), // hh-1 FY2025
            stmt("s2", Some("hh-1"), Some("2024-09-15")), // hh-1 FY2025 (second statement)
            stmt("s3", Some("hh-1"), Some("2023-07-01")), // hh-1 FY2024
            stmt("s4", Some("hh-2"), Some("2024-07-01")), // hh-2 FY2025
            stmt("s5", None, Some("2024-07-01")),         // no household: never matches
            stmt("s6", Some("hh-3"), None),               // no date: never matches
            stmt("s7", Some("hh-3"), Some("not-a-date")), // unparsable: never matches
            stmt("", Some("hh-4"), Some("2024-07-01")),   // "" is a legal statement id
            stmt("dup", Some("hh-5"), Some("2024-07-01")), // same id on two household-years
            stmt("dup", Some("hh-6"), Some("2024-07-01")),
            stmt("dup", Some("hh-5"), Some("2024-08-01")), // same id, same household-year twice
        ];
        let other = |statement_id: Option<&'static str>, amount: f64| BillingStatementLine {
            statement_id,
            product_family: Some("Fees"),
            product_name: Some("Security Fee"),
            amount: Some(amount),
            received: Some(0.0),
            balance: Some(amount),
        };
        let lines = [
            // hh-1 FY2025: amounts whose f64 sum depends on order (0.1 + 0.2 + 0.3).
            BillingStatementLine::dues("s1", 0.1, Some(0.1), Some(0.0)),
            other(Some("s1"), 75.0),
            BillingStatementLine::dues("s2", 0.2, Some(0.0), Some(0.2)),
            BillingStatementLine::dues("s3", 400.0, Some(400.0), Some(0.0)),
            BillingStatementLine::dues("s1", 0.3, None, None),
            BillingStatementLine::dues("s4", 500.0, Some(100.0), Some(400.0)),
            BillingStatementLine::dues("s5", 999.0, Some(999.0), Some(0.0)),
            BillingStatementLine::dues("s6", 999.0, Some(999.0), Some(0.0)),
            BillingStatementLine::dues("s7", 999.0, Some(999.0), Some(0.0)),
            BillingStatementLine::dues("", 250.0, Some(250.0), Some(0.0)),
            BillingStatementLine::dues("dup", 10.0, Some(10.0), Some(0.0)),
            BillingStatementLine::dues("missing-parent", 999.0, Some(0.0), Some(999.0)),
            BillingStatementLine {
                statement_id: None,
                ..BillingStatementLine::dues("s1", 999.0, Some(0.0), Some(999.0))
            },
            BillingStatementLine {
                amount: None,
                ..BillingStatementLine::dues("s2", 0.0, Some(0.0), Some(0.0))
            },
            other(None, 1.0),
        ];
        let index = DuesIndex::build(&statements, &lines);
        let households = ["hh-1", "hh-2", "hh-3", "hh-4", "hh-5", "hh-6", "hh-7", ""];
        let mut present = 0;
        for household in households {
            for fy in 2023..=2026 {
                let naive = dues_evidence_naive(household, fy, &statements, &lines);
                let indexed = index.evidence(household, fy);
                assert_eq!(indexed, naive, "{household} FY{fy}");
                assert_eq!(dues_evidence(household, fy, &statements, &lines), naive);
                if naive.coverage == BillingCoverage::Present {
                    present += 1;
                }
            }
        }
        assert_eq!(present, 6, "hh-1 x2, hh-2, hh-4, hh-5, hh-6");
        let hh1 = index.evidence("hh-1", 2025);
        assert_eq!(hh1.dues_billed.to_bits(), (0.1f64 + 0.2 + 0.3).to_bits());
        assert_eq!(hh1.settlement, SettlementState::Unknown);
    }

    #[test]
    fn dues_evidence_joins_lines_through_statements_and_marks_coverage() {
        let statements = [BillingStatement {
            id: "stmt-1",
            household_id: Some("hh-1"),
            issued_at: Some("2024-07-01"),
        }];
        let lines = [
            BillingStatementLine {
                statement_id: Some("stmt-1"),
                product_family: Some("Membership"),
                product_name: Some("Annual Membership Dues"),
                amount: Some(500.0),
                received: Some(500.0),
                balance: Some(0.0),
            },
            BillingStatementLine {
                statement_id: Some("stmt-1"),
                product_family: Some("Fees"),
                product_name: Some("Security Fee"),
                amount: Some(75.0),
                received: Some(75.0),
                balance: Some(0.0),
            },
            BillingStatementLine {
                statement_id: Some("missing-parent"),
                product_family: Some("Membership"),
                product_name: Some("Annual Membership Dues"),
                amount: Some(999.0),
                received: Some(0.0),
                balance: Some(999.0),
            },
        ];

        let evidence = dues_evidence("hh-1", 2025, &statements, &lines);
        assert_eq!(evidence.coverage, BillingCoverage::Present);
        assert_eq!(evidence.dues_billed, 500.0);
        assert_eq!(evidence.settlement, SettlementState::Settled);
        assert_eq!(
            dues_evidence("hh-2", 2025, &statements, &lines).coverage,
            BillingCoverage::Missing
        );
    }

    #[test]
    fn dues_evidence_labels_final_mirror_values_as_eventual_settlement() {
        let statements = [
            BillingStatement {
                id: "partial",
                household_id: Some("hh-1"),
                issued_at: Some("2024-06-01"),
            },
            BillingStatement {
                id: "unsettled",
                household_id: Some("hh-2"),
                issued_at: Some("2024-06-01"),
            },
        ];
        let lines = [
            BillingStatementLine::dues("partial", 500.0, Some(100.0), Some(400.0)),
            BillingStatementLine::dues("unsettled", 500.0, Some(0.0), Some(500.0)),
        ];

        let partial = dues_evidence("hh-1", 2025, &statements, &lines);
        assert_eq!(partial.settlement, SettlementState::PartiallySettled);
        assert_eq!(
            partial.settlement_label(),
            "Eventual settlement: partially settled"
        );

        let unsettled = dues_evidence("hh-2", 2025, &statements, &lines);
        assert_eq!(unsettled.settlement, SettlementState::Unsettled);
        assert_eq!(
            unsettled.settlement_label(),
            "Eventual settlement: unsettled"
        );

        let missing = dues_evidence("hh-3", 2025, &statements, &lines);
        assert_eq!(missing.coverage, BillingCoverage::Missing);
        assert_eq!(missing.settlement, SettlementState::Unknown);
    }

    #[test]
    fn settlement_reads_the_dues_line_not_the_statement_total() {
        // One statement: dues fully paid, a security fee still owed. The statement total
        // says partially settled; the dues line says settled — and the label describes
        // dues, so it must follow the line.
        let statements = [BillingStatement {
            id: "stmt",
            household_id: Some("hh-1"),
            issued_at: Some("2024-07-01"),
        }];
        let lines = [
            BillingStatementLine::dues("stmt", 500.0, Some(500.0), Some(0.0)),
            BillingStatementLine {
                statement_id: Some("stmt"),
                product_family: Some("Dues"),
                product_name: Some("Membership Security Fee"),
                amount: Some(75.0),
                received: Some(0.0),
                balance: Some(75.0),
            },
        ];
        let evidence = dues_evidence("hh-1", 2025, &statements, &lines);
        assert_eq!(evidence.dues_billed, 500.0);
        assert_eq!(evidence.settlement, SettlementState::Settled);

        // The mirror image: dues unpaid, the security fee paid. The statement total says
        // partially settled; the dues line is unsettled.
        let lines = [
            BillingStatementLine::dues("stmt", 500.0, Some(0.0), Some(500.0)),
            BillingStatementLine {
                statement_id: Some("stmt"),
                product_family: Some("Dues"),
                product_name: Some("Membership Security Fee"),
                amount: Some(75.0),
                received: Some(75.0),
                balance: Some(0.0),
            },
        ];
        let evidence = dues_evidence("hh-1", 2025, &statements, &lines);
        assert_eq!(evidence.settlement, SettlementState::Unsettled);
    }

    #[test]
    fn dues_line_without_settlement_fields_is_unknown() {
        // The statement total is fully paid, but the dues line itself carries no
        // balance/received values: settlement falls back to Unknown rather than
        // borrowing the statement-level figure.
        let statements = [BillingStatement {
            id: "stmt",
            household_id: Some("hh-1"),
            issued_at: Some("2024-07-01"),
        }];
        let lines = [BillingStatementLine::dues("stmt", 500.0, None, None)];
        let evidence = dues_evidence("hh-1", 2025, &statements, &lines);
        assert_eq!(evidence.coverage, BillingCoverage::Present);
        assert_eq!(evidence.settlement, SettlementState::Unknown);
        assert_eq!(evidence.settlement_label(), "Eventual settlement: unknown");
    }

    #[test]
    fn enrollment_normalization_distinguishes_confirmed_anchors_from_withdrawals() {
        let nursery = normalize_enrollment(Some("Nursery School Pre-K"), Some("Confirmed"));
        assert_eq!(nursery.school, Some(SchoolType::Nursery));
        assert_eq!(nursery.outcome, EnrollmentOutcome::Confirmed);
        assert_eq!(nursery.anchor, Some(RelationshipAnchor::NurserySchool));

        let religious = normalize_enrollment(Some("Religious School Grade 3"), Some("Enrolled"));
        assert_eq!(religious.school, Some(SchoolType::Religious));
        assert_eq!(religious.anchor, Some(RelationshipAnchor::ReligiousSchool));

        let withdrawn = normalize_enrollment(Some("Religious School Grade 3"), Some("Withdrawn"));
        assert_eq!(withdrawn.school, Some(SchoolType::Religious));
        assert_eq!(withdrawn.outcome, EnrollmentOutcome::Withdrawn);
        assert_eq!(withdrawn.anchor, None, "withdrawal is not an anchor");

        let pending = normalize_enrollment(Some("Nursery School Pre-K"), Some("Pending"));
        assert_eq!(pending.outcome, EnrollmentOutcome::Other);
        assert_eq!(pending.anchor, None);
    }

    #[test]
    fn committee_normalization_treats_far_future_end_dates_as_open_and_respects_is_active() {
        let current = normalize_committee(Some("2024-06-01"), Some("2199-12-31"), Some("true"));
        assert_eq!(current.start_fy, Some(2025));
        assert_eq!(current.end_fy, None);
        assert!(current.open_ended);
        assert!(current.current_active);

        let inactive = normalize_committee(Some("2024-06-01"), None, Some("false"));
        assert_eq!(inactive.start_fy, Some(2025));
        assert!(!inactive.open_ended);
        assert!(inactive.end_fy.is_none());
        assert!(!inactive.current_active, "IsActive__c is authoritative");

        let ended = normalize_committee(Some("2023-06-01"), Some("2024-05-31"), Some("true"));
        assert_eq!(ended.end_fy, Some(2024));
        assert!(
            ended.current_active,
            "date history does not override IsActive__c"
        );
    }

    use crate::salesforce::Row;
    use crate::store::{open, Store};

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    pub(super) fn mem() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = open(&dir.path().join("t.db"), KEY).unwrap();
        (dir, s)
    }

    /// Column order for the synthetic Account mirror used by these tests.
    pub(super) const ACCT_COLS: [&str; 16] = [
        "Id",
        "Name",
        "Type",
        "IsATempleMember__c",
        "IsResigned__c",
        "Join_Date__c",
        "OriginalJoinDate__c",
        "LastResignDate__c",
        "Sub_Type__c",
        "Member_Category__c",
        "Join_Reason__c",
        "Resign_Reason__c",
        "FormerReligiousSchoolStudents__c",
        "ActiveReligiousSchoolStudents__c",
        "WasEverNSAffiliated__c",
        "LastYearAttendedRS__c",
    ];

    /// Build one Account row. `vals` is positional per ACCT_COLS; "" means NULL.
    pub(super) fn acct(vals: [&str; 16]) -> Row {
        let mut m = Row::new();
        for (c, v) in ACCT_COLS.iter().zip(vals.iter()) {
            if !v.is_empty() {
                m.insert((*c).into(), serde_json::Value::String((*v).into()));
            }
        }
        m
    }

    pub(super) fn seed_account(s: &mut Store, rows: &[Row], cols: &[&str]) {
        s.upsert_object("Account", "Account", rows.len() as i64)
            .unwrap();
        for c in cols {
            s.upsert_field("Account", c, "string", c, false).unwrap();
        }
        let cols: Vec<String> = cols.iter().map(|c| c.to_string()).collect();
        s.replace_mirror("Account", &cols, rows).unwrap();
    }

    /// Seed one synthetic optional-source mirror table and mark it synced.
    fn seed_object(s: &mut Store, object: &str, cols: &[&str], rows: &[Row]) {
        s.upsert_object(object, object, rows.len() as i64).unwrap();
        for c in cols {
            s.upsert_field(object, c, "string", c, false).unwrap();
        }
        let owned: Vec<String> = cols.iter().map(|c| c.to_string()).collect();
        s.replace_mirror(object, &owned, rows).unwrap();
    }

    /// Build a mirror row from (column, value) pairs; "" means NULL.
    fn row(pairs: &[(&str, &str)]) -> Row {
        let mut m = Row::new();
        for (k, v) in pairs {
            if !v.is_empty() {
                m.insert((*k).into(), serde_json::Value::String((*v).into()));
            }
        }
        m
    }

    fn fixture() -> Vec<Row> {
        vec![
            // current voting member, joined FY2015 (Sept 2014), RS family, reason RS
            acct([
                "001A",
                "Cohen",
                "Member Family",
                "true",
                "false",
                "2014-09-01",
                "2014-09-01",
                "",
                "Voting Member",
                "MAIN",
                "Religious School",
                "",
                "2",
                "0",
                "false",
                "2023-2024",
            ]),
            // resigned FY2020 (Aug 2019), joined FY2015, reason Nursery+RS, non-payment
            acct([
                "001B",
                "Levy",
                "Member Family",
                "false",
                "true",
                "2014-08-15",
                "2014-08-15",
                "2019-08-01",
                "Voting Member",
                "MAIN",
                "Nursery School and Religious School",
                "Non-payment",
                "1",
                "0",
                "true",
                "2019-2020",
            ]),
            // rejoiner: original 2005, left 2010, rejoined FY2023 (Jul 2022), current
            acct([
                "001C",
                "Adler",
                "Member Family",
                "true",
                "false",
                "2022-07-10",
                "2005-01-01",
                "2010-05-31",
                "Young Professionals",
                "Young Professionals Introductory Membership",
                "Community;Young Professionals",
                "",
                "0",
                "0",
                "false",
                "",
            ]),
            // resigned, date unknown, joined FY2018
            acct([
                "001D",
                "Roth",
                "Member Family",
                "false",
                "true",
                "2017-10-01",
                "2017-10-01",
                "",
                "Voting Member",
                "MAIN",
                "",
                "Moved",
                "0",
                "0",
                "false",
                "",
            ]),
            // placeholder join date -> bad_join_date
            acct([
                "001E",
                "Katz",
                "Member Family",
                "true",
                "false",
                "2199-06-02",
                "2199-06-02",
                "",
                "Voting Member",
                "MAIN",
                "",
                "",
                "0",
                "0",
                "false",
                "",
            ]),
            // not a member family at all -> excluded
            acct([
                "001F",
                "Some Vendor",
                "Vendor",
                "false",
                "false",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
            ]),
            // current, joined last FY (FY2025 = Jul 2024) via nursery school only
            acct([
                "001G",
                "Green",
                "Member Family",
                "true",
                "false",
                "2024-07-01",
                "2024-07-01",
                "",
                "Voting Member",
                "MAIN",
                "Nursery School",
                "",
                "0",
                "0",
                "true",
                "",
            ]),
        ]
    }

    #[test]
    fn rebuild_derives_the_mart_and_reports_unavailable_columns() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        let info = rebuild(&mut s).unwrap();
        assert_eq!(info.households, 6, "vendor row excluded");
        assert!(info.unavailable.is_empty());
        let hh = load(&s).unwrap();
        let by = |id: &str| hh.iter().find(|h| h.account_id == id).unwrap().clone();

        let a = by("001A");
        assert!(a.is_current && !a.is_resigned);
        assert_eq!(
            (a.join_fy, a.cohort_fy, a.resign_fy),
            (Some(2015), Some(2015), None)
        );
        assert!(a.ch[0], "religious_school flag");
        assert!(a.rs_family && !a.ns_family);
        assert_eq!(a.last_rs_year, Some(2024));
        assert_eq!(a.resign_reason_group, "(not coded)");

        let b = by("001B");
        assert_eq!(b.resign_fy, Some(2020));
        assert!(b.ch[0] && b.ch[1]);
        assert_eq!(b.resign_reason_group, "Addressable Churn");
        assert!(b.ns_family);

        let c = by("001C");
        assert!(c.rejoined, "original join before latest join");
        assert_eq!(
            c.resign_fy, None,
            "a current member's old resign date is not a resignation"
        );
        assert_eq!(c.cohort_fy, Some(2005));
        assert_eq!(c.join_fy, Some(2023));

        let d = by("001D");
        assert!(d.resigned_unknown_date);
        assert_eq!(d.resign_fy, None);

        let e = by("001E");
        assert!(e.bad_join_date);
        assert_eq!(e.join_fy, None);

        assert!(s.get_meta("insights_built_at").unwrap().is_some());
        assert!(s.table_exists(MART).unwrap());
        assert!(s.table_exists(MART_FY).unwrap());
        let fy_rows = load_household_years(&s).unwrap();
        assert!(fy_rows.iter().any(|row| {
            row.account_id == "001B"
                && row.fy == 2020
                && row.resigned_this_fy
                && !row.active_end_of_fy
                && row.exit_reason.as_deref() == Some("Non-payment")
        }));
    }

    #[test]
    fn rebuild_without_join_reason_column_marks_channels_unavailable() {
        let (_d, mut s) = mem();
        let cols: Vec<&str> = ACCT_COLS
            .iter()
            .copied()
            .filter(|c| *c != "Join_Reason__c")
            .collect();
        let rows: Vec<Row> = fixture()
            .into_iter()
            .map(|mut r| {
                r.remove("Join_Reason__c");
                r
            })
            .collect();
        seed_account(&mut s, &rows, &cols);
        let info = rebuild(&mut s).unwrap();
        assert_eq!(info.unavailable, vec!["Join_Reason__c".to_string()]);
        assert!(load(&s)
            .unwrap()
            .iter()
            .all(|h| h.ch == [false; 12] && h.join_reason.is_none()));
    }

    #[test]
    fn rebuild_reports_five_phases_in_order_with_counts() {
        use crate::progress::{ProgressEvent, Reporter};
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        let mut events: Vec<ProgressEvent> = Vec::new();
        let mut sink = |e: &ProgressEvent| events.push(e.clone());
        let mut reporter =
            Reporter::new("rebuild", 5, &mut sink).with_min_interval(std::time::Duration::ZERO);
        let info = rebuild_with(&mut s, &mut reporter).unwrap();

        assert!(events.iter().all(|e| e.job == "rebuild" && e.steps == 5));
        // Phase transitions (events without counters) arrive in the fixed order, 1..=5.
        let phases: Vec<(u32, &str)> = events
            .iter()
            .filter(|e| e.done.is_none())
            .map(|e| (e.step, e.phase.as_str()))
            .collect();
        assert_eq!(
            phases,
            vec![
                (1, "Reading membership records"),
                (2, "Building yearly membership history"),
                (3, "Applying engagement sources"),
                (4, "Writing analysis tables"),
                (5, "Finalizing"),
                (5, "Finalizing"), // finish()
            ]
        );
        // Steps never go backwards and the last event is terminal.
        assert!(events.windows(2).all(|w| w[0].step <= w[1].step));
        assert_eq!(events.last().unwrap().step, 5);
        // Phase 1 counts Account rows: the terminal tick has done == total == seeded rows.
        let read_ticks: Vec<&ProgressEvent> = events
            .iter()
            .filter(|e| e.step == 1 && e.done.is_some())
            .collect();
        let last = read_ticks.last().expect("phase 1 ticks");
        assert_eq!(last.done, Some(6));
        assert_eq!(last.total, Some(6));
        // Phase 4 writes every household and household-year row.
        let write_last = events
            .iter()
            .filter(|e| e.step == 4 && e.done.is_some())
            .last()
            .expect("phase 4 ticks");
        let fy_rows = load_household_years(&s).unwrap().len() as u64;
        assert_eq!(write_last.done, Some(6 + fy_rows));
        assert_eq!(write_last.total, Some(6 + fy_rows));

        // Progress is a pure side effect: the rebuild result is unchanged.
        let plain = rebuild(&mut s).unwrap();
        assert_eq!(info.households, plain.households);
        assert_eq!(info.unavailable, plain.unavailable);
    }

    #[test]
    fn source_capabilities_require_every_required_mirror_and_report_freshness() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        for object in ["BillingStatement__c", "BillingStatementLine__c"] {
            s.upsert_object(object, object, 0).unwrap();
            s.upsert_field(object, "Id", "id", "Id", false).unwrap();
            s.replace_mirror(object, &["Id".into()], &[]).unwrap();
        }

        let capabilities = source_capabilities(&s).unwrap();
        let get = |key: &str| {
            capabilities
                .iter()
                .find(|capability| capability.key == key)
                .unwrap()
        };
        assert!(get("membership").available);
        assert!(get("renewal").available);
        assert!(!get("school").available);
        assert_eq!(
            get("school").unavailable_reason.as_deref(),
            Some("Select and sync Class_Enrolment__c")
        );
        assert!(!get("committee").available);

        rebuild(&mut s).unwrap();
        let insight = views(&s, 2026).unwrap();
        assert!(insight.newest_source_sync_at.is_some());
        assert!(!insight.stale, "rebuild follows the source sync");
        assert_eq!(insight.capabilities, capabilities);

        s.set_meta("insights_built_at", "2000-01-01T00:00:00Z")
            .unwrap();
        assert!(views(&s, 2026).unwrap().stale);
    }

    #[test]
    fn only_a_sync_of_a_mart_source_makes_the_views_stale() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        s.set_meta("insights_built_at", "2027-01-01T00:00:00Z")
            .unwrap();
        let sync_at = |s: &Store, object: &str, at: &str| {
            s.upsert_object(object, object, 0).unwrap();
            s.conn()
                .execute(
                    "UPDATE _objects SET last_synced_at = ?2 WHERE name = ?1",
                    params![object, at],
                )
                .unwrap();
        };

        // Contact is never read by the mart: a newer sync of it changes nothing.
        sync_at(&s, "Contact", "2027-02-01T00:00:00Z");
        let v = views(&s, 2026).unwrap();
        assert!(!v.stale, "an unrelated sync must not mark the mart stale");
        assert!(
            v.newest_source_sync_at.as_deref() < Some("2027-01-01T00:00:00Z"),
            "the reported source sync is the mart's own sources, not Contact's: {:?}",
            v.newest_source_sync_at
        );

        // Account is a mart source: a newer sync of it does.
        sync_at(&s, "Account", "2027-03-01T00:00:00Z");
        let v = views(&s, 2026).unwrap();
        assert!(v.stale);
        assert_eq!(
            v.newest_source_sync_at.as_deref(),
            Some("2027-03-01T00:00:00Z")
        );
    }

    #[test]
    fn rebuild_honors_withheld_fields_not_just_synced_columns() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        s.conn()
            .execute(
                "UPDATE _fields SET withheld = 1 WHERE object='Account' AND field='Join_Reason__c'",
                [],
            )
            .unwrap();
        let info = rebuild(&mut s).unwrap();
        assert_eq!(info.unavailable, vec!["Join_Reason__c".to_string()]);
        assert!(load(&s).unwrap().iter().all(|h| h.join_reason.is_none()));
    }

    #[test]
    fn current_spell_suppresses_old_exit_evidence() {
        let (_d, mut s) = mem();
        let mut rows = fixture();
        rows.push(acct([
            "001H",
            "Hale",
            "Member Family",
            "true",
            "false",
            "2018-06-01",
            "2016-06-01",
            "2017-05-31",
            "Voting Member",
            "MAIN",
            "",
            "Moved; Non-payment",
            "0",
            "0",
            "false",
            "",
        ]));
        seed_account(&mut s, &rows, &ACCT_COLS);
        rebuild(&mut s).unwrap();
        let hh = load(&s).unwrap();
        let h = hh.iter().find(|h| h.account_id == "001H").unwrap();
        assert_eq!(h.resign_fy, None);
        assert_eq!(h.resign_reason_group, "(not coded)");
    }

    #[test]
    fn encrypted_lifecycle_fixture_covers_each_membership_outcome() {
        let (_d, mut s) = mem();
        let mut rows = fixture();
        rows.extend([
            acct([
                "001I",
                "Intro",
                "Member Family",
                "false",
                "true",
                "2020-06-01",
                "2020-06-01",
                "2022-06-01",
                "Young Professionals",
                "MAIN",
                "Young Professionals",
                "Aged out",
                "0",
                "0",
                "false",
                "",
            ]),
            acct([
                "001J",
                "Unknown",
                "Member Family",
                "false",
                "true",
                "2020-06-01",
                "2020-06-01",
                "2022-06-01",
                "Voting Member",
                "MAIN",
                "",
                "Administrative",
                "0",
                "0",
                "false",
                "",
            ]),
        ]);
        seed_account(&mut s, &rows, &ACCT_COLS);
        rebuild(&mut s).unwrap();
        let rows = load(&s).unwrap();
        let outcome = |id: &str| {
            rows.iter()
                .find(|row| row.account_id == id)
                .unwrap()
                .resign_reason_group
                .as_str()
        };
        assert_eq!(outcome("001B"), "Addressable Churn");
        assert_eq!(outcome("001D"), "Structural Exit");
        assert_eq!(outcome("001I"), "Conversion Loss");
        assert_eq!(outcome("001J"), "Administrative or Unknown Exit");
        assert!(rows
            .iter()
            .any(|row| row.account_id == "001C" && row.rejoined && row.is_current));
        assert!(load_household_years(&s)
            .unwrap()
            .iter()
            .any(|row| row.account_id == "001A" && row.active_end_of_fy));
    }

    /// Two current households; A joined FY2021, B joined FY2020.
    fn anchor_accounts() -> Vec<Row> {
        vec![
            acct([
                "accA",
                "A Family",
                "Member Family",
                "true",
                "false",
                "2020-06-01",
                "2020-06-01",
                "",
                "Voting Member",
                "MAIN",
                "",
                "",
                "0",
                "0",
                "false",
                "",
            ]),
            acct([
                "accB",
                "B Family",
                "Member Family",
                "true",
                "false",
                "2019-06-01",
                "2019-06-01",
                "",
                "Voting Member",
                "MAIN",
                "",
                "",
                "0",
                "0",
                "false",
                "",
            ]),
        ]
    }

    #[test]
    fn anchors_populate_from_optional_mirror_sources() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &anchor_accounts(), &ACCT_COLS);
        // Renewal: A billed membership dues in FY2025 (statement dated Sep 2024). The dues
        // line is fully paid; a security fee on the same statement is still owed, so the
        // statement total would read "partially settled" while the dues line is settled.
        seed_object(
            &mut s,
            "BillingStatement__c",
            &["Id", "Account__c", "Date__c"],
            &[row(&[
                ("Id", "st1"),
                ("Account__c", "accA"),
                ("Date__c", "2024-09-01"),
            ])],
        );
        seed_object(
            &mut s,
            "BillingStatementLine__c",
            &[
                "Id",
                "BillingStatement__c",
                "Billing_PrimaryProductFamily__c",
                "Billing_PrimaryProductName__c",
                "Charges__c",
                "Billing_ReceivedAmount__c",
                "Billing_Balance__c",
            ],
            &[
                row(&[
                    ("Id", "ln1"),
                    ("BillingStatement__c", "st1"),
                    ("Billing_PrimaryProductFamily__c", "Dues"),
                    (
                        "Billing_PrimaryProductName__c",
                        "Membership Dues (Unreserved)",
                    ),
                    ("Charges__c", "1000"),
                    ("Billing_ReceivedAmount__c", "1000"),
                    ("Billing_Balance__c", "0"),
                ]),
                row(&[
                    ("Id", "ln2"),
                    ("BillingStatement__c", "st1"),
                    ("Billing_PrimaryProductFamily__c", "Dues"),
                    ("Billing_PrimaryProductName__c", "Membership Security Fee"),
                    ("Charges__c", "75"),
                    ("Billing_ReceivedAmount__c", "0"),
                    ("Billing_Balance__c", "75"),
                ]),
            ],
        );
        // School: A confirmed Religious School enrolment for the 2024-2025 year (FY2025).
        seed_object(
            &mut s,
            "Class_Enrolment__c",
            &[
                "Id",
                "Account__c",
                "IsNursery__c",
                "IsReligious__c",
                "Status__c",
                "Academic_Year__c",
            ],
            &[row(&[
                ("Id", "en1"),
                ("Account__c", "accA"),
                ("IsNursery__c", "false"),
                ("IsReligious__c", "true"),
                ("Status__c", "Confirmed"),
                ("Academic_Year__c", "2024-2025"),
            ])],
        );
        // Committee: B active from FY2025 with a far-future placeholder end date.
        seed_object(
            &mut s,
            "Committee_Membership__c",
            &[
                "Id",
                "Account__c",
                "Member_From__c",
                "Member_To__c",
                "IsActive__c",
            ],
            &[row(&[
                ("Id", "cm1"),
                ("Account__c", "accB"),
                ("Member_From__c", "2024-06-01"),
                ("Member_To__c", "2199-12-31"),
                ("IsActive__c", "true"),
            ])],
        );
        rebuild(&mut s).unwrap();

        let years = load_household_years(&s).unwrap();
        let at = |id: &str, fy: i32| {
            years
                .iter()
                .find(|r| r.account_id == id && r.fy == fy)
                .unwrap()
        };
        let a25 = at("accA", 2025);
        assert!(a25.anchor_dues && a25.anchor_religious && !a25.anchor_committee);
        assert_eq!(
            a25.dues_settlement.as_deref(),
            Some("Eventual settlement: settled"),
            "settlement follows the dues line, not the statement total"
        );
        assert_eq!(a25.anchor_count(), 2);
        let b25 = at("accB", 2025);
        assert!(b25.anchor_committee && !b25.anchor_dues);
        // Open-ended committee membership stays active in later fiscal years.
        assert!(at("accB", 2026).anchor_committee);
        // Every source carried rows for FY2025, so both households are observed for every
        // family that year; B's missing dues line is coverage missing on an observed year.
        assert!(a25.renewal_observed && a25.school_observed && a25.committee_observed);
        assert!(b25.renewal_observed && b25.dues_coverage_missing);
        // FY2024 has no statement, enrolment, or committee rows: unobserved, not zero.
        let a24 = at("accA", 2024);
        assert!(!a24.renewal_observed && !a24.school_observed && !a24.committee_observed);
        assert!(
            a24.dues_coverage_missing,
            "coverage missing is derived independently of observation"
        );

        let cur = current_fy();
        let caps = source_capabilities(&s).unwrap();
        let dues = dues(&years, cur);
        let fy25 = dues.iter().find(|d| d.fy == 2025).unwrap();
        assert_eq!((fy25.billed, fy25.settled), (1, 1));
        assert!(
            fy25.coverage_missing >= 1,
            "B is active in FY2025 with no dues line"
        );
        let anchors = anchor_type(&years, cur, &caps);
        let keys: Vec<&str> = anchors.iter().map(|a| a.key.as_str()).collect();
        assert!(
            keys.contains(&"dues") && keys.contains(&"religious") && keys.contains(&"committee")
        );
        let counts = anchor_count(&years, cur);
        assert!(
            counts.iter().any(|c| c.anchors == 2 && c.n == 1),
            "A held two anchors"
        );
        assert!(
            counts.iter().any(|c| c.anchors == 1 && c.n == 1),
            "B held one anchor"
        );
    }

    #[test]
    fn unavailable_optional_sources_leave_anchors_and_views_empty() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &anchor_accounts(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        assert!(load_household_years(&s)
            .unwrap()
            .iter()
            .all(|r| r.anchor_count() == 0 && r.dues_settlement.is_none()));
        let v = views(&s, current_fy()).unwrap();
        assert!(v.dues.is_empty() && v.anchor_type.is_empty() && v.anchor_count.is_empty());
        assert!(v
            .capabilities
            .iter()
            .any(|c| c.key == "renewal" && !c.available));
    }

    #[test]
    fn rebuild_fails_cleanly_when_account_is_not_synced() {
        let (_d, mut s) = mem();
        let err = rebuild(&mut s).unwrap_err().to_string();
        assert!(err.contains("Account"), "{err}");
    }

    #[test]
    fn failed_rebuild_keeps_the_prior_household_and_household_year_marts() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        let prior_households = load(&s).unwrap().len();
        let prior_years = load_household_years(&s).unwrap().len();
        s.conn().execute("DROP TABLE Account", []).unwrap();

        assert!(rebuild(&mut s).is_err());
        assert_eq!(load(&s).unwrap().len(), prior_households);
        assert_eq!(load_household_years(&s).unwrap().len(), prior_years);
    }

    #[test]
    fn purge_drops_the_mart() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        s.purge_mirror().unwrap();
        assert!(!s.table_exists(MART).unwrap());
        assert!(!s.table_exists(MART_FY).unwrap());
    }

    fn h(id: &str, current: bool, join: Option<i32>, resign: Option<i32>) -> Hh {
        Hh {
            account_id: id.into(),
            is_current: current,
            is_resigned: !current,
            join_fy: join,
            cohort_fy: join,
            resign_fy: if current { None } else { resign },
            resigned_unknown_date: !current && resign.is_none(),
            resign_reason_group: "(not coded)".into(),
            ..Default::default()
        }
    }

    #[test]
    fn membership_spell_rules() {
        let cur = h("a", true, Some(2015), None);
        assert!(member_in(&cur, 2015) && member_in(&cur, 2026));
        assert!(!member_in(&cur, 2014));
        let gone = h("b", false, Some(2015), Some(2020));
        assert!(member_in(&gone, 2019) && !member_in(&gone, 2020));
        let unknown = h("c", false, Some(2018), None);
        assert!(
            member_in(&unknown, 2018) && !member_in(&unknown, 2019),
            "unknown resign date = lost after year 1"
        );
        let nojoin = h("d", true, None, None);
        assert!(!member_in(&nojoin, 2026));
    }

    #[test]
    fn trend_counts_joins_resigns_and_active() {
        let hh = vec![
            h("a", true, Some(2020), None),
            h("b", false, Some(2020), Some(2022)),
            h("c", false, Some(2021), Some(2022)),
            h("d", true, Some(2022), None),
        ];
        let t = trend(&hh, 2023);
        let row = |fy: i32| t.iter().find(|r| r.fy == fy).unwrap().clone();
        assert_eq!(
            (
                row(2020).joins,
                row(2020).resigns,
                row(2020).active_end_of_fy
            ),
            (2, 0, 2)
        );
        assert_eq!((row(2021).joins, row(2021).active_end_of_fy), (1, 3));
        assert_eq!(
            (
                row(2022).joins,
                row(2022).resigns,
                row(2022).active_end_of_fy
            ),
            (1, 2, 2)
        );
        assert_eq!(t.first().unwrap().fy, 2005);
        assert_eq!(t.last().unwrap().fy, 2023);
    }

    // ── reference implementations of the household-year views ──────────────
    // The original bodies: one linear scan of every row per member. The indexed
    // versions must reproduce them exactly.

    fn year1_reference(rows: &[HhFy], cur: i32) -> Vec<CohortYear1> {
        (FIRST_COHORT_FY..cur)
            .filter_map(|cohort| {
                let members: Vec<_> = rows
                    .iter()
                    .filter(|row| row.fy == cohort && row.joined_this_fy)
                    .collect();
                if members.is_empty() {
                    return None;
                }
                let kept = members
                    .iter()
                    .filter(|member| {
                        rows.iter().any(|row| {
                            row.account_id == member.account_id
                                && row.fy == cohort + 1
                                && row.active_end_of_fy
                        })
                    })
                    .count() as i64;
                Some(CohortYear1 {
                    cohort,
                    n: members.len() as i64,
                    pct_retained: pct(kept, members.len() as i64),
                })
            })
            .collect()
    }

    fn cohort_matrix_reference(rows: &[HhFy], cur: i32) -> Vec<CohortCell> {
        let mut out = Vec::new();
        for cohort in FIRST_COHORT_FY..cur {
            let members: Vec<_> = rows
                .iter()
                .filter(|row| row.fy == cohort && row.joined_this_fy)
                .collect();
            if members.is_empty() {
                continue;
            }
            for k in 1..=MAX_K {
                if cohort + k > cur {
                    break;
                }
                let kept = members
                    .iter()
                    .filter(|member| {
                        rows.iter().any(|row| {
                            row.account_id == member.account_id
                                && row.fy == cohort + k
                                && row.active_end_of_fy
                        })
                    })
                    .count() as i64;
                out.push(CohortCell {
                    cohort,
                    n: members.len() as i64,
                    k,
                    pct_retained: pct(kept, members.len() as i64),
                });
            }
        }
        out
    }

    fn channels_reference(rows: &[HhFy], cur: i32) -> Vec<ChannelRow> {
        let joiners: Vec<_> = rows
            .iter()
            .filter(|row| row.joined_this_fy && row.fy >= cur - 12 && row.fy <= cur - 4)
            .collect();
        let mut out: Vec<_> = CHANNELS
            .iter()
            .enumerate()
            .filter_map(|(index, (key, _))| {
                let members: Vec<_> = joiners.iter().filter(|row| row.entry_jobs[index]).collect();
                if members.len() < CHANNEL_MIN_N {
                    return None;
                }
                let outcomes: Vec<_> = members
                    .iter()
                    .map(|member| {
                        rows.iter()
                            .filter(|row| row.account_id == member.account_id)
                            .max_by_key(|row| row.fy)
                    })
                    .collect();
                let n = members.len() as i64;
                let still_members = outcomes
                    .iter()
                    .filter(|row| row.is_some_and(|row| row.active_end_of_fy))
                    .count() as i64;
                let avg_tenure = outcomes
                    .iter()
                    .filter_map(|row| row.and_then(|row| row.tenure_years))
                    .map(f64::from)
                    .sum::<f64>()
                    / n as f64;
                let left_within_2y = outcomes
                    .iter()
                    .filter(|row| {
                        row.is_some_and(|row| {
                            row.resigned_this_fy && row.tenure_years.unwrap_or(i32::MAX) <= 2
                        })
                    })
                    .count() as i64;
                Some(ChannelRow {
                    key: (*key).to_string(),
                    label: channel_label(key),
                    n,
                    still_members,
                    pct: pct(still_members, n),
                    avg_tenure: (avg_tenure * 10.0).round() / 10.0,
                    left_within_2y,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            b.pct
                .partial_cmp(&a.pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    fn kpis_reference(rows: &[HhFy], cur: i32, at_risk_count: i64) -> Kpis {
        let y1 = year1_reference(rows, cur);
        let latest = y1.iter().filter(|row| row.cohort <= cur - 2).last();
        let baseline: Vec<_> = y1.iter().filter(|row| row.cohort <= cur - 3).collect();
        let year1_baseline_pct = if baseline.is_empty() {
            0.0
        } else {
            (10.0 * baseline.iter().map(|row| row.pct_retained).sum::<f64>()
                / baseline.len() as f64)
                .round()
                / 10.0
        };
        let count = |fy: i32, predicate: fn(&HhFy) -> bool| {
            rows.iter()
                .filter(|row| row.fy == fy && predicate(row))
                .count() as i64
        };
        Kpis {
            members_now: count(cur, |row| row.active_end_of_fy),
            net_vs_prior_fy: count(cur, |row| row.active_end_of_fy)
                - count(cur - 1, |row| row.active_end_of_fy),
            joins_this_fy: count(cur, |row| row.joined_this_fy),
            resigns_this_fy: count(cur, |row| row.resigned_this_fy),
            year1_cohort: latest.map(|row| row.cohort).unwrap_or(cur - 1),
            year1_pct: latest.map(|row| row.pct_retained).unwrap_or(0.0),
            year1_baseline_pct,
            at_risk_count,
        }
    }

    /// A multi-cohort synthetic mart: 13 cohorts, four exit patterns, three entry
    /// channels (channel 0 on every household so it clears `CHANNEL_MIN_N`), plus
    /// households with no usable join date, which produce no rows at all.
    fn synthetic_household_years(cur: i32) -> Vec<HhFy> {
        let mut hh = Vec::new();
        for i in 0..90usize {
            let join = 2010 + (i % 13) as i32;
            let mut household = match i % 4 {
                0 => h(&format!("hh-{i}"), false, Some(join), Some(join + 1)),
                1 => h(&format!("hh-{i}"), false, Some(join), Some(join + 3)),
                2 => h(&format!("hh-{i}"), true, Some(join), None),
                _ => h(&format!("hh-{i}"), false, Some(join), None),
            };
            household.ch[0] = true;
            household.ch[1 + i % 3] = true;
            household.join_reason = Some("synthetic".into());
            hh.push(household);
        }
        hh.push(h("no-join-a", true, None, None));
        hh.push(h("no-join-b", false, None, None));
        household_year_rows(&hh, cur)
    }

    #[test]
    fn indexed_household_year_views_match_the_reference_scans() {
        let cur = 2026;
        let rows = synthetic_household_years(cur);
        assert!(rows.len() > 500, "dataset is non-trivial: {}", rows.len());
        let index = HouseholdYearIndex::build(&rows);
        let json = |v: &dyn ToJson| v.to_json();

        let year1 = year1_indexed(&rows, &index, cur);
        assert!(year1.len() >= 10);
        assert_eq!(json(&year1), json(&year1_reference(&rows, cur)));
        assert_eq!(
            json(&year1_from_household_years(&rows, cur)),
            json(&year1_reference(&rows, cur))
        );

        let matrix = cohort_matrix_indexed(&rows, &index, cur);
        assert!(matrix.len() > 50);
        assert_eq!(json(&matrix), json(&cohort_matrix_reference(&rows, cur)));
        assert_eq!(
            json(&cohort_matrix_from_household_years(&rows, cur)),
            json(&cohort_matrix_reference(&rows, cur))
        );

        let channels = channels_indexed(&rows, &index, cur);
        assert!(!channels.is_empty(), "channel 0 clears the minimum n");
        assert_eq!(json(&channels), json(&channels_reference(&rows, cur)));
        assert_eq!(
            json(&channels_from_household_years(&rows, cur)),
            json(&channels_reference(&rows, cur))
        );

        assert_eq!(
            json(&kpis_from_household_years(&rows, &year1, cur, 7)),
            json(&kpis_reference(&rows, cur, 7))
        );

        // Rows are keyed by account, not by position: shuffling the input order must not
        // change the indexed result any more than it changes the reference.
        let mut shuffled = rows.clone();
        shuffled.reverse();
        let index = HouseholdYearIndex::build(&shuffled);
        assert_eq!(
            json(&channels_indexed(&shuffled, &index, cur)),
            json(&channels_reference(&shuffled, cur))
        );
        assert_eq!(
            json(&cohort_matrix_indexed(&shuffled, &index, cur)),
            json(&cohort_matrix_reference(&shuffled, cur))
        );
    }

    /// Exact structural comparison for the `Serialize`-only view rows.
    trait ToJson {
        fn to_json(&self) -> serde_json::Value;
    }
    impl<T: Serialize> ToJson for T {
        fn to_json(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap()
        }
    }

    #[test]
    fn year1_and_cohort_matrix() {
        let hh = vec![
            h("a", true, Some(2020), None),
            h("b", false, Some(2020), Some(2021)), // lost in year 1
            h("c", false, Some(2020), Some(2023)), // lost in year 3
            h("d", true, Some(2021), None),
        ];
        let y = year1(&hh, 2024);
        let yearly = household_year_rows(&hh, 2024);
        assert_eq!(year1_from_household_years(&yearly, 2024).len(), y.len());
        let c2020 = y.iter().find(|r| r.cohort == 2020).unwrap();
        assert_eq!((c2020.n, c2020.pct_retained), (3, 66.7));
        let m = cohort_matrix(&hh, 2024);
        assert_eq!(
            cohort_matrix_from_household_years(&yearly, 2024).len(),
            m.len()
        );
        let cell = |c: i32, k: i32| {
            m.iter()
                .find(|x| x.cohort == c && x.k == k)
                .unwrap()
                .pct_retained
        };
        assert_eq!(cell(2020, 1), 66.7);
        assert_eq!(cell(2020, 2), 66.7);
        assert_eq!(cell(2020, 3), 33.3);
        assert_eq!(cell(2020, 4), 33.3);
        assert!(m.iter().all(|x| x.cohort + x.k <= 2024), "no future cells");
        assert!(m.iter().any(|x| x.cohort == 2021 && x.k == 3));
        assert!(!m.iter().any(|x| x.cohort == 2021 && x.k == 4));
    }

    #[test]
    fn channels_window_flags_and_threshold() {
        let mut hh = Vec::new();
        for i in 0..25 {
            let mut x = h(&format!("rs{i}"), i % 5 != 0, Some(2016), Some(2019));
            x.join_reason = Some("Religious School".into());
            x.ch = channel_flags(x.join_reason.as_deref());
            hh.push(x);
        }
        // too recent to count (joined within 3 years)
        let mut recent = h("r", true, Some(2025), None);
        recent.join_reason = Some("Religious School".into());
        recent.ch = channel_flags(recent.join_reason.as_deref());
        hh.push(recent);
        // below the 20-household threshold
        let mut c = h("c", true, Some(2016), None);
        c.join_reason = Some("Clergy".into());
        c.ch = channel_flags(c.join_reason.as_deref());
        hh.push(c);
        let ch = channels(&hh, 2026);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].key, "religious_school");
        assert_eq!((ch[0].n, ch[0].still_members, ch[0].pct), (25, 20, 80.0));
        assert_eq!(ch[0].left_within_2y, 0);
        assert!((ch[0].avg_tenure - (20.0 * 10.0 + 5.0 * 3.0) / 25.0).abs() < 0.01);
    }

    #[test]
    fn school_groups_and_reasons() {
        let mut a = h("a", true, Some(2016), None);
        a.rs_family = true;
        a.ns_family = true;
        let mut b = h("b", false, Some(2017), Some(2024));
        b.rs_family = true;
        b.resign_reason_group = "Structural Exit".into();
        let c = h("c", false, Some(2018), Some(2025));
        let mut c = c;
        c.resign_reason_group = "Administrative or Unknown Exit".into();
        let hh = vec![a, b, c.clone()];
        let s = school(&hh, 2026);
        let g = |name: &str| s.iter().find(|r| r.group == name).unwrap();
        assert_eq!(
            (
                g("Both nursery and religious school").n,
                g("Both nursery and religious school").pct
            ),
            (1, 100.0)
        );
        assert_eq!(
            (
                g("Religious school family").n,
                g("Religious school family").pct
            ),
            (1, 0.0)
        );
        assert_eq!(g("No school history").n, 1);
        let r = reasons(&hh, 2026);
        assert!(r
            .iter()
            .any(|x| x.fy == 2024 && x.reason == "Structural Exit" && x.n == 1));
        assert!(r
            .iter()
            .any(|x| x.fy == 2025 && x.reason == "Administrative or Unknown Exit" && x.n == 1));
    }

    fn with_reason(id: &str, current: bool, join: i32, resign: Option<i32>, reason: &str) -> Hh {
        let mut x = h(id, current, Some(join), resign);
        x.join_reason = Some(reason.into());
        x.ch = channel_flags(Some(reason));
        x
    }

    #[test]
    fn multi_job_buckets_by_stated_job_count() {
        let cur = 2026;
        let hh = vec![
            with_reason("one", true, 2016, None, "Religious School"),
            with_reason("two", true, 2016, None, "Community and Young Professionals"),
            with_reason(
                "three",
                false,
                2016,
                Some(2020),
                "Religious School, Nursery School and Community",
            ),
        ];
        let rows = multi_job(&hh, cur);
        let get = |bucket: &str| rows.iter().find(|r| r.bucket == bucket).unwrap();
        assert_eq!((get("1 job").n, get("1 job").still_members), (1, 1));
        assert_eq!((get("2 jobs").n, get("2 jobs").still_members), (1, 1));
        // "three" stated three recognized jobs and has since resigned.
        assert_eq!(
            (
                get("3+ jobs").n,
                get("3+ jobs").still_members,
                get("3+ jobs").pct
            ),
            (1, 0, 0.0)
        );
        assert_eq!(get("1 job").avg_tenure, 10.0, "current, joined 2016");
    }

    #[test]
    fn outcome_by_tenure_buckets_exits_by_tenure_at_exit() {
        let mut a = h("a", false, Some(2018), Some(2019)); // 1 year tenure
        a.exit_reason = "No longer engaged".into();
        let mut b = h("b", false, Some(2010), Some(2018)); // 8 years
        b.exit_reason = "Moved".into();
        let mut c = h("c", false, Some(2000), Some(2020)); // 20 years
        c.exit_reason = "Displeased".into();
        let rows = outcome_by_tenure(&[a, b, c], 2026);
        let has = |bucket: &str, outcome: &str| {
            rows.iter()
                .any(|r| r.tenure_bucket == bucket && r.outcome == outcome && r.n == 1)
        };
        assert!(has("1-2y", "No longer engaged"));
        assert!(has("6-10y", "Moved"));
        assert!(has("11+y", "Displeased"));
    }

    #[test]
    fn school_progression_splits_nursery_families_and_ignores_non_nursery() {
        let mut progressed = h("p", true, Some(2016), None);
        progressed.ns_family = true;
        progressed.rs_family = true;
        let mut only = h("o", false, Some(2016), Some(2020));
        only.ns_family = true;
        let mut not_nursery = h("n", true, Some(2016), None);
        not_nursery.rs_family = true; // no nursery history -> excluded
        let rows = school_progression(&[progressed, only, not_nursery], 2026);
        let g = |name: &str| rows.iter().find(|r| r.group == name).unwrap();
        assert_eq!(
            (
                g("Nursery → Religious school").n,
                g("Nursery → Religious school").still_members,
                g("Nursery → Religious school").pct
            ),
            (1, 1, 100.0)
        );
        assert_eq!(
            (g("Nursery school only").n, g("Nursery school only").pct),
            (1, 0.0)
        );
    }

    #[test]
    fn school_gap_buckets_years_since_religious_school() {
        let ended = |id: &str, current: bool, last: i32| {
            let mut x = h(
                id,
                current,
                Some(2010),
                if current { None } else { Some(2024) },
            );
            x.rs_family = true;
            x.active_rs_students = 0;
            x.last_rs_year = Some(last);
            x
        };
        let mut still_enrolled = ended("busy", true, 2025);
        still_enrolled.active_rs_students = 2; // school not over -> excluded
        let rows = school_gap(
            &[
                ended("a", true, 2025),  // gap 1
                ended("b", false, 2023), // gap 3
                ended("c", true, 2016),  // gap 10
                still_enrolled,
            ],
            2026,
        );
        let g = |bucket: &str| rows.iter().find(|r| r.bucket == bucket).unwrap();
        assert_eq!((g("0-1y").n, g("0-1y").still_members), (1, 1));
        assert_eq!((g("2-3y").n, g("2-3y").still_members), (1, 0));
        assert_eq!((g("7+y").n, g("7+y").still_members), (1, 1));
        assert!(
            !rows.iter().any(|r| r.bucket == "4-6y"),
            "no household there"
        );
    }

    #[test]
    fn kpis_summarize_the_latest_year() {
        // a: current, joined 2010. b: current, joined 2024. c: resigned, joined 2024,
        // resigned FY2025. d: resigned, joined 2011, resigned FY2026 (this FY, in progress).
        // e: current, joined 2025 (the in-progress FY's cohort).
        // "e" joined in the in-progress year: the old y1.last() rule would report (2025, 100.0).
        let hh = vec![
            h("a", true, Some(2010), None),
            h("b", true, Some(2024), None),
            h("c", false, Some(2024), Some(2025)),
            h("d", false, Some(2011), Some(2026)),
            h("e", true, Some(2025), None),
        ];
        let k = kpis(&hh, 2026, 7);
        assert_eq!(k.members_now, 3, "a, b, e are current");
        assert_eq!(k.joins_this_fy, 0);
        assert_eq!(k.resigns_this_fy, 1, "only d resigned in FY2026");
        assert_eq!(
            k.net_vs_prior_fy, -1,
            "active 2026={{a,b,e}}=3, active 2025={{a,b,d,e}}=4"
        );
        assert_eq!(
            (k.year1_cohort, k.year1_pct),
            (2024, 50.0),
            "the FY2026 cohort (e, still mid-first-year) is excluded; latest complete cohort is 2024"
        );
        assert_eq!(k.at_risk_count, 7);
    }

    #[test]
    fn views_end_to_end_from_the_mart() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        let v = views(&s, 2026).unwrap();
        assert_eq!(v.current_fy, 2026);
        assert!(v.built_at.is_some());
        // 4 current members, but Katz (001E) has a placeholder join date and no valid
        // join FY, so the household-year mart cannot place it on the fiscal timeline.
        assert_eq!(v.kpis.members_now, 3);
        assert!(!v.trend.is_empty());
        assert!(v.year1.iter().any(|r| r.cohort == 2015 && r.n == 2));
    }

    #[test]
    fn at_risk_rules_fire_with_reasons() {
        let cur = 2026;
        let mut ns_only = h("ns", true, Some(2025), None);
        ns_only.name = Some("NS Only".into());
        ns_only.ch = channel_flags(Some("Nursery School"));
        let mut intro = h("yp", true, Some(2023), None);
        intro.name = Some("Young Pro".into());
        intro.tier = Some("Young Professionals".into());
        let mut rs_done = h("rs", true, Some(2012), None);
        rs_done.name = Some("RS Done".into());
        rs_done.rs_family = true;
        rs_done.active_rs_students = 0;
        rs_done.last_rs_year = Some(2025);
        let mut old_rs_done = h("old-rs", true, Some(2012), None);
        old_rs_done.name = Some("Old RS Done".into());
        old_rs_done.rs_family = true;
        old_rs_done.active_rs_students = 0;
        old_rs_done.last_rs_year = Some(2016);
        let mut safe = h("ok", true, Some(2012), None);
        safe.name = Some("Safe".into());
        let mut gone = h("gone", false, Some(2025), Some(2026));
        gone.name = Some("Gone".into());
        let rows = at_risk_rows(&[ns_only, intro, rs_done, old_rs_done, safe, gone], cur);
        let get = |id: &str| {
            rows.iter()
                .find(|r| r.account_id == id)
                .map(|r| r.rules.clone())
        };
        assert_eq!(
            get("ns"),
            Some(vec!["first_year".to_string(), "new_ns_only".to_string()])
        );
        assert_eq!(get("yp"), None, "one weak churn signal is not enough");
        assert_eq!(get("rs"), None, "one weak churn signal is not enough");
        assert_eq!(
            get("old-rs"),
            None,
            "old school-end history is not current risk"
        );
        assert_eq!(get("ok"), None);
        assert_eq!(get("gone"), None, "only current members can be at risk");
        assert_eq!(rows[0].account_id, "ns", "most rules first");
    }

    #[test]
    fn csv_renders_every_view_with_a_header() {
        let (_d, mut s) = mem();
        seed_account(&mut s, &fixture(), &ACCT_COLS);
        rebuild(&mut s).unwrap();
        let ins = views(&s, 2026).unwrap();
        let ar = at_risk(&s, 2026).unwrap();
        for v in VIEWS {
            let (text, n) = to_csv(v, &ins, &ar).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            assert!(lines.len() >= 1, "{v} has a header");
            assert_eq!(lines.len() - 1, n, "{v} row count matches");
        }
        let (t, _) = to_csv("trend", &ins, &ar).unwrap();
        assert!(t.starts_with("fy,joins,resigns,active_end_of_fy\n"));
        assert!(to_csv("nope", &ins, &ar).is_err());
        let (t, n) = to_csv("at_risk", &ins, &ar).unwrap();
        assert!(
            n >= 1 && t.contains("Green"),
            "NS-only recent joiner is at risk"
        );
    }

    #[test]
    fn export_path_guard_only_accepts_files_inside_the_exports_dir() {
        let dir = tempfile::tempdir().unwrap();
        let exports = dir.path().join("exports");
        std::fs::create_dir_all(&exports).unwrap();
        let ok = exports.join("insights-trend-20260825-1200.csv");
        std::fs::write(&ok, "x").unwrap();
        assert!(path_is_inside(&ok, &exports));
        let outside = dir.path().join("mirror.db");
        std::fs::write(&outside, "x").unwrap();
        assert!(!path_is_inside(&outside, &exports));
        let sneaky = exports.join("..").join("mirror.db");
        assert!(!path_is_inside(&sneaky, &exports));
        assert!(
            !path_is_inside(&exports.join("missing.csv"), &exports),
            "must exist"
        );
    }
}

pub fn reasons_from_household_years(rows: &[HhFy], cur: i32) -> Vec<ReasonCell> {
    let mut counts: std::collections::BTreeMap<(i32, String), i64> = Default::default();
    for row in rows
        .iter()
        .filter(|row| row.resigned_this_fy && row.fy >= cur - 5 && row.fy <= cur)
    {
        *counts
            .entry((
                row.fy,
                row.exit_reason
                    .clone()
                    .unwrap_or_else(|| OTHER_EXIT.into()),
            ))
            .or_default() += 1;
    }
    counts
        .into_iter()
        .map(|((fy, reason), n)| ReasonCell { fy, reason, n })
        .collect()
}

pub fn load_household_years(store: &Store) -> Result<Vec<HhFy>> {
    let flag_cols = CHANNELS
        .iter()
        .map(|(key, _)| format!("entry_job_{key}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut st = store.conn().prepare(&format!(
        "SELECT account_id, fy, active_end_of_fy, joined_this_fy, resigned_this_fy,
         tenure_years, exit_reason, entry_job_count, {flag_cols},
         anchor_dues, anchor_nursery, anchor_religious, anchor_committee,
         dues_coverage_missing, dues_settlement,
         renewal_observed, school_observed, committee_observed
         FROM _m_household_fy ORDER BY account_id, fy"
    ))?;
    let rows = st.query_map([], |row| {
        let mut entry_jobs = [false; 12];
        for (index, job) in entry_jobs.iter_mut().enumerate() {
            *job = row.get::<_, i64>(8 + index)? != 0;
        }
        let anchor = 8 + entry_jobs.len(); // first anchor column index
        Ok(HhFy {
            account_id: row.get(0)?,
            fy: row.get(1)?,
            active_end_of_fy: row.get::<_, i64>(2)? != 0,
            joined_this_fy: row.get::<_, i64>(3)? != 0,
            resigned_this_fy: row.get::<_, i64>(4)? != 0,
            tenure_years: row.get(5)?,
            exit_reason: row.get(6)?,
            entry_job_count: row.get(7)?,
            entry_jobs,
            anchor_dues: row.get::<_, i64>(anchor)? != 0,
            anchor_nursery: row.get::<_, i64>(anchor + 1)? != 0,
            anchor_religious: row.get::<_, i64>(anchor + 2)? != 0,
            anchor_committee: row.get::<_, i64>(anchor + 3)? != 0,
            dues_coverage_missing: row.get::<_, i64>(anchor + 4)? != 0,
            dues_settlement: row.get(anchor + 5)?,
            renewal_observed: row.get::<_, i64>(anchor + 6)? != 0,
            school_observed: row.get::<_, i64>(anchor + 7)? != 0,
            committee_observed: row.get::<_, i64>(anchor + 8)? != 0,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
