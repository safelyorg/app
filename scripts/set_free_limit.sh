#!/bin/bash
# Changes how many free scans the Free plan gets each month, everywhere:
# backend limit, welcome email, extension (English and Portuguese),
# dashboard, website, /try page and the tests.
#
# Usage (from anywhere):
#   sed -i 's/\r$//' set_free_limit.sh
#   bash set_free_limit.sh 30        <- the new number of free scans
#
# It reads the CURRENT number from the backend by itself, so you only
# type the new one. Safe to run again later with another number.
# The 0-100 risk score, "750 scans" and "100%" are never touched.
#
# Good to know:
# - Run this ONLY when you want a new free limit. It edits the files
#   once and saves them; you never need to run it (or any sed) again
#   before testing. Normal testing is just:
#       cd extension && npx vitest run
#       cargo test
# - The `sed -i 's/\r$//'` line above is needed only once after
#   downloading this file from Windows (it removes Windows line endings).
# - It never touches migrations/, so the database never needs a reset
#   when the free limit changes. The limit lives only in the backend
#   code (FREE_MONTHLY_SCANS in services/billing.rs).
# - It changes the tests too (backend tests and the extension's
#   tests/subscription-status.test.ts), so they keep passing.
# - It does NOT change ad images. Remake any image that shows the
#   old number.
# - After running it: rebuild the dashboard (npx tsc) and the extension,
#   run the tests, then deploy and purge the Cloudflare cache.
set -e

NEW="$1"
if ! [[ "$NEW" =~ ^[0-9]+$ ]] || [ "$NEW" -lt 1 ] || [ "$NEW" -ge 750 ]; then
    echo "Give the new number of free scans, between 1 and 749. Example: bash set_free_limit.sh 30"
    exit 1
fi

cd "/mnt/c/Users/Bilal Khan/Documents/projects/Rust/app"

BILLING=$(grep -rl --include=*.rs --exclude-dir=target "pub const FREE_MONTHLY_SCANS" backend | head -1)
if [ -z "$BILLING" ]; then
    echo "Could not find FREE_MONTHLY_SCANS in the backend. Is the new billing.rs in place?"
    exit 1
fi
echo "Found the limit in: $BILLING"
OLD=$(grep -oP 'pub const FREE_MONTHLY_SCANS: i32 = \K[0-9]+' $BILLING)

if [ "$OLD" = "$NEW" ]; then
    echo "The Free plan is already $NEW scans. Nothing to change."
    exit 0
fi
echo "Changing the Free plan from $OLD to $NEW scans a month..."

# ---------- 1. The real limit (backend) ----------
sed -i "s/pub const FREE_MONTHLY_SCANS: i32 = $OLD;/pub const FREE_MONTHLY_SCANS: i32 = $NEW;/" $BILLING

# ---------- 2. Text shown to users ----------
TEXT_FILES=$(ls \
    backend/templates/*.html \
    extension/ts/core/i18n.ts \
    extension/ts/core/panel.ts \
    extension/ts/core/api.ts \
    extension/ts/core/panel-subscription-logic.ts \
    dashboard/index.html \
    dashboard/ts/init.ts \
    dashboard/locales/*.json \
    site/templates/*.html \
    site/locales/*.json \
    2>/dev/null)
sed -i -E \
    -e "s/\b$OLD( free| scans| supplier| verificações| \/ month|\/month| \/ mês|\/mês)/$NEW\1/g" \
    -e "s/37\/$OLD\b/37\/$NEW/g; s/63\/$OLD\b/63\/$NEW/g; s/\"1\/$OLD\"/\"1\/$NEW\"/g" \
    -e "s/\(limit \|\| $OLD\)/(limit || $NEW)/g" \
    -e "s|<b>$OLD</b><span>free scans a month</span>|<b>$NEW</b><span>free scans a month</span>|g" \
    $TEXT_FILES

# The scan meter on the pricing card: free scans as a share of Team's 750.
PCT=$(( (NEW * 100 + 375) / 750 ))
[ "$PCT" -lt 1 ] && PCT=1
sed -i -E "0,/<div class=\"pm-bar\"><i style=\"width: [0-9]+%\"><\/i><\/div>/s//<div class=\"pm-bar\"><i style=\"width: $PCT%\"><\/i><\/div>/" site/templates/index.html

# ---------- 3. Tests ----------
sed -i -E \
    -e "s/_${OLD}_scans/_${NEW}_scans/g" \
    -e "s/assert_eq!\(FREE_MONTHLY_SCANS, $OLD\);/assert_eq!(FREE_MONTHLY_SCANS, $NEW);/" \
    -e "s/^        allowed, $OLD,/        allowed, $NEW,/" \
    -e "s/\"expected exactly $OLD of 150 scans to be allowed\"/\"expected exactly $NEW of 150 scans to be allowed\"/" \
    -e "s/json!\($OLD\)\);/json!($NEW));/" \
    -e "s/^            assert_eq!\(limit, $OLD\);/            assert_eq!(limit, $NEW);/" \
    -e "s/\/\/ All $OLD used/\/\/ All $NEW used/" \
    -e "s/'1 month', $OLD FROM users/'1 month', $NEW FROM users/" \
    backend/tests/billing_test.rs
sed -i -E \
    -e "s/_after_${OLD}_scans_/_after_${NEW}_scans_/" \
    -e "s/^        limit: $OLD,/        limit: $NEW,/" \
    -e "s/assert_eq!\(body\[\"limit\"\], $OLD\);/assert_eq!(body[\"limit\"], $NEW);/" \
    -e "s/message.contains\(\"$OLD free scans\"\)/message.contains(\"$NEW free scans\")/" \
    -e "s/\/\/ All $OLD used/\/\/ All $NEW used/" \
    -e "s/created_at, $OLD FROM users/created_at, $NEW FROM users/" \
    -e "s/=> assert_eq!\(limit, $OLD\),/=> assert_eq!(limit, $NEW),/" \
    backend/tests/analyze_test.rs

# Extension test for the fallback when the server sends no limit.
ST=extension/tests/subscription-status.test.ts
[ -f $ST ] && sed -i -E \
    -e "s/falls back to $OLD when the limit is missing/falls back to $NEW when the limit is missing/" \
    -e "s/\"$OLD free scans\"/\"$NEW free scans\"/g" \
    -e "s/Free: says $OLD free scans/Free: says $NEW free scans/" \
    -e "s/\"free_scan_limit_reached\", $OLD,/\"free_scan_limit_reached\", $NEW,/" \
    $ST

# ---------- 4. Check ----------
echo
echo "Free plan is now: $(grep -oP 'pub const FREE_MONTHLY_SCANS: i32 = \K[0-9]+' $BILLING) scans a month"
echo "Lines that still say $OLD about the Free plan (should be none):"
grep -rnE "\b$OLD (free|scans|supplier|verificações)|\b$OLD ?/ ?(month|mês)" \
    backend/templates extension/ts dashboard/index.html dashboard/ts dashboard/locales site/templates site/locales 2>/dev/null || echo "  (none)"
echo
echo "Next: rebuild the dashboard and extension, run the tests, and update any ad images that show the old number."
