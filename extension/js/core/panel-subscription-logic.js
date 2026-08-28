"use strict";
function isSubscriptionActive(status) {
    return status === "active" || status === "trialing";
}
globalThis.__safelyIsSubscriptionActive = isSubscriptionActive;
