# Safely

Safely is a Chrome extension (plus a small backend and dashboard) that warns people about fraud *before* they send money to a scammer on an online marketplace. It reads a listing's details, checks the seller's history, and uses AI to flag common fraud patterns — urgency language, suspicious pricing, recycled images, and more — showing a plain 0-100 risk score right in the browser.

It also checks the current website's address against a list of known marketplaces, to catch lookalike/phishing domains pretending to be the real thing.

**Status:** Fully live on OLX. More marketplaces are on the roadmap.

---

## How it's built

One Rust workspace, one deployed backend, several front-facing pieces:

```
├── backend/       Rust + Axum API server, PostgreSQL via sqlx
├── dashboard/     The logged-in web dashboard (History, Reports, Settings)
├── extension/     The Chrome extension itself
├── site/          The public landing page + sign-in page
├── migrations/     SQL migrations, run automatically on backend startup
└── wasm/          A small Rust to WASM module used by the extension for
                    signal analysis and rendering (falls back to a plain
                    JS implementation if WASM fails to load)
```

Dashboard and site each split their CSS/JS into several small, focused files rather than one large one — see the comment headers at the top of each for what belongs where, and check each page's `<head>`/`<script>` tags for the required load order (some files depend on shared variables/functions defined in earlier ones).

### Core stack

- **Backend:** Rust, Axum, sqlx, PostgreSQL
- **AI analysis:** Claude API
- **Extension:** Vanilla JS (no framework, no build step), Manifest V3
- **Frontend (dashboard/site):** Plain HTML/CSS/JS, Tailwind (via CDN, dashboard only)
- **Hosting:** Hetzner Cloud, Cloudflare (DNS/CDN/HTTPS), Nginx as reverse proxy
- **Email:** Resend
- **Payments:** Creem (in progress)

---

## Running this locally

### 1. Backend

```bash
cd backend
cargo run
```

This needs a `backend/.env` file (not committed to git) with your **local** database credentials:

```
ADMIN_URL=postgres://safely:yourlocalpassword@localhost:5432/safely
APP_URL=postgres://safely:yourlocalpassword@localhost:5432/safely
RESEND_API_KEY=...
RESEND_FROM_EMAIL=...
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
GOOGLE_REDIRECT_URI=http://localhost:3000/api/v1/auth/google/callback
PUBLIC_BASE_URL=http://localhost:3000
ANTHROPIC_API_KEY=...
```

The server listens on `http://localhost:3000` and serves the dashboard, the extension's static files, and the landing site all from the same process.

### 2. Extension

Load `extension/` as an unpacked extension in `chrome://extensions` (enable Developer Mode first).

**Important:** the extension needs to know whether to talk to your local backend or the real production one. This is controlled by one line near the top of `extension/js/core/api.js`:

```js
var SAFELY_ENV = "production"; // "production" | "local"
```

Set this to `"local"` while testing locally, and back to `"production"` before you're done — the extension will not do this automatically. **Reload the unpacked extension in `chrome://extensions` every time you change this.**

### 3. Logging in locally

Open your local dashboard first (`http://localhost:3000/dashboard/`) and log in there *before* analyzing a listing with the extension. The extension picks up your session token from whichever dashboard tab is open and logged in — give it a couple of seconds after logging in before switching to test a listing, so the token has time to sync.

---

## Local vs. production - the two environments never mix

- **Local `.env`** lives only on your own machine, with your own local database's password.
- **Production `.env`** lives only on the Hetzner server, with the real database's password.
- Neither file is ever committed to git (`.gitignore` excludes `.env`), and neither ever needs to contain the other's credentials. Each server only ever needs to know about the database sitting on itself.
- The `SAFELY_ENV` toggle above is the *only* thing that ever needs manually switching — it lives in the extension, in your browser, not in either `.env` file.

---

## Deployment

The backend deploys automatically via GitHub Actions on every push to `main`:

```
git pull -> cargo build --release -> systemctl restart safely
```

See `.github/workflows/deploy.yml`. The dashboard, site, and extension are all static files served directly by the same Rust process - no separate build/deploy step needed for those; a `git pull` on the server is enough for changes to take effect immediately.

The Chrome extension itself is a separate release track - new versions need to be manually zipped and re-submitted through the Chrome Web Store developer dashboard; this isn't automated (deliberately, since it's infrequent and each submission goes through its own review anyway).

---

## A few things worth knowing

- **No free tier.** Every paid plan starts with a trial instead, since AI analysis has a real per-use cost.
- **Anonymous use works.** People can use the extension without an account; signing in just attaches history to their profile and unlocks the dashboard.
- **Domain-lookalike detection** runs independently of listing analysis, on every page, specifically because a fake domain by definition won't be recognized as a real, supported marketplace.
