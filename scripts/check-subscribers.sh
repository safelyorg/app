#!/bin/bash
# Shows every subscriber - plan, monthly/yearly, status, scans used -
# and every Free user's scan count, in one command.
#
# Usage:
# sed -i 's/\r$//' scripts/check-subscribers.sh
# ./scripts/check-subscribers.sh

psql "postgresql://safely:password@localhost:5432/safely" -c "
SELECT
    u.email,
    s.plan_name,
    s.billing_interval,
    s.status,
    s.scans_used_this_period AS scans_used,
    s.current_period_end,
    s.canceled_at
FROM subscriptions s
JOIN users u ON u.id = s.user_id
ORDER BY s.created_at DESC;
" -c "
SELECT
    u.email,
    f.scans_used AS free_scans_used,
    f.period_start AS free_month_started,
    u.created_at AS signed_up
FROM free_scan_usage f
JOIN users u ON u.id = f.user_id
ORDER BY f.updated_at DESC;
"
