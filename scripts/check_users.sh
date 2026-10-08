#!/bin/bash
# Shows sign-ups, paid subscribers and Free scan usage for BOTH the
# local database and the live production database, in one command.
#
# Run it on your laptop (WSL), from the app folder:
#   sed -i 's/\r$//' scripts/check_users.sh
#   bash scripts/check_users.sh            <- local + production
#   bash scripts/check_users.sh local      <- local only
#   bash scripts/check_users.sh prod       <- production only
#
# Production is read through the "safely" SSH shortcut in
# ~/.ssh/config, so the production password never leaves the server.
# If you run it on the server itself, it just shows that database.
#
# Only reads data. It never changes anything.

WHICH="${1:-both}"
APP_DIR="/mnt/c/Users/Bilal Khan/Documents/projects/Rust/app"
SERVER="safely"

# The same report for both databases. Columns that only exist after the
# newest migration (like billing_interval) are read safely, so the report
# also works on a database that has not been updated yet.
read -r -d '' REPORT <<'SQL'
\pset footer off
\echo '--- Totals ---'
SELECT
    (SELECT COUNT(*) FROM users) AS sign_ups,
    (SELECT COUNT(*) FROM users WHERE created_at >= NOW() - interval '7 days') AS last_7_days,
    (SELECT COUNT(*) FROM subscriptions) AS paid_subscriptions;

\echo ''
\echo '--- Latest 20 sign-ups ---'
SELECT email, created_at::date AS signed_up
FROM users
ORDER BY created_at DESC
LIMIT 20;

\echo ''
\echo '--- Paid subscribers ---'
SELECT
    u.email,
    s.plan_name,
    to_jsonb(s) ->> 'billing_interval' AS billing,
    s.status,
    to_jsonb(s) ->> 'scans_used_this_period' AS scans_used,
    s.current_period_end::date AS renews_or_ends,
    s.canceled_at::date AS canceled
FROM subscriptions s
JOIN users u ON u.id = s.user_id
ORDER BY s.created_at DESC;

SELECT to_regclass('public.free_scan_usage') IS NOT NULL AS has_free_table \gset
\if :has_free_table
\echo ''
\echo '--- Free users who scanned (this Free month) ---'
SELECT
    u.email,
    f.scans_used AS free_scans_used,
    f.period_start::date AS free_month_started
FROM free_scan_usage f
JOIN users u ON u.id = f.user_id
ORDER BY f.updated_at DESC;
\endif
SQL

show_local() {
    echo
    echo "================  LOCAL  ================"
    if [ ! -f "$APP_DIR/backend/.env" ]; then
        echo "No backend/.env found in $APP_DIR"
        return
    fi
    source <(grep -E '^APP_URL=' "$APP_DIR/backend/.env" | tr -d '\r')
    psql "$APP_URL" -X -q <<< "$REPORT"
}

show_prod() {
    echo
    echo "==============  PRODUCTION  =============="
    ssh "$SERVER" "cd ~/app/backend && source <(grep -E '^APP_URL=' .env | tr -d '\r') && psql \"\$APP_URL\" -X -q" <<< "$REPORT" \
        || echo "Could not reach the server. Check that 'ssh safely' works."
}

# Running on the server itself: only that database.
if [ -f "$HOME/app/backend/.env" ] && [ ! -d "/mnt/c" ]; then
    echo
    echo "==============  THIS SERVER  =============="
    source <(grep -E '^APP_URL=' "$HOME/app/backend/.env" | tr -d '\r')
    psql "$APP_URL" -X -q <<< "$REPORT"
    exit 0
fi

case "$WHICH" in
    local) show_local ;;
    prod)  show_prod ;;
    *)     show_local; show_prod ;;
esac
