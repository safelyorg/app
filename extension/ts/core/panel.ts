interface PendingTabRegistration {
  id: string;
  title: string;
  html: string;
  iconSvg: string;
  initFn: ((root: HTMLElement) => void) | null;
}

(async function () {
  "use strict";

  if (document.getElementById("safely-root")) return;

  // Translates through i18n.ts; plain English if it isn't loaded.
  function tr(en: string, vars?: Record<string, string | number>): string {
    const i18n = (window as any).__safelyI18n;
    if (i18n) return i18n.t(en, vars);
    if (!vars) return en;
    return en.replace(/\{(\w+)\}/g, (whole, key) =>
      Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole,
    );
  }

  const UNSUPPORTED_TEXT =
    "Safely doesn't check this page — open a listing on a supported marketplace to scan it.";
  const SIGNIN_TEXT =
    "Sign in free to analyze this listing. You get 5 free scans every month — no credit card needed.";

  let panelVisible = false;
  let toolbarExpanded = false;
  let currentTab = "";
  let collapseTimer: ReturnType<typeof setTimeout> | undefined;
  let intentionallyClosed = false;
  let tabsHaveBeenBuilt = false;
  let pendingTabRegistrations: PendingTabRegistration[] = [];
  const tabIds: string[] = [];
  const tabTitles: Record<string, string> = {};
  // Redraws the current "couldn't analyze" / "scan limit" message when
  // the language changes. Null when no such message is showing.
  let renderFailedMessage: (() => void) | null = null;
  let renderScanLimitMessage: (() => void) | null = null;
  // The scraper's own "not supported" message, when it gave one.
  let unsupportedCustomMessage: string | null = null;

  // Base DOM Structure — the unsupported notice exists from the start
  // as its own permanent piece of the panel, separate from tabsArea
  // (where the 3 real tabs get built) - this way there's no possibility
  // of the real tabs ever appearing alongside it by accident.
  const root = document.createElement("div");
  root.id = "safely-root";
  root.innerHTML =
    '<div id="safely-panel">' +
    '<div class="safely-panel-header">' +
    '<span class="safely-panel-title" id="safely-panel-title">Safely</span>' +
    '<span id="safely-usage-line" style="display:none; margin-left:10px; font-size:11px; color:#8a8a93; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;"></span>' +
    '<button type="button" id="safely-lang-btn" style="margin-left:auto; margin-right:8px; padding:3px 8px; border:1px solid #3a3a42; border-radius:6px; background:transparent; color:#8a8a93; font-size:11px; font-weight:700; letter-spacing:0.4px; line-height:1.4; cursor:pointer;"></button>' +
    '<div class="safely-close-btn" id="safely-close-btn" style="margin-left:0;">×</div>' +
    "</div>" +
    '<div class="safely-tabs-area" id="safely-tabs-area"></div>' +
    '<div class="safely-loading-overlay" id="safely-loading-overlay"><div class="safely-loading-dots"><span></span><span></span><span></span></div></div>' +
    '<div class="safely-tab-content" id="safely-tab-unsupported" style="display:none; padding: 20px; font-size: 13px; line-height: 1.5; color: #8a8a93;">' +
    '<span id="safely-unsupported-message"></span>' +
    "</div>" +
    '<div class="safely-tab-content" id="safely-tab-signin-required" style="display:none; padding: 20px; text-align: center;">' +
    '<div id="safely-signin-text" style="font-size:13px; line-height:1.6; color:#8a8a93; margin-bottom:16px;"></div>' +
    '<a href="' +
    (window as any).__safelyAPI.SITE_BASE +
    '" target="_blank" class="safely-signin-required-btn" id="safely-signin-btn"></a>' +
    "</div>" +
    '<div class="safely-tab-content" id="safely-tab-analysis-failed" style="display:none; padding: 20px; text-align: center;">' +
    '<div class="safely-failed-icon">&#9888;</div>' +
    '<div class="safely-failed-message" id="safely-failed-message" style="font-size:13px; line-height:1.6; color:#8a8a93; margin-top:10px;"></div>' +
    '<button class="safely-retry-btn" id="safely-retry-btn" style="display:none;">' +
    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 4 1 10 7 10"></polyline><path d="M3.51 15a9 9 0 102.13-9.36L1 10"></path></svg>' +
    '<span id="safely-retry-text"></span>' +
    "</button>" +
    "</div>" +
    '<div class="safely-tab-content" id="safely-tab-scan-limit-reached" style="display:none; padding: 20px; text-align: center;">' +
    '<div style="font-size:13px; line-height:1.6; color:#8a8a93; margin-bottom:16px;" id="safely-scan-limit-message"></div>' +
    '<a href="' +
    (window as any).__safelyAPI.SITE_BASE +
    '/dashboard/?manage_billing=1" target="_blank" class="safely-signin-required-btn" id="safely-see-plans-btn"></a>' +
    "</div>" +
    "</div>" +
    '<div id="safely-toolbar"><img class="safely-toolbar-letter" src="' +
    chrome.runtime.getURL("icons/icon48.png") +
    '" alt="Safely" /><div class="safely-toolbar-inner" id="safely-toolbar-inner">' +
    '<span class="safely-toolbar-label" id="safely-collapse-btn">Safely</span>' +
    "</div></div>";
  document.body.appendChild(root);
  (window as any).__safelyRoot = root;
  const panel = document.getElementById("safely-panel") as HTMLElement;
  const toolbar = document.getElementById("safely-toolbar") as HTMLElement;
  const collapseBtn = document.getElementById("safely-collapse-btn") as HTMLElement;
  const panelTitle = document.getElementById("safely-panel-title") as HTMLElement;
  const closeBtn = document.getElementById("safely-close-btn") as HTMLElement;
  const langBtn = document.getElementById("safely-lang-btn") as HTMLButtonElement | null;
  const tabsArea = document.getElementById("safely-tabs-area") as HTMLElement;
  const loadingOverlay = document.getElementById("safely-loading-overlay") as HTMLElement | null;
  const toolbarInner = document.getElementById("safely-toolbar-inner") as HTMLElement;
  const unsupportedContent = document.getElementById("safely-tab-unsupported") as HTMLElement;
  const unsupportedMessageEl = document.getElementById(
    "safely-unsupported-message",
  ) as HTMLElement | null;
  const signinRequiredContent = document.getElementById(
    "safely-tab-signin-required",
  ) as HTMLElement;
  const signinText = document.getElementById("safely-signin-text") as HTMLElement | null;
  const signinBtn = document.getElementById("safely-signin-btn") as HTMLElement | null;
  const analysisFailedContent = document.getElementById(
    "safely-tab-analysis-failed",
  ) as HTMLElement;
  const usageLine = document.getElementById("safely-usage-line") as HTMLElement | null;
  const scanLimitReachedContent = document.getElementById(
    "safely-tab-scan-limit-reached",
  ) as HTMLElement;
  const scanLimitMessage = document.getElementById(
    "safely-scan-limit-message",
  ) as HTMLElement | null;
  const seePlansBtn = document.getElementById("safely-see-plans-btn") as HTMLElement | null;
  const failedMessage = document.getElementById("safely-failed-message") as HTMLElement | null;
  const retryBtn = document.getElementById("safely-retry-btn") as HTMLElement | null;
  const retryText = document.getElementById("safely-retry-text") as HTMLElement | null;

  if (retryBtn) {
    retryBtn.addEventListener("click", () => {
      // Reuses the exact same check-then-fetch flow that already runs
      // on every real navigation.
      updateSupportState();
    });
  }

  // EN / PT switch in the panel header. The choice is saved and used
  // on every page and tab.
  if (langBtn) {
    langBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const i18n = (window as any).__safelyI18n;
      if (!i18n) return;
      i18n.setLang(i18n.getLang() === "pt-br" ? "en" : "pt-br");
    });
  }

  // ── Reserve icon positions: the 3 real tabs PLUS one dedicated
  // "unsupported" icon, kept as separate slots so exactly one relevant
  // set is ever visible at a time. ──
  const TAB_ORDER = ["risk", "intelligence", "protect"];
  const iconSlots: Record<string, HTMLElement> = {};

  TAB_ORDER.forEach((id) => {
    const iconDiv = document.createElement("div");
    iconDiv.className = "safely-toolbar-icon";
    iconDiv.dataset.open = id;
    iconDiv.style.display = "none";
    toolbarInner.insertBefore(iconDiv, collapseBtn);
    iconDiv.addEventListener("click", (e) => {
      e.stopPropagation();
      togglePanel(id);
    });
    iconSlots[id] = iconDiv;
  });

  const unsupportedIcon = document.createElement("div");
  unsupportedIcon.className = "safely-toolbar-icon";
  unsupportedIcon.dataset.open = "unsupported";
  unsupportedIcon.style.display = "none";
  unsupportedIcon.innerHTML =
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="8" x2="12" y2="12"></line><line x1="12" y1="16" x2="12.01" y2="16"></line></svg>';
  toolbarInner.insertBefore(unsupportedIcon, collapseBtn);
  unsupportedIcon.addEventListener("click", (e) => {
    e.stopPropagation();
    togglePanel("unsupported");
  });

  const signinRequiredIcon = document.createElement("div");
  signinRequiredIcon.className = "safely-toolbar-icon";
  signinRequiredIcon.dataset.open = "signin-required";
  signinRequiredIcon.style.display = "none";
  signinRequiredIcon.innerHTML =
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h4a2 2 0 012 2v14a2 2 0 01-2 2h-4"></path><polyline points="10 17 15 12 10 7"></polyline><line x1="15" y1="12" x2="3" y2="12"></line></svg>';
  toolbarInner.insertBefore(signinRequiredIcon, collapseBtn);
  signinRequiredIcon.addEventListener("click", (e) => {
    e.stopPropagation();
    togglePanel("signin-required");
  });
  const scanLimitReachedIcon = document.createElement("div");
  scanLimitReachedIcon.className = "safely-toolbar-icon";
  scanLimitReachedIcon.dataset.open = "scan-limit-reached";
  scanLimitReachedIcon.style.display = "none";
  scanLimitReachedIcon.innerHTML =
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><path d="M12 6v6l4 2"></path></svg>';
  toolbarInner.insertBefore(scanLimitReachedIcon, collapseBtn);
  scanLimitReachedIcon.addEventListener("click", (e) => {
    e.stopPropagation();
    togglePanel("scan-limit-reached");
  });

  const analysisFailedIcon = document.createElement("div");
  analysisFailedIcon.className = "safely-toolbar-icon";
  analysisFailedIcon.dataset.open = "analysis-failed";
  analysisFailedIcon.style.display = "none";
  analysisFailedIcon.innerHTML =
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>';
  toolbarInner.insertBefore(analysisFailedIcon, collapseBtn);
  analysisFailedIcon.addEventListener("click", (e) => {
    e.stopPropagation();
    togglePanel("analysis-failed");
  });

  const SPECIAL_STATES = ["unsupported", "signin-required", "analysis-failed", "scan-limit-reached"];

  // Every fixed text in the panel, in the current language. Runs once
  // now and again whenever the language changes.
  function applyStaticTexts(): void {
    const i18n = (window as any).__safelyI18n;
    if (langBtn) {
      if (i18n) {
        langBtn.textContent = i18n.getLang() === "pt-br" ? "PT" : "EN";
        langBtn.title = tr("Change language");
      } else {
        langBtn.style.display = "none";
      }
    }
    if (unsupportedMessageEl) {
      unsupportedMessageEl.textContent = tr(unsupportedCustomMessage || UNSUPPORTED_TEXT);
    }
    if (signinText) signinText.textContent = tr(SIGNIN_TEXT);
    if (signinBtn) signinBtn.textContent = tr("Sign in free");
    if (retryText) retryText.textContent = tr("Reload");
    if (seePlansBtn) seePlansBtn.textContent = tr("See plans");
    if (scanLimitMessage && !renderScanLimitMessage) {
      scanLimitMessage.textContent = tr("You've used all your scans for this month.");
    }
    unsupportedIcon.title = tr("Not a listing page");
    signinRequiredIcon.title = tr("Sign in required");
    scanLimitReachedIcon.title = tr("Scan limit reached");
    analysisFailedIcon.title = tr("Couldn't analyze this listing");
    tabIds.forEach((id) => {
      if (iconSlots[id]) iconSlots[id].title = tr(tabTitles[id] || id);
    });
    if (currentTab && SPECIAL_STATES.indexOf(currentTab) === -1) {
      panelTitle.textContent = tr(tabTitles[currentTab] || currentTab);
    }
    if (renderFailedMessage) renderFailedMessage();
    if (renderScanLimitMessage) renderScanLimitMessage();
  }

  function switchTab(tab: string): void {
    currentTab = tab;
    if (SPECIAL_STATES.indexOf(tab) !== -1) {
      panelTitle.textContent = "Safely";
      tabIds.forEach((id) => {
        const el = document.getElementById("safely-tab-" + id);
        if (el) el.style.display = "none";
      });
      unsupportedContent.style.display = tab === "unsupported" ? "block" : "none";
      signinRequiredContent.style.display = tab === "signin-required" ? "block" : "none";
      analysisFailedContent.style.display = tab === "analysis-failed" ? "block" : "none";
      scanLimitReachedContent.style.display = tab === "scan-limit-reached" ? "block" : "none";
    } else {
      panelTitle.textContent = tr(tabTitles[tab] || tab);
      unsupportedContent.style.display = "none";
      signinRequiredContent.style.display = "none";
      analysisFailedContent.style.display = "none";
      scanLimitReachedContent.style.display = "none";
      tabIds.forEach((id) => {
        const el = document.getElementById("safely-tab-" + id);
        if (el) el.style.display = id === tab ? "block" : "none";
      });
    }
    if (tabsArea) tabsArea.scrollTop = 0;
  }
  function togglePanel(tab: string): void {
    if (panelVisible && currentTab === tab) {
      panelVisible = false;
      panel.classList.remove("safely-visible");
    } else {
      switchTab(tab);
      panelVisible = true;
      panel.classList.add("safely-visible");
    }
  }

  function closePanel(): void {
    panelVisible = false;
    panel.classList.remove("safely-visible");
  }

  function collapseToolbar(): void {
    toolbarExpanded = false;
    panelVisible = false;
    toolbar.classList.remove("safely-toolbar-expanded");
    panel.classList.remove("safely-visible");
  }

  // The REAL tab-building logic - this only ever actually runs once
  // we're certain we're on a genuine listing. The title is kept in
  // English and translated when shown.
  function reallyAddTab(
    id: string,
    title: string,
    html: string,
    iconSvg: string,
    initFn: ((root: HTMLElement) => void) | null,
  ): void {
    tabIds.push(id);
    tabTitles[id] = title;
    const tabDiv = document.createElement("div");
    tabDiv.className = "safely-tab-content";
    tabDiv.id = "safely-tab-" + id;
    tabDiv.style.display = "none";
    tabDiv.innerHTML = html;
    tabsArea.appendChild(tabDiv);

    const iconDiv = iconSlots[id];
    if (iconDiv) {
      iconDiv.title = tr(title);
      iconDiv.innerHTML = iconSvg;
      iconDiv.style.display = "flex";
    }
    if (id === "risk") switchTab(id);
    if (typeof initFn === "function") initFn(root);
  }
  // Until we know for certain this is a real listing page, calls from
  // tabs/risk.js, tabs/intelligence.js, and tabs/protect.js are only
  // QUEUED, never actually built into the DOM. This guarantees the
  // three real tabs can never appear on a page that isn't a listing.
  (window as any).__safelyAddTab = function (
    id: string,
    title: string,
    html: string,
    iconSvg: string,
    initFn: ((root: HTMLElement) => void) | null,
  ): void {
    pendingTabRegistrations.push({ id, title, html, iconSvg, initFn });
  };

  function buildQueuedTabsIfNeeded(): void {
    if (tabsHaveBeenBuilt) return;
    tabsHaveBeenBuilt = true;
    (window as any).__safelyAddTab = reallyAddTab;
    pendingTabRegistrations.forEach((t) => {
      reallyAddTab(t.id, t.title, t.html, t.iconSvg, t.initFn);
    });
    pendingTabRegistrations = [];
  }

  // Global helper to stop inputs from bubbling to the host page.
  (window as any).__safelyPreventInputBubbling = function (): void {
    root.querySelectorAll("input, textarea, select").forEach((el) => {
      ["keydown", "keyup", "keypress"].forEach((evt) => {
        const stop = (e: Event) => e.stopImmediatePropagation();
        el.removeEventListener(evt, stop, true);
        el.addEventListener(evt, stop, true);
      });
    });
  };

  // ── Switches between "real listing" and "not supported" - can be
  // called repeatedly as navigation happens. ──
  function updateSupportState(): void {
    const requestUrl = window.location.href;
    const isStillCurrentPage = () => window.location.href === requestUrl;

    const supported = (window as any).__safelyScrapers.isListingPage();

    if (supported) {
      unsupportedIcon.style.display = "none";

      chrome.storage.local.get("safely_session_token", async (result) => {
        if (!isStillCurrentPage()) return;
        if (result.safely_session_token) {
          // Everyone signed in can scan: Free gets 5 scans a month,
          // paid plans get more. The server checks the limit on each
          // scan and replies with free_scan_limit_reached /
          // scan_limit_reached when it's used up.
          signinRequiredIcon.style.display = "none";
          analysisFailedIcon.style.display = "none";
          scanLimitReachedIcon.style.display = "none";
          renderFailedMessage = null;
          renderScanLimitMessage = null;

          buildQueuedTabsIfNeeded();
          TAB_ORDER.forEach((id) => {
            if (iconSlots[id] && tabTitles[id]) {
              iconSlots[id].style.display = "flex";
            }
          });
          if (tabIds.indexOf("risk") !== -1) switchTab("risk");
          (window as any).__safelyResetState();
          if (loadingOverlay) loadingOverlay.classList.add("safely-visible");
          if (tabsArea) tabsArea.classList.add("safely-loading-blur");
          (window as any).__safelyAPI.fetchAnalysis();
        } else {
          TAB_ORDER.forEach((id) => {
            if (iconSlots[id]) iconSlots[id].style.display = "none";
          });
          signinRequiredIcon.style.display = "flex";
          analysisFailedIcon.style.display = "none";
          scanLimitReachedIcon.style.display = "none";
          if (usageLine) usageLine.style.display = "none";
          switchTab("signin-required");
        }
      });
    } else {
      TAB_ORDER.forEach((id) => {
        if (iconSlots[id]) iconSlots[id].style.display = "none";
      });
      signinRequiredIcon.style.display = "none";
      analysisFailedIcon.style.display = "none";
      scanLimitReachedIcon.style.display = "none";
      unsupportedIcon.style.display = "flex";

      unsupportedCustomMessage =
        (window as any).__safelyScrapers.getUnsupportedMessage?.() || null;
      if (unsupportedMessageEl) {
        unsupportedMessageEl.textContent = tr(unsupportedCustomMessage || UNSUPPORTED_TEXT);
      }

      switchTab("unsupported");
    }
  }
  // ── Toolbar Hover Events ──
  toolbar.addEventListener("mouseenter", () => {
    clearTimeout(collapseTimer);
    if (intentionallyClosed) return;
    toolbarExpanded = true;
    toolbar.classList.add("safely-toolbar-expanded");
  });
  toolbar.addEventListener("mouseleave", (e: MouseEvent) => {
    if (intentionallyClosed) return;
    if (e.relatedTarget && panel.contains(e.relatedTarget as Node)) return;
    collapseTimer = setTimeout(collapseToolbar, 200);
  });
  panel.addEventListener("mouseenter", () => {
    clearTimeout(collapseTimer);
  });
  panel.addEventListener("mouseleave", (e: MouseEvent) => {
    if (e.relatedTarget && toolbar.contains(e.relatedTarget as Node)) return;
    collapseTimer = setTimeout(collapseToolbar, 200);
  });
  toolbar.addEventListener("click", (e) => {
    e.stopPropagation();
    if (intentionallyClosed) {
      intentionallyClosed = false;
      toolbarExpanded = true;
      toolbar.classList.add("safely-toolbar-expanded");
    }
  });

  // Clicking "Safely" label collapses and locks until mouse leaves.
  collapseBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    intentionallyClosed = true;
    collapseToolbar();
  });
  closeBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    closePanel();
  });

  // Clicking outside closes panel and collapses toolbar (with lock).
  document.addEventListener("click", (e: MouseEvent) => {
    if (!root.contains(e.target as Node)) {
      if (panelVisible) {
        panelVisible = false;
        panel.classList.remove("safely-visible");
      }
      if (toolbarExpanded) {
        toolbarExpanded = false;
        intentionallyClosed = true;
        toolbar.classList.remove("safely-toolbar-expanded");
      }
    }
  });

  // Once the mouse fully leaves the root, unlock hover-expand.
  root.addEventListener("mouseleave", () => {
    if (intentionallyClosed) {
      setTimeout(() => {
        intentionallyClosed = false;
      }, 150);
    }
  });
  // ── Rate-limit countdown, shown on the analysis-failed tab. ──
  let rateLimitCountdownTimer: ReturnType<typeof setInterval> | null = null;

  function formatCountdown(totalSeconds: number): string {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return minutes + ":" + (seconds < 10 ? "0" : "") + seconds;
  }

  (globalThis as any).__safelyFormatCountdown = formatCountdown;

  function startRateLimitCountdown(seconds: number): void {
    if (rateLimitCountdownTimer) clearInterval(rateLimitCountdownTimer);
    let remaining = seconds;

    function render(): void {
      if (failedMessage) {
        failedMessage.textContent =
          remaining > 0
            ? tr("You've checked several listings quickly. Try again in {time}.", {
                time: formatCountdown(remaining),
              })
            : tr("You can check another listing now.");
      }
      if (retryBtn) retryBtn.style.display = remaining > 0 ? "none" : "flex";
    }

    renderFailedMessage = render;
    render();
    rateLimitCountdownTimer = setInterval(() => {
      remaining -= 1;
      if (remaining <= 0) {
        clearInterval(rateLimitCountdownTimer as ReturnType<typeof setInterval>);
        rateLimitCountdownTimer = null;
        remaining = 0;
      }
      render();
    }, 1000);
  }
  // The last plan / usage the server sent, so the line can be redrawn
  // in another language without asking the server again.
  let lastUsage: unknown = null;

  function showUsageLine(): void {
    if (!usageLine) return;
    const text = (globalThis as any).__safelyFormatUsageLine(lastUsage);
    usageLine.textContent = text;
    usageLine.style.display = text ? "inline" : "none";
  }

  async function refreshUsageLine(): Promise<void> {
    if (!usageLine) return;
    lastUsage = await (window as any).__safelyAPI.getScanUsage();
    showUsageLine();
  }

  window.addEventListener("safely-analysis-finished", (e: any) => {
    if (loadingOverlay) loadingOverlay.classList.remove("safely-visible");
    if (tabsArea) tabsArea.classList.remove("safely-loading-blur");

    // Refresh the "63/5" scans-used line after every scan attempt.
    // It never blocks anything.
    refreshUsageLine();

    const reason = e.detail && e.detail.error;
    if (!reason) return; // success - tabs are already showing real data

    if (reason === "unauthorized") {
      signinRequiredIcon.style.display = "flex";
      TAB_ORDER.forEach((id) => {
        if (iconSlots[id]) iconSlots[id].style.display = "none";
      });
      switchTab("signin-required");
      return;
    }

    if (reason === "scan_limit_reached" || reason === "free_scan_limit_reached") {
      const scanLimit = e.detail.scanLimit;
      const resetsOn = e.detail.resetsOn;
      renderScanLimitMessage = () => {
        if (scanLimitMessage) {
          scanLimitMessage.textContent = (globalThis as any).__safelyScanLimitMessage(
            reason,
            scanLimit,
            resetsOn,
          );
        }
      };
      renderScanLimitMessage();
      scanLimitReachedIcon.style.display = "flex";
      TAB_ORDER.forEach((id) => {
        if (iconSlots[id]) iconSlots[id].style.display = "none";
      });
      switchTab("scan-limit-reached");
      return;
    }

    if (reason === "rate_limited" && e.detail.retryAfterSeconds) {
      startRateLimitCountdown(e.detail.retryAfterSeconds);
    } else if (failedMessage) {
      renderFailedMessage = () => {
        if (failedMessage) {
          failedMessage.textContent = tr(
            "Couldn't analyze this listing right now. Please try again in a moment.",
          );
        }
      };
      renderFailedMessage();
      if (retryBtn) retryBtn.style.display = "flex";
    }

    analysisFailedIcon.style.display = "flex";
    TAB_ORDER.forEach((id) => {
      if (iconSlots[id]) iconSlots[id].style.display = "none";
    });
    switchTab("analysis-failed");
  });
  // If someone signs in on a separate tab while this panel is showing
  // "sign in required," this picks that up immediately.
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && changes.safely_session_token && currentTab === "signin-required") {
      updateSupportState();
    }
  });

  // ── Result text in the other language ──
  // The labels switch on their own (i18n.ts). The reasons under each
  // check and the risk factor explanations were written by the backend
  // in the language of the scan, so they are fetched again, once per
  // language, from the scan saved on the server (no new scan is used).
  // Both versions are kept here, so switching back and forth after that
  // is instant.
  interface ResultTexts {
    subs: string[];
    descriptions: string[];
  }
  let textsForAnalysis: string | null = null;
  let textsByLanguage: Record<string, ResultTexts> = {};

  function currentResultTexts(data: any): ResultTexts {
    return {
      subs: (data.signals || []).map((s: any) => s.sub || ""),
      descriptions: (data.riskFactors || []).map((f: any) => f.description || ""),
    };
  }

  async function showResultIn(language: string): Promise<void> {
    const data = (window as any).__safelyData;
    if (!data || !data.analysisId) return;
    const analysisId: string = data.analysisId;
    const shownLanguage: string = data.textLanguage || "en";

    if (textsForAnalysis !== analysisId) {
      textsForAnalysis = analysisId;
      textsByLanguage = {};
    }
    if (!textsByLanguage[shownLanguage]) {
      textsByLanguage[shownLanguage] = currentResultTexts(data);
    }
    if (shownLanguage === language) return;

    let texts: ResultTexts | null = textsByLanguage[language] || null;
    if (!texts) {
      if (loadingOverlay) loadingOverlay.classList.add("safely-visible");
      if (tabsArea) tabsArea.classList.add("safely-loading-blur");
      texts = await (window as any).__safelyAPI.getResultTexts(analysisId, language);
      if (loadingOverlay) loadingOverlay.classList.remove("safely-visible");
      if (tabsArea) tabsArea.classList.remove("safely-loading-blur");
      if (!texts) return;
      textsByLanguage[language] = texts;
    }

    // Still the same result, and still the language that was asked for?
    const now = (window as any).__safelyData;
    const i18n = (window as any).__safelyI18n;
    if (!now || now.analysisId !== analysisId) return;
    if (i18n && i18n.getLang() !== language) return;

    const signals = now.signals || [];
    const factors = now.riskFactors || [];
    if (texts.subs.length !== signals.length || texts.descriptions.length !== factors.length) {
      return;
    }
    signals.forEach((s: any, i: number) => {
      s.sub = texts!.subs[i];
    });
    factors.forEach((f: any, i: number) => {
      f.description = texts!.descriptions[i];
    });
    now.textLanguage = language;
    window.dispatchEvent(new CustomEvent("safely-result-text-changed"));
  }

  // The language changed (EN / PT button, here or in another tab):
  // redraw every fixed text, then bring the result text along. The
  // tabs redraw themselves.
  window.addEventListener("safely-lang-changed", () => {
    applyStaticTexts();
    if (lastUsage) showUsageLine();
    const i18n = (window as any).__safelyI18n;
    if (i18n) showResultIn(i18n.getLang());
  });

  // ── Initial check, then keep checking on every URL change ──
  applyStaticTexts();
  updateSupportState();
  let lastUrl = window.location.href;
  new MutationObserver(() => {
    const currentUrl = window.location.href;
    if (currentUrl !== lastUrl) {
      lastUrl = currentUrl;
      updateSupportState();
    }
  }).observe(document.body, { subtree: true, childList: true });
})();
