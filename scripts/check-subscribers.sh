#!/bin/bash
# Shows every real subscriber, their plan, and current status - a
# quick, one-command way to answer "who's actually subscribed right
# now", without needing to manually connect and type SQL each time.
#
# Usage:
# sed -i 's/\r$//' scripts/bump-version.sh
# ./check-subscribers.sh

psql "postgresql://safely:password@localhost:5432/safely" -c "
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
