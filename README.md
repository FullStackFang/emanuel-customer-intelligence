# Emanuel Customer Intelligence

Desktop app (Tauri v2, Rust + React) that mirrors a governed, user-selected subset of
Temple Emanu-El's Salesforce data into an encrypted local database, profiles it, and
lets staff build simple segments. Read-only against Salesforce. No server, no cloud copy.

Original v1 design: `docs/superpowers/specs/2026-08-25-customer-intelligence-v1-design.md`

## Specifications

OpenSpec is the authoritative workflow for new features, behavior changes, and architectural work.
Active changes live under `openspec/changes/`; accepted capability specifications live under
`openspec/specs/`. Run the repository-local CLI with `npm exec -- openspec <command>`.

## Setup
1. Salesforce admin: External Client App with callback `http://localhost:1717/callback`,
   scopes `api refresh_token openid id profile email`, PKCE required, **Require Secret
   for Web Server Flow and Refresh Token Flow both OFF**.
2. Copy `.env.example` to `.env`; set `SF_CLIENT_ID` to the Consumer Key and
   `SF_LOGIN_URL` to the org's My Domain URL.
3. `npm install` then `npm run tauri dev`.

## Governance model
- The webview can only call named Rust commands (`src-tauri/src/commands.rs`).
- Scan copies metadata only. Rows are mirrored only for objects you select, and only for
  fields not withheld. Fields that look sensitive are withheld by default; overriding
  one is recorded in the audit log.
- Tokens and the database key live in Windows Credential Manager. The mirror
  (`%APPDATA%\org.emanuelnyc.customerintelligence\mirror.db`) is SQLCipher-encrypted.
- `_audit` is append-only: there is no code path that edits or deletes it.
- SQLCipher's `cipher_memory_security` in-memory hardening is disabled (off-by-default in
  SQLCipher 4.x and crashes on Windows in this setup), an accepted residual risk that
  decrypted pages or the key could in principle reach the Windows pagefile, mitigated by
  full-disk encryption.

## Insights
The Insights page is computed from the local mirror only (never Salesforce). After each
sync + profile, a household mart (`_m_household`) is rebuilt from the synced, non-withheld
Account columns; the page reads that table. Fiscal years run June 1 – May 31 and are labeled
by the year they end. Viewing the at-risk list and exporting CSVs are recorded in the audit log;
exports land in `%APPDATA%\org.emanuelnyc.customerintelligence\exports\`.
Exported CSVs are plain files outside the encrypted mirror (the at-risk export contains
household names); Purge local data deletes them along with the mirror.

## Verify
`npm run typecheck && npm test && (cd src-tauri && cargo test)`

## Windows build notes
- The `bundled-sqlcipher-vendored-openssl` feature compiles vendored OpenSSL, which
  requires a full Perl (e.g. Strawberry Perl) on PATH — the minimal MSYS/Git Perl lacks
  modules the OpenSSL `Configure` script needs. If it's not already first on PATH, point
  the build at it explicitly: `OPENSSL_SRC_PERL=C:/Strawberry/perl/bin/perl.exe`.
- If the project lives under a deeply-nested path (e.g. inside OneDrive), the vendored
  OpenSSL build can exceed Windows' 260-character path limit. Set `CARGO_TARGET_DIR` to
  a short path (e.g. `C:\ct`) to avoid it.
