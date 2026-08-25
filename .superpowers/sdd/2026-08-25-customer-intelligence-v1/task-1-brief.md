### Task 1: Scaffold the Tauri project and copy the design system

**Files:**
- Create: everything `create-tauri-app` generates (`package.json`, `index.html`, `vite.config.ts`, `tsconfig.json`, `src/`, `src-tauri/`)
- Create: `.env.example`, `.env`
- Create: `src/design-system/**` (copied), `src/assets/emanuel_logo.png` (copied)
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `tsconfig.json`, `src/design-system/ui-kits/grant-management/chrome.jsx` (one line)

**Interfaces:**
- Produces: a project where `npm run tauri dev` opens a window; `import { Button } from './design-system'` works from TS.

- [ ] **Step 1: Scaffold into the existing folder**

Run from the workspace parent (`fullstackfang/`):

```bash
npx --yes create-tauri-app@latest emanuel-customer-intelligence --manager npm --template react-ts --identifier org.emanuelnyc.customerintelligence --yes --force
cd emanuel-customer-intelligence
npm install
npm install react@19 react-dom@19 lucide-react
npm install -D @types/react@19 @types/react-dom@19 vitest
```

Expected: `src-tauri/` with `Cargo.toml`, `tauri.conf.json`, `capabilities/default.json`, `src/main.rs`, `src/lib.rs`, `icons/`; `src/` with `App.tsx`, `main.tsx`, `vite-env.d.ts`. The `--force` flag is needed because the folder already contains `.git`, `.gitignore`, and `docs/`. If the generator overwrote `.gitignore`, re-add the lines `.env`, `.env.*`, `!.env.example`, `src-tauri/target/`, `src-tauri/gen/schemas/`.

- [ ] **Step 2: Baseline dev run**

Run: `npm run tauri dev`
Expected: a window titled "emanuel-customer-intelligence" with the template greeting. First Rust compile takes several minutes. Close the window (Ctrl+C in the terminal).

- [ ] **Step 3: Env files**

Create `.env.example`:

```
# Salesforce External Client App — Consumer Key (public client id, no secret)
SF_CLIENT_ID=
# Org My Domain login URL
SF_LOGIN_URL=https://emanu-el.my.salesforce.com
```

Create `.env` with the same two lines, `SF_CLIENT_ID=` set to the Consumer Key the user provided in the conversation. Verify `git status` does NOT list `.env`.

- [ ] **Step 4: Copy the design system and logo**

```bash
cp -r ../emanuel-grant-management-app/src/design-system src/design-system
mkdir -p src/assets && cp ../emanuel-grant-management-app/src/assets/emanuel_logo.png src/assets/
```

Then edit ONE line in `src/design-system/ui-kits/grant-management/chrome.jsx`: the eyebrow under "Temple Emanu-El" in the header reads `Philanthropic Fund`; change that text to `Customer Intelligence`. Nothing else changes.

- [ ] **Step 5: tsconfig for JSX design system**

In `tsconfig.json`, inside `compilerOptions`, add `"allowJs": true` and make sure `"jsx": "react-jsx"` is present. Confirm `src/vite-env.d.ts` contains `/// <reference types="vite/client" />` (needed for the `.png` import).

- [ ] **Step 6: Tauri config and capability**

Edit `src-tauri/tauri.conf.json`:
- `productName`: `Emanuel Customer Intelligence`
- `identifier`: `org.emanuelnyc.customerintelligence`
- `app.windows[0]`: `"title": "Emanuel Customer Intelligence", "width": 1280, "height": 840, "minWidth": 1024, "minHeight": 700`

Replace `src-tauri/capabilities/default.json` with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Least privilege: the webview may only call the app's own commands. No fs, shell, http, or opener access from JS — all egress happens in Rust.",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

- [ ] **Step 7: Prove the design system renders**

Replace `src/App.tsx` with:

```tsx
import "./design-system/styles.css";
import { Button, Card, CardHeader, CardTitle } from "./design-system";

export default function App() {
  return (
    <div style={{ padding: "var(--space-8)" }}>
      <Card>
        <CardHeader><CardTitle>Design system smoke</CardTitle></CardHeader>
        <Button onClick={() => alert("ok")}>Primary Button</Button>
      </Card>
    </div>
  );
}
```

Delete `src/App.css` and any `import "./App.css"` line if the template created one. Run `npm run tauri dev`. Expected: a white card with a sapphire "Primary Button" in DM Sans on a warm off-white background. Run `npx tsc --noEmit` — expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add -A
git status   # confirm .env is NOT staged
git commit -m "chore: scaffold Tauri v2 app with Emanuel design system"
```

---
