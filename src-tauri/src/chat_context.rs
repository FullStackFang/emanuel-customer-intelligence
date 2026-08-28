//! The governance boundary for chat: a de-identified aggregate snapshot that is the ONLY
//! data any chat backend ever receives. It is assembled from the existing PII-free Insights
//! aggregates (`insights::views`) plus the CONTEXT.md data dictionary — never from households,
//! per-household financials, at-risk rows, the Watch List, or Segment member lists.
//!
//! Two guarantees, enforced here and proven by the tests below:
//!   1. Allow-list, not deny-list. `build` reads only `insights::views` (aggregate totals and
//!      group summaries) and the packaged data dictionary. No PII-bearing type (`Hh`, `HhFy`,
//!      `AtRiskRow`, Watch List, segment members) is referenced.
//!   2. k-anonymity floor N = 5. Any group summarizing fewer than five Membership Households is
//!      dropped before it enters the snapshot, and the snapshot says so, so the model never
//!      treats the published totals as exhaustive.

use crate::insights::{self, Insights};
use crate::store::Store;
use anyhow::Result;

/// The k-anonymity floor: a group summarizing fewer than this many Membership Households is
/// omitted from the snapshot.
pub const K_ANON_FLOOR: i64 = 5;

/// The packaged data dictionary — the canonical membership vocabulary the model must use. Bundled
/// at compile time so the snapshot never reads a file at runtime and the dictionary can never
/// drift from the app's own definitions.
const DATA_DICTIONARY: &str = include_str!("../../CONTEXT.md");

/// A de-identified aggregate snapshot — the sole model input for a chat turn.
#[derive(Debug, Clone)]
pub struct GovernedSnapshot {
    pub text: String,
}

impl GovernedSnapshot {
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Build the governed snapshot for one fiscal year from the sanctioned aggregate sources only.
///
/// The single choke point: it reads `insights::views` (the same PII-free aggregates the dashboard
/// shows) and the packaged data dictionary, applies the k-anonymity floor, and renders a bounded
/// text summary. It never loads households, at-risk rows, the Watch List, or segment members.
pub fn build(store: &Store, cur: i32) -> Result<GovernedSnapshot> {
    let insights = insights::views(store, cur)?;
    Ok(GovernedSnapshot {
        text: render(&insights, DATA_DICTIONARY),
    })
}

/// True when a group of `n` households clears the k-anonymity floor and may appear.
fn passes_floor(n: i64) -> bool {
    n >= K_ANON_FLOOR
}

/// Render the aggregate snapshot text from a PII-free `Insights` value and the data dictionary.
/// Pure and deterministic — the whole render surface is visible here, which is what makes the leak
/// test a real guarantee: what this function does not put in the string cannot reach a model.
///
/// Every per-group line is gated on `passes_floor`; institution-wide totals (KPIs, per-year
/// membership and money totals) are not groups and are kept.
fn render(ins: &Insights, dictionary: &str) -> String {
    let mut s = String::with_capacity(8 * 1024);

    s.push_str("# Temple Emanu-El — Membership Intelligence (governed aggregate snapshot)\n\n");
    s.push_str(
        "You are a data assistant for congregation staff. Answer questions about the membership \
         data using ONLY the aggregates below and the vocabulary in the data dictionary. These are \
         de-identified totals and group summaries — there is no individual household, name, email, \
         address, or identifier here, and you must never invent one. Groups smaller than five \
         Membership Households are omitted, so treat every total as possibly excluding small \
         groups; never claim a figure is exhaustive, and say when the data does not answer a \
         question.\n\n",
    );

    // ── Data dictionary ──────────────────────────────────────────────────────
    s.push_str("## Data dictionary\n\n");
    s.push_str(dictionary.trim());
    s.push_str("\n\n");

    // ── As-of ────────────────────────────────────────────────────────────────
    s.push_str("## As of\n\n");
    s.push_str(&format!("- Current fiscal year: FY{}\n", ins.current_fy));
    if let Some(built) = &ins.built_at {
        s.push_str(&format!("- Aggregates built at: {built}\n"));
    }
    if !ins.unavailable.is_empty() {
        s.push_str(&format!(
            "- Views without a synced source (no data): {}\n",
            ins.unavailable.join(", ")
        ));
    }
    s.push('\n');

    // ── KPIs (institution totals) ────────────────────────────────────────────
    let k = &ins.kpis;
    s.push_str("## Key figures (current members)\n\n");
    s.push_str(&format!("- Members now: {}\n", k.members_now));
    s.push_str(&format!(
        "- Net change vs prior fiscal year: {:+}\n",
        k.net_vs_prior_fy
    ));
    s.push_str(&format!("- Joins this fiscal year: {}\n", k.joins_this_fy));
    s.push_str(&format!("- Resignations this fiscal year: {}\n", k.resigns_this_fy));
    s.push_str(&format!(
        "- Newest full cohort (FY{}) one-year retention: {:.1}% (baseline {:.1}%)\n",
        k.year1_cohort, k.year1_pct, k.year1_baseline_pct
    ));
    s.push_str(&format!(
        "- Households flagged with current risk evidence: {}\n\n",
        k.at_risk_count
    ));

    // ── Membership trend (institution totals per year) ───────────────────────
    if !ins.trend.is_empty() {
        s.push_str("## Membership by fiscal year (joins, resignations, active at year end)\n\n");
        for r in &ins.trend {
            s.push_str(&format!(
                "- FY{}: joins {}, resignations {}, active at end {}\n",
                r.fy, r.joins, r.resigns, r.active_end_of_fy
            ));
        }
        s.push('\n');
    }

    // ── Cohort year-1 retention (per-group) ──────────────────────────────────
    section_group(
        &mut s,
        "One-year retention by join cohort",
        ins.year1.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- FY{} cohort: {} households, {:.1}% retained a year later", r.cohort, r.n, r.pct_retained),
    );

    // ── Cohort makeup of today's base (per-group) ────────────────────────────
    section_group(
        &mut s,
        "Make-up of today's member base by join cohort",
        ins.cohort_makeup.iter().filter(|r| passes_floor(r.current)),
        |r| format!("- FY{} joiners: {} current members ({:.1}% of the base)", r.cohort, r.current, r.pct_of_base),
    );

    // ── Membership-age bands (per-group) ─────────────────────────────────────
    section_group(
        &mut s,
        "Current members by membership age",
        ins.membership_age.iter().filter(|r| passes_floor(r.households)),
        |r| format!("- {}: {} households ({:.1}% of the dated base)", r.band, r.households, r.pct_of_base),
    );

    // ── Entry-job channels retention (per-group) ─────────────────────────────
    section_group(
        &mut s,
        "Retention by Entry Job (stated join reason)",
        ins.channels.iter().filter(|r| passes_floor(r.n)),
        |r| format!(
            "- {}: {} households, {:.1}% still members, avg tenure {:.1} yrs, {} left within 2 yrs",
            r.label, r.n, r.pct, r.avg_tenure, r.left_within_2y
        ),
    );

    // ── Multi-Entry-Job retention (per-group) ────────────────────────────────
    section_group(
        &mut s,
        "Retention by number of stated Entry Jobs",
        ins.multi_job.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- {}: {} households, {:.1}% still members, avg tenure {:.1} yrs", r.bucket, r.n, r.pct, r.avg_tenure),
    );

    // ── Schools (per-group) ──────────────────────────────────────────────────
    section_group(
        &mut s,
        "Retention by religious-school relationship",
        ins.school.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- {}: {} households, {:.1}% still members", r.group, r.n, r.pct),
    );
    section_group(
        &mut s,
        "Retention by fiscal years since religious school ended",
        ins.school_gap.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- {}: {} households, {:.1}% still members", r.bucket, r.n, r.pct),
    );

    // ── Exit outcomes by tenure (per-group) ──────────────────────────────────
    section_group(
        &mut s,
        "Exit Outcomes by tenure at exit",
        ins.outcome_by_tenure.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- {} / {}: {} spells", r.tenure_bucket, r.outcome, r.n),
    );

    // ── Resignation reasons by year (per-group) ──────────────────────────────
    section_group(
        &mut s,
        "Resignation reason groups by fiscal year",
        ins.reasons.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- FY{} {}: {}", r.fy, r.reason, r.n),
    );

    // ── Relationship anchors (per-group) ─────────────────────────────────────
    section_group(
        &mut s,
        "Retention by Relationship Anchor type",
        ins.anchor_type.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- {}: {} households, {:.1}% still members", r.label, r.n, r.pct),
    );
    section_group(
        &mut s,
        "Retention by number of Relationship Anchors",
        ins.anchor_count.iter().filter(|r| passes_floor(r.n)),
        |r| format!("- {}: {} households, {:.1}% still members", r.label, r.n, r.pct),
    );

    // ── Dues (institution counts per year) ───────────────────────────────────
    if !ins.dues.is_empty() {
        s.push_str("## Dues (Renewal Evidence) by fiscal year\n\n");
        for r in &ins.dues {
            s.push_str(&format!(
                "- FY{}: {} active, {} billed, {} with no dues line, {} settled, {} partial, {} unsettled\n",
                r.fy, r.active, r.billed, r.coverage_missing, r.settled, r.partially_settled, r.unsettled
            ));
        }
        s.push('\n');
    }

    // ── Financials (aggregate only) ──────────────────────────────────────────
    if let Some(f) = &ins.financials {
        s.push_str("## Institutional money (aggregate only)\n\n");
        s.push_str(&format!(
            "- Latest complete fiscal year FY{}: {} members, {} paying, ${:.0} billed, ${:.0} received\n",
            f.fiscal_year, f.households, f.paying_households, f.total_billed, f.total_received
        ));
        for r in &f.by_year {
            s.push_str(&format!(
                "- FY{}{}: ${:.0} billed, ${:.0} received\n",
                r.fy,
                if r.complete { "" } else { " (in progress)" },
                r.billed,
                r.received
            ));
        }
        section_group(
            &mut s,
            "Value by membership-age band (latest complete year)",
            f.by_membership_age.iter().filter(|r| passes_floor(r.households)),
            |r| format!(
                "- {}: {} households ({:.1}% of members), ${:.0} received ({:.1}% of money)",
                r.band, r.households, r.share_of_households, r.received, r.share_of_received
            ),
        );
        section_group(
            &mut s,
            "Revenue concentration by decile (top decile first)",
            f.concentration.iter().filter(|r| passes_floor(r.households)),
            |r| format!(
                "- Decile {}: {} households, {:.1}% of received ({:.1}% cumulative)",
                r.decile, r.households, r.received_share, r.cumulative_received_share
            ),
        );
    }

    // ── Geography (already server-side suppressed; floor on n as a second guard) ──
    if let Some(g) = &ins.geography {
        if g.available && !g.cells.is_empty() {
            let mut cells: Vec<_> = g.cells.iter().filter(|c| passes_floor(c.n)).collect();
            cells.sort_by(|a, b| b.n.cmp(&a.n));
            if !cells.is_empty() {
                s.push_str(&format!(
                    "## Where members are (FY{}, by neighborhood, largest first)\n\n",
                    g.fiscal_year
                ));
                for c in cells.iter().take(15) {
                    // For neighborhood geography the `zip` slot carries the public neighborhood name.
                    s.push_str(&format!("- {}: {} households\n", c.zip, c.n));
                }
                if g.out_of_area > 0 {
                    s.push_str(&format!("- Outside the mapped area: {} households\n", g.out_of_area));
                }
                s.push('\n');
            }
        }
    }

    s.push_str(
        "\n(End of snapshot. If a question needs data not shown above, say it is not available in \
         this snapshot rather than guessing.)\n",
    );

    s
}

/// Render a titled section from an iterator of already-floor-filtered rows. Emits nothing when
/// the iterator is empty, so a section that is entirely sub-floor or absent leaves no heading.
fn section_group<T>(
    out: &mut String,
    title: &str,
    rows: impl Iterator<Item = T>,
    line: impl Fn(&T) -> String,
) {
    let lines: Vec<String> = rows.map(|r| line(&r)).collect();
    if lines.is_empty() {
        return;
    }
    out.push_str("## ");
    out.push_str(title);
    out.push_str("\n\n");
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::{rebuild, ChannelRow};
    use crate::salesforce::Row;
    use crate::store::{self, Store};

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn mem() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = store::open(&dir.path().join("t.db"), KEY).unwrap();
        (dir, s)
    }

    /// Column order for the synthetic Account mirror, including PII columns (name, email, street,
    /// postal) that the mart never reads — so the snapshot proving it carries none of them is the
    /// guarantee, not an accident of what was seeded.
    const COLS: [&str; 19] = [
        "Id",
        "Name",
        "PersonEmail",
        "BillingStreet",
        "BillingPostalCode",
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

    fn acct(vals: [&str; 19]) -> Row {
        let mut m = Row::new();
        for (c, v) in COLS.iter().zip(vals.iter()) {
            if !v.is_empty() {
                m.insert((*c).into(), serde_json::Value::String((*v).into()));
            }
        }
        m
    }

    fn seed(s: &mut Store, rows: &[Row]) {
        s.upsert_object("Account", "Account", rows.len() as i64).unwrap();
        for c in COLS {
            s.upsert_field("Account", c, "string", c, false).unwrap();
        }
        let cols: Vec<String> = COLS.iter().map(|c| c.to_string()).collect();
        s.replace_mirror("Account", &cols, rows).unwrap();
    }

    /// The distinctive PII strings seeded into the raw Account mirror. None may appear in a snapshot.
    const SECRET_NAME: &str = "Zzytolski-Quibbleworth";
    const SECRET_EMAIL: &str = "secret.person@example.com";
    const SECRET_STREET: &str = "1234 Undisclosed Terrace";
    const SECRET_ID: &str = "001SECRETHH0001";

    /// Representative data: current members, resignations, a distinctive "at-risk-shaped" household
    /// (a current member whose religious school recently ended), plus PII in name/email/street/id.
    fn representative() -> Vec<Row> {
        let mut rows = Vec::new();
        // Eight plain current members joined FY2015 via religious school (an at-floor group).
        for i in 0..8 {
            rows.push(acct([
                &format!("001A{i:03}"),
                "Cohen",
                "",
                "",
                "10024",
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
                "1",
                "0",
                "false",
                "2018-2019",
            ]));
        }
        // Six resignations in FY2020 for non-payment (an at-floor group).
        for i in 0..6 {
            rows.push(acct([
                &format!("001B{i:03}"),
                "Levy",
                "",
                "",
                "10025",
                "Member Family",
                "false",
                "true",
                "2014-08-15",
                "2014-08-15",
                "2019-08-01",
                "Voting Member",
                "MAIN",
                "Religious School",
                "Non-payment",
                "1",
                "0",
                "false",
                "2018-2019",
            ]));
        }
        // The distinctive at-risk-shaped household, carrying all four kinds of PII.
        rows.push(acct([
            SECRET_ID,
            SECRET_NAME,
            SECRET_EMAIL,
            SECRET_STREET,
            "10028",
            "Member Family",
            "true",
            "false",
            "2019-09-01",
            "2019-09-01",
            "",
            "Voting Member",
            "MAIN",
            "Religious School",
            "",
            "1",
            "0",
            "false",
            "2022-2023",
        ]));
        rows
    }

    // ── 1.1 the leak test (written first): a built snapshot carries no PII ────
    #[test]
    fn snapshot_carries_no_household_name_email_address_or_identifier() {
        let (_d, mut s) = mem();
        seed(&mut s, &representative());
        rebuild(&mut s).unwrap();

        let snap = build(&s, 2027).unwrap();
        let text = snap.text();

        assert!(!text.contains(SECRET_NAME), "household name must never appear");
        assert!(!text.contains(SECRET_EMAIL), "email must never appear");
        assert!(!text.contains(SECRET_STREET), "postal address must never appear");
        assert!(!text.contains(SECRET_ID), "household identifier must never appear");
        // Seeded household surnames must not appear either — no name reaches the aggregates.
        assert!(!text.contains("Cohen") && !text.contains("Levy"));
        // No email pattern at all: no '@' bounded by word characters.
        assert!(
            !text.contains('@'),
            "no email-shaped token may appear in the snapshot"
        );
        // The snapshot must still be substantive (it summarized real aggregates).
        assert!(text.contains("Members now:"));
        assert!(text.contains("Data dictionary"));
        assert!(
            text.contains("smaller than five") || text.contains("small groups"),
            "must disclose that small groups are omitted"
        );
    }

    // ── 1.4 k-anon floor: sub-floor dropped, at-floor retained ───────────────
    #[test]
    fn k_anon_floor_drops_sub_floor_group_and_keeps_at_floor() {
        let (_d, mut s) = mem();
        seed(&mut s, &representative());
        rebuild(&mut s).unwrap();
        let mut ins = insights::views(&s, 2027).unwrap();

        // Two synthetic channel rows: one just under the floor, one exactly at it.
        ins.channels = vec![
            ChannelRow {
                key: "under".into(),
                label: "UnderFloorChannel".into(),
                n: K_ANON_FLOOR - 1,
                still_members: 1,
                pct: 25.0,
                avg_tenure: 3.0,
                left_within_2y: 0,
            },
            ChannelRow {
                key: "at".into(),
                label: "AtFloorChannel".into(),
                n: K_ANON_FLOOR,
                still_members: 3,
                pct: 60.0,
                avg_tenure: 4.0,
                left_within_2y: 1,
            },
        ];

        let text = render(&ins, "dictionary");
        assert!(
            !text.contains("UnderFloorChannel"),
            "a group below the floor must be dropped"
        );
        assert!(
            text.contains("AtFloorChannel"),
            "a group at the floor must be retained"
        );
    }

    #[test]
    fn passes_floor_boundary() {
        assert!(!passes_floor(0));
        assert!(!passes_floor(4));
        assert!(passes_floor(5));
        assert!(passes_floor(6));
    }

    // ── 3.4 chat history is independent of Insights rebuilds ─────────────────
    #[test]
    fn chat_history_survives_an_insights_rebuild() {
        let (_d, mut s) = mem();
        seed(&mut s, &representative());
        rebuild(&mut s).unwrap();
        s.create_conversation("c1", "ollama", "Kept across rebuild").unwrap();
        s.append_chat_message("m1", "c1", "user", "who joined most?").unwrap();

        // A forced rebuild churns the mart and the Insights cache; chat tables are untouched.
        rebuild(&mut s).unwrap();

        assert_eq!(s.list_conversations().unwrap().len(), 1);
        assert_eq!(s.list_chat_messages("c1").unwrap().len(), 1);
    }
}
