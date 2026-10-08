#!/bin/bash
# Checks real, LIVE production subscribers.
#
# Works in two places:
# - On your laptop (WSL): connects to the server with "ssh safely"
#   and runs the query there, so the production database password
#   never has to be on your laptop.
# - On the server itself: runs the query directly.
#
# One-time setup on the laptop: the "safely" SSH shortcut in
# ~/.ssh/config (HostName 54.91.198.45, User ubuntu, IdentityFile
# ~/.ssh/<your-key>.pem).
#
# Usage:
#   sed -i 's/\r$//' scripts/check-subscribers-production.sh   (once, after download)
#   bash scripts/check-subscribers-production.sh

APP_DIR="$HOME/app/backend"
SERVER="safely"

QUERY="
SELECT
    u.email,
    s.plan_name,
    s.billing_interval,
    s.status,
    s.current_period_end,
    s.canceled_at
FROM subscriptions s
JOIN users u ON u.id = s.user_id
ORDER BY s.created_at DESC;
"

if [ -f "$APP_DIR/.env" ]; then
    # Already on the server.
    cd "$APP_DIR" || exit 1
    source <(grep -E '^APP_URL=' .env)
    psql "$APP_URL" -c "$QUERY"
else
    # On the laptop: run the same query on the server.
    ssh "$SERVER" "cd ~/app/backend && source <(grep -E '^APP_URL=' .env) && psql \"\$APP_URL\" -c \"$QUERY\""
fi
