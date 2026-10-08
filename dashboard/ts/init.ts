document.addEventListener("DOMContentLoaded", async () => {
  checkGoogleConnectResult();

  if ((window as any).safelyAuth && (window as any).safelyAuth.getToken()) {
    loadDashboardData();
  }

  // Event delegation for history row clicks - works correctly however
  // the rows were inserted, since the listener lives on the parent.
  const historyRowsBody = document.getElementById("history-rows");
  if (historyRowsBody) {
    historyRowsBody.addEventListener("click", (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      const row = target.closest(".history-row") as HTMLElement | null;
      if (row) openDetail(row.dataset.id as string);
    });
  }

  // Updates the "Checked" stat count right after HTMX finishes
  // inserting the real rows - the rows don't exist yet until then.
  document.body.addEventListener("htmx:afterSwap", (event: any) => {
    if (event.target.id === "history-rows" || event.target.id === "report-rows") {
      renderStats();
    }
  });

  const closeBtn = document.getElementById("detail-close");
  if (closeBtn) {
    closeBtn.addEventListener("click", closeDetailPanel);
  }

  const navHistory = document.getElementById("view-history") as HTMLInputElement | null;
  const navReports = document.getElementById("view-reports") as HTMLInputElement | null;
  const navSettings = document.getElementById("view-settings") as HTMLInputElement | null;

  if (navHistory) {
    navHistory.addEventListener("change", closeDetailPanel);
    navHistory.addEventListener("click", closeDetailPanel);
  }
  if (navReports) {
    navReports.addEventListener("change", closeDetailPanel);
    navReports.addEventListener("click", closeDetailPanel);
  }
  if (navSettings) {
    navSettings.addEventListener("change", closeDetailPanel);
    navSettings.addEventListener("click", closeDetailPanel);
    navSettings.addEventListener("change", () => {
      if (!settingsLoaded) loadSettingsData();
    });
    if (navSettings.checked && !settingsLoaded) {
      loadSettingsData();
    }
  }

  const mobileToggle = document.getElementById("mobile-nav-toggle") as HTMLInputElement | null;
  if (mobileToggle) {
    [navHistory, navReports, navSettings].forEach((input) => {
      if (input) {
        input.addEventListener("change", () => {
          mobileToggle.checked = false;
        });
      }
    });
  }

  window.addEventListener("pageshow", () => {
    const settingsRadio = document.getElementById("view-settings") as HTMLInputElement | null;
    if (settingsRadio && settingsRadio.checked && !settingsLoaded) {
      loadSettingsData();
    }
  });

  const settingsLink = document.getElementById("account-settings-link");
  if (settingsLink) {
    settingsLink.addEventListener("click", () => {
      const menu = document.getElementById("account-menu") as any;
      if (menu) menu.open = false;
    });
  }

  const profileEditBtn = document.getElementById("profile-edit-btn");
  if (profileEditBtn) {
    profileEditBtn.addEventListener("click", () => toggleProfileEdit(true));
  }
  const profileCancelBtn = document.getElementById("profile-cancel-btn");
  if (profileCancelBtn) {
    profileCancelBtn.addEventListener("click", () => toggleProfileEdit(false));
  }

  const profileSaveBtn = document.getElementById("profile-save-btn");
  if (profileSaveBtn) {
    profileSaveBtn.addEventListener("click", saveProfileEdit);
  }

  const nameInput = document.getElementById("settings-name-input");
  if (nameInput) {
    nameInput.addEventListener("keydown", (e: KeyboardEvent) => {
      if (e.key === "Enter") saveProfileEdit();
      if (e.key === "Escape") toggleProfileEdit(false);
    });
  }

  // ============================================================
  // Delete account
  // ============================================================
  const deleteBtn = document.getElementById("delete-account-btn");
  const deleteConfirmBox = document.getElementById("delete-account-confirm");
  const deleteConfirmEmailEl = document.getElementById("delete-confirm-email");
  const deleteConfirmInput = document.getElementById("delete-confirm-input") as HTMLInputElement | null;
  const deleteConfirmBtn = document.getElementById("delete-confirm-btn") as HTMLButtonElement | null;
  const deleteCancelBtn = document.getElementById("delete-cancel-btn");
  const deleteConfirmError = document.getElementById("delete-confirm-error");

  if (deleteBtn && deleteConfirmBox) {
    deleteBtn.addEventListener("click", () => {
      const emailEl = document.getElementById("settings-email");
      const accountEmail = emailEl ? emailEl.textContent : "";
      if (deleteConfirmEmailEl) deleteConfirmEmailEl.textContent = accountEmail;
      if (deleteConfirmInput) deleteConfirmInput.value = "";
      if (deleteConfirmBtn) deleteConfirmBtn.disabled = true;
      if (deleteConfirmError) deleteConfirmError.classList.add("hidden");
      deleteConfirmBox.classList.remove("hidden");
      if (deleteConfirmInput) deleteConfirmInput.focus();
    });
  }

  if (deleteCancelBtn && deleteConfirmBox) {
    deleteCancelBtn.addEventListener("click", () => {
      deleteConfirmBox.classList.add("hidden");
    });
  }

  if (deleteConfirmInput && deleteConfirmBtn) {
    deleteConfirmInput.addEventListener("input", () => {
      const emailEl = document.getElementById("settings-email");
      const accountEmail = emailEl ? (emailEl.textContent || "").trim().toLowerCase() : "";
      const typed = deleteConfirmInput.value.trim().toLowerCase();
      deleteConfirmBtn.disabled = !(typed && typed === accountEmail);
    });
  }

  if (deleteConfirmBtn) {
    deleteConfirmBtn.addEventListener("click", async () => {
      deleteConfirmBtn.disabled = true;
      const originalText = deleteConfirmBtn.textContent;
      deleteConfirmBtn.textContent = "Deleting...";
      try {
        const res = await fetch(API_BASE + "/me", {
          method: "DELETE",
          headers: (window as any).safelyAuth.authHeader(),
        });
        if (res.status === 401) {
          (window as any).safelyAuth.logout();
          return;
        }
        if (!res.ok) throw new Error("Failed to delete account");
        (window as any).safelyAuth.clearToken();
        window.location.href = "/?account_deleted=1";
      } catch (e) {
        if (deleteConfirmError) {
          deleteConfirmError.textContent = "Could not delete your account. Please try again.";
          deleteConfirmError.classList.remove("hidden");
        }
        deleteConfirmBtn.disabled = false;
        deleteConfirmBtn.textContent = originalText;
      }
    });
  }

  const googleConnectBtn = document.getElementById("google-connect-btn");
  if (googleConnectBtn) {
    wireGoogleButtonHover();
    googleConnectBtn.addEventListener("click", handleGoogleButtonClick);
  }

  // Creem product IDs: { Team, TeamYearly, Enterprise, EnterpriseYearly }.
  // A yearly ID is null until it's set up on the server.
  let productIds: Record<string, string | null> = {};
  async function loadProductIds(): Promise<void> {
    try {
      const res = await fetch(API_BASE + "/billing/product-ids");
      if (!res.ok) return;
      productIds = await res.json();
    } catch (e) {
      console.error("Safely: failed to load product IDs", e);
    }
  }
  await loadProductIds();

  // ============================================================
  // Plan & Billing - inline expanding section
  // ============================================================
  const planBillingToggle = document.getElementById("plan-billing-toggle");
  const planBillingExpanded = document.getElementById("plan-billing-expanded");
  const planBillingChevron = document.getElementById("plan-billing-chevron");
  const continueBtn = document.getElementById("plan-continue-btn") as HTMLButtonElement | null;
  const cancelSubBtn = document.getElementById("cancel-subscription-btn");
  const cancelSubConfirm = document.getElementById("cancel-sub-confirm");
  const currentPlanBadge = document.getElementById("current-plan-badge");
  const cancelSubArea = document.getElementById("cancel-subscription-area");

  type Interval = "month" | "year";
  let selectedPlanName: string | null = null;
  let selectedInterval: Interval = "month";
  let realSubscriptionStatus: string | null = null;
  let realSubscriptionPlan: string | null = null;
  let realSubscriptionInterval: Interval | null = null;
  const intervalNote = document.getElementById("plan-interval-note");

  function productIdFor(plan: string, interval: Interval): string | null {
    return productIds[interval === "year" ? plan + "Yearly" : plan] || null;
  }

  function planLabel(plan: string, interval: Interval | null): string {
    const name = plan === "Enterprise" ? t("dash.plan.enterprise", "Enterprise") : t("dash.plan.team", "Team");
    if (!interval) return name;
    return interval === "year"
      ? name + " · " + t("dash.plan.yearly", "Yearly")
      : name + " · " + t("dash.plan.monthly", "Monthly");
  }

  // Ticks the plan the user is on - only while its billing (monthly or
  // yearly) is the one shown - or the plan they just picked.
  function renderChecks(): void {
    document.querySelectorAll<HTMLElement>(".plan-option").forEach((opt) => {
      const check = opt.querySelector(".plan-check");
      if (!check) return;
      const plan = opt.dataset.plan;
      const on = selectedPlanName
        ? plan === selectedPlanName
        : plan === realSubscriptionPlan && selectedInterval === realSubscriptionInterval;
      check.classList.toggle("hidden", !on);
    });
  }

  // The line under the plans when moving between monthly and yearly.
  function renderIntervalNote(): void {
    if (!intervalNote) return;
    let text = "";
    if (realSubscriptionPlan && realSubscriptionInterval === "month" && selectedInterval === "year") {
      text = t(
        "dash.plan.note_to_yearly",
        "Your yearly plan starts as soon as you pay, and your monthly plan ends automatically. The rest of the current month isn't refunded.",
      );
    } else if (realSubscriptionPlan && realSubscriptionInterval === "year" && selectedInterval === "month") {
      text = t(
        "dash.plan.note_to_monthly",
        "To move a yearly plan to monthly billing, email help@safely.sh.",
      );
    }
    intervalNote.textContent = text;
    intervalNote.classList.toggle("hidden", !text);
  }

  // Monthly / Yearly switch: swaps the prices and clears the pick.
  function setBillingInterval(interval: Interval): void {
    selectedInterval = interval;
    selectedPlanName = null;
    if (continueBtn) continueBtn.disabled = true;
    document.querySelectorAll<HTMLElement>(".billing-interval-btn").forEach((btn) => {
      const on = btn.dataset.interval === interval;
      btn.classList.toggle("bg-surface3", on);
      btn.classList.toggle("text-ink", on);
      btn.classList.toggle("text-muted", !on);
    });
    document.querySelectorAll<HTMLElement>(".plan-desc").forEach((el) => {
      const key = interval === "year" ? el.dataset.i18nYear : el.dataset.i18nMonth;
      const fallback = (interval === "year" ? el.dataset.textYear : el.dataset.textMonth) || "";
      if (key) el.setAttribute("data-i18n", key);
      el.textContent = key ? t(key, fallback) : fallback;
    });
    renderChecks();
    renderIntervalNote();
  }

  document.querySelectorAll<HTMLElement>(".billing-interval-btn").forEach((btn) => {
    btn.addEventListener("click", () => setBillingInterval(btn.dataset.interval as Interval));
  });
  setBillingInterval("month");

  function togglePlanSection(show: boolean): void {
    if (!planBillingExpanded) return;
    planBillingExpanded.style.maxHeight = show ? "600px" : "0";
    if (planBillingChevron) {
      (planBillingChevron as HTMLElement).style.transform = show ? "rotate(180deg)" : "";
    }
  }

  if (planBillingToggle) {
    planBillingToggle.addEventListener("click", () => {
      const isCurrentlyOpen =
        (planBillingExpanded as HTMLElement).style.maxHeight &&
        (planBillingExpanded as HTMLElement).style.maxHeight !== "0px" &&
        (planBillingExpanded as HTMLElement).style.maxHeight !== "0";
      togglePlanSection(!isCurrentlyOpen);
    });
  }

  // "2026-11-01" or a full timestamp -> "Nov 1". Built from the date
  // parts, so a UTC date never shows as the day before in local time.
  function formatShortDate(value: string | null | undefined): string {
    if (!value) return "";
    const p = value.slice(0, 10).split("-").map((n) => parseInt(n, 10));
    if (p.length !== 3 || p.some((n) => isNaN(n))) return "";
    return new Date(p[0], p[1] - 1, p[2]).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  }

  // The bar under the plan name: "37 of 10 scans used · resets Nov 1".
  function renderUsage(
    usage: { plan: string; used: number; limit: number | null; resets_on: string | null } | null,
  ): void {
    const box = document.getElementById("current-plan-usage");
    const bar = document.getElementById("current-plan-usage-bar");
    const text = document.getElementById("current-plan-usage-text");
    if (!box || !bar || !text) return;
    if (!usage) {
      box.classList.add("hidden");
      return;
    }
    const resets = formatShortDate(usage.resets_on);
    if (usage.limit === null) {
      bar.style.width = "100%";
      text.textContent = t(
        "dash.settings.usage_unlimited",
        "{used} scans this month · unlimited",
      ).replace("{used}", String(usage.used));
    } else {
      const pct = usage.limit > 0 ? Math.min(100, (usage.used / usage.limit) * 100) : 100;
      bar.style.width = pct + "%";
      bar.classList.toggle("bg-coral", pct >= 100);
      bar.classList.toggle("bg-brand", pct < 100);
      let line = t("dash.settings.usage_text", "{used} of {limit} scans used")
        .replace("{used}", String(usage.used))
        .replace("{limit}", String(usage.limit));
      if (resets) line += " · " + t("dash.settings.resets_on", "resets") + " " + resets;
      text.textContent = line;
    }
    box.classList.remove("hidden");
  }

  // Shows either the active paid plan, or the Free plan (everyone
  // without an active paid plan is on Free: 10 scans a month).
  function renderPlan(
    planName: string | null,
    interval: Interval | null,
    periodEnd: string | null,
    scheduledPlan: string | null,
  ): void {
    const isPaid = !!planName;
    const nameEl = document.getElementById("current-plan-name");
    const priceEl = document.getElementById("current-plan-price");

    if (isPaid) {
      if (nameEl) nameEl.textContent = planLabel(planName as string, interval);
      if (priceEl) {
        const renews = formatShortDate(periodEnd);
        let line = renews ? t("dash.settings.renews_at", "Renews at") + " " + renews : "";
        // A downgrade waiting for the renewal (e.g. Enterprise -> Team).
        if (scheduledPlan && scheduledPlan !== planName) {
          line +=
            (line ? " · " : "") +
            t("dash.settings.then_switches_to", "then switches to") +
            " " +
            planLabel(scheduledPlan, interval);
        }
        priceEl.textContent = line;
      }
      if (currentPlanBadge) {
        currentPlanBadge.classList.remove("hidden");
        currentPlanBadge.textContent = t("dash.settings.active_badge", "Active");
      }
    } else {
      if (nameEl) nameEl.textContent = t("dash.settings.free_plan", "Free plan");
      if (priceEl) priceEl.textContent = t("dash.settings.free_desc", "10 free scans every month");
      if (currentPlanBadge) currentPlanBadge.classList.add("hidden");
    }
    if (cancelSubArea) cancelSubArea.classList.toggle("hidden", !isPaid);

    // Open the switch on the billing the user already has.
    setBillingInterval(isPaid && interval === "year" ? "year" : "month");
  }

  async function loadRealSubscriptionStatus(): Promise<void> {
    try {
      const res = await fetch(API_BASE + "/billing/subscription-status", {
        headers: (window as any).safelyAuth.authHeader(),
      });
      if (!res.ok) return;
      const data = await res.json();
      realSubscriptionStatus = data.status;
      realSubscriptionPlan = data.status === "active" ? data.plan_name : null;
      realSubscriptionInterval = realSubscriptionPlan
        ? data.billing_interval === "year"
          ? "year"
          : "month"
        : null;
      renderPlan(
        realSubscriptionPlan,
        realSubscriptionInterval,
        data.current_period_end,
        realSubscriptionPlan ? data.scheduled_plan_name || null : null,
      );
      renderUsage(data.usage || null);
    } catch (e) {
      console.error("Safely: failed to load subscription status", e);
    }
  }
  loadRealSubscriptionStatus();

  const billingParams = new URLSearchParams(window.location.search);
  if (billingParams.get("manage_billing") === "1") {
    (document.getElementById("view-settings") as HTMLInputElement).checked = true;
    togglePlanSection(true);
    history.replaceState(null, "", window.location.pathname);
  }

  // Detect a successful checkout redirect, show a friendly
  // confirmation, then clean Creem's appended parameters out of the
  // URL bar - purely cosmetic, nothing here is trusted for security.
  const checkoutParams = new URLSearchParams(window.location.search);
  if (checkoutParams.get("checkout") === "success") {
    history.replaceState(null, "", window.location.pathname);
    loadRealSubscriptionStatus().then(() => {
      showToast(t("dash.toast.sub_active", "Welcome! Your subscription is now active."));
    });
  }

  document.querySelectorAll<HTMLElement>(".plan-option").forEach((opt) => {
    opt.addEventListener("click", () => {
      selectedPlanName = opt.dataset.plan as string;
      renderChecks();
      if (continueBtn) continueBtn.disabled = false;
    });
  });

  async function startCheckout(productId: string): Promise<void> {
    const res = await fetch(API_BASE + "/billing/checkout", {
      method: "POST",
      headers: Object.assign(
        { "Content-Type": "application/json" },
        (window as any).safelyAuth.authHeader(),
      ),
      body: JSON.stringify({ product_id: productId }),
    });
    if (res.status === 401) {
      (window as any).safelyAuth.logout();
      return;
    }
    if (!res.ok) throw new Error("Checkout creation failed");
    const data = await res.json();
    window.location.href = data.checkout_url;
  }

  if (continueBtn) {
    continueBtn.addEventListener("click", async () => {
      if (!selectedPlanName) return;
      const plan = selectedPlanName;
      const interval = selectedInterval;
      const label = planLabel(plan, interval);
      const productId = productIdFor(plan, interval);
      if (!productId) {
        showToast(label + " " + t("dash.toast.not_available", "isn't available yet - check back soon."));
        return;
      }

      const isActive = realSubscriptionStatus === "active" && !!realSubscriptionPlan;
      if (isActive && realSubscriptionPlan === plan && realSubscriptionInterval === interval) {
        showToast(t("dash.toast.already_subscribed", "You're already subscribed to") + " " + label + ".");
        return;
      }
      if (isActive && realSubscriptionInterval === "year" && interval === "month") {
        showToast(t("dash.plan.note_to_monthly", "To move a yearly plan to monthly billing, email help@safely.sh."));
        return;
      }

      const originalText = continueBtn.textContent;
      continueBtn.disabled = true;

      // Same billing (monthly -> monthly, yearly -> yearly): change the
      // plan in place. Monthly -> yearly, or no plan yet: checkout.
      if (isActive && realSubscriptionInterval === interval) {
        continueBtn.textContent = t("dash.common.updating", "Updating...");
        try {
          const res = await fetch(API_BASE + "/billing/change-plan", {
            method: "POST",
            headers: Object.assign(
              { "Content-Type": "application/json" },
              (window as any).safelyAuth.authHeader(),
            ),
            body: JSON.stringify({ product_id: productId }),
          });
          if (res.status === 401) {
            (window as any).safelyAuth.logout();
            return;
          }
          if (!res.ok) throw new Error("Plan change failed");
          const data = await res.json();
          if (data.applied === "immediately") {
            showToast(t("dash.toast.upgraded", "You've been upgraded to") + " " + label + ".");
          } else {
            showToast(t("dash.toast.switch_at_period_end", "You'll switch to") + " " + label + " " + t("dash.toast.when_period_ends", "when your current period ends."));
          }
          await loadRealSubscriptionStatus();
          togglePlanSection(false);
        } catch (e) {
          showToast(t("dash.toast.plan_update_failed", "Couldn't update your plan. Please try again."));
        } finally {
          continueBtn.disabled = false;
          continueBtn.textContent = originalText;
        }
        return;
      }

      continueBtn.textContent = t("dash.common.redirecting", "Redirecting...");
      try {
        await startCheckout(productId);
      } catch (e) {
        console.error("Safely: failed to start checkout", e);
        showToast(t("dash.toast.checkout_failed", "Couldn't start checkout. Please try again."));
        continueBtn.disabled = false;
        continueBtn.textContent = originalText;
      }
    });
  }

  function toggleCancelConfirm(show: boolean): void {
    if (!cancelSubConfirm) return;
    (cancelSubConfirm as HTMLElement).style.maxHeight = show ? "200px" : "0";
  }

  if (cancelSubBtn) {
    cancelSubBtn.addEventListener("click", () => {
      const isActive = realSubscriptionStatus === "active";
      if (!isActive) {
        showToast(t("dash.toast.no_sub_to_cancel", "You don't have an active subscription to cancel."));
        return;
      }
      toggleCancelConfirm(true);
    });
  }

  const cancelSubConfirmNo = document.getElementById("cancel-sub-confirm-no");
  if (cancelSubConfirmNo) {
    cancelSubConfirmNo.addEventListener("click", () => toggleCancelConfirm(false));
  }
  const cancelSubConfirmYes = document.getElementById("cancel-sub-confirm-yes") as HTMLButtonElement | null;
  if (cancelSubConfirmYes) {
    cancelSubConfirmYes.addEventListener("click", async () => {
      const originalText = cancelSubConfirmYes.textContent;
      cancelSubConfirmYes.disabled = true;
      cancelSubConfirmYes.textContent = t("dash.common.canceling", "Canceling...");
      try {
        const res = await fetch(API_BASE + "/billing/cancel-subscription", {
          method: "POST",
          headers: (window as any).safelyAuth.authHeader(),
        });
        if (res.status === 401) {
          (window as any).safelyAuth.logout();
          return;
        }
        if (!res.ok) throw new Error("Cancel failed");

        // The server marks the subscription canceled straight away, so
        // re-reading the status now shows the Free plan and its usage.
        await loadRealSubscriptionStatus();
        toggleCancelConfirm(false);
        togglePlanSection(false);
        showToast(t("dash.toast.sub_canceled", "Your subscription has been canceled."));
      } catch (e) {
        showToast(t("dash.toast.cancel_failed", "Couldn't cancel your subscription. Please try again."));
        cancelSubConfirmYes.disabled = false;
        cancelSubConfirmYes.textContent = originalText;
      }
    });
  }

  // ============================================================
  // Terms & Privacy modal
  // ============================================================
  const termsBtn = document.getElementById("terms-privacy-btn");
  const termsModal = document.getElementById("terms-privacy-modal");
  const termsClose = document.getElementById("terms-privacy-close");
  const termsBackdrop = document.getElementById("terms-privacy-backdrop");

  function toggleTermsModal(show: boolean): void {
    if (!termsModal) return;
    termsModal.classList.toggle("hidden", !show);
    termsModal.classList.toggle("flex", show);
  }

  if (termsBtn) termsBtn.addEventListener("click", () => toggleTermsModal(true));
  if (termsClose) termsClose.addEventListener("click", () => toggleTermsModal(false));
  if (termsBackdrop) termsBackdrop.addEventListener("click", () => toggleTermsModal(false));

  document.addEventListener("keydown", (e: KeyboardEvent) => {
    if (e.key === "Escape") toggleTermsModal(false);
  });
  const avatarInput = document.getElementById("settings-avatar-input") as HTMLInputElement | null;
  if (avatarInput) {
    avatarInput.addEventListener("change", (e: Event) => {
      const target = e.target as HTMLInputElement;
      const file = target.files && target.files[0];
      if (file) uploadAvatar(file);
    });
  }

  document.querySelectorAll<HTMLElement>(".detail-tab-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      switchDetailTab(btn.dataset.detailTab as string);
    });
  });

  const searchBox = document.getElementById("search-box") as HTMLInputElement | null;
  if (searchBox) {
    searchBox.addEventListener("input", (e: Event) => {
      const target = e.target as HTMLInputElement;
      const q = target.value.trim().toLowerCase();
      document.querySelectorAll("#history-rows tr").forEach((tr) => {
        const haystack = tr.getAttribute("data-search") || "";
        tr.setAttribute("data-search-hidden", q && haystack.indexOf(q) === -1 ? "true" : "false");
      });
    });
  }

  document.addEventListener("click", (e: MouseEvent) => {
    const menu = document.getElementById("account-menu") as any;
    if (menu && menu.open && !menu.contains(e.target as Node)) {
      menu.open = false;
    }
  });
});
