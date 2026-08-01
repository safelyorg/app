#!/bin/bash
# Checks real, LIVE production subscribers - runs directly on the
# production server via SSH, so the real production database
# credentials never need to be stored on your own laptop at all.
#
# Usage:
# # sed -i 's/\r$//' scripts/check-subscribers-production.sh
# ./check-subscribers-production.sh

ssh root@167.233.216.129 << 'ENDSSH'
cd ~/safely/backend
source <(grep -E '^APP_URL=' .env)
psql "$APP_URL" -c "
SELECT
    u.email,
    s.plan_name,
    s.status,
    s.current_period_end,
    s.canceled_at
FROM subscriptions s
JOIN users u ON u.id = s.user_id
ORDER BY s.created_at DESC;
"
ENDSSH
