function isSubscriptionActive(status: string | null): boolean {
  return status === "active" || status === "trialing";
}
(globalThis as any).__safelyIsSubscriptionActive = isSubscriptionActive;
