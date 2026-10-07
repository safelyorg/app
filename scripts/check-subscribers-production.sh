#!/bin/bash
# Checks real, LIVE production subscribers - runs directly on the
# production server via SSH, so the real production database
# credentials never need to be stored on your own laptop at all.
#
# Usage:
# sed -i 's/\r$//' scripts/check-subscribers-production.sh
# ./scripts/check-subscribers-production.sh

# Change these two lines if your key file or app folder are different.
SSH_KEY="$HOME/.ssh/your-key.pem"
APP_DIR="~/safely/backend"

ssh -i "$SSH_KEY" ubuntu@54.91.198.45 "cd $APP_DIR && bash -s" << 'ENDSSH'
source <(grep -E '^APP_URL=' .env)
psql "$APP_URL" -c "
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
ENDSSH
