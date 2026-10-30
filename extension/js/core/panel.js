"use strict";
(async function () {
    "use strict";
    if (document.getElementById("safely-root"))
        return;
    // ── Fonts ──
    // Some sites block fonts from outside (their security rules), which
    // made the panel fall back to the site's own font. The fonts ship
    // inside the extension and are handed to the page as data, which
    // those rules don't block. Names start with "Safely" so they never
    // clash with a site's own fonts.
    const FONTS = [
        ["Safely Sans", "fonts/inter-400.woff2", "400"],
        ["Safely Sans", "fonts/inter-500.woff2", "500"],
        ["Safely Sans", "fonts/inter-600.woff2", "600"],
        ["Safely Sans", "fonts/inter-700.woff2", "700"],
        ["Safely Sans", "fonts/inter-800.woff2", "800"],
        ["Safely Display", "fonts/gasoek-one.woff2", "400"],
    ];
    FONTS.forEach(([family, file, weight]) => {
        fetch(chrome.runtime.getURL(file))
            .then((res) => res.arrayBuffer())
            .then((data) => new FontFace(family, data, { weight, display: "swap" }).load())
            .then((face) => document.fonts.add(face))
            .catch((e) => console.warn("Safely: could not load " + file, e));
    });
    // Translates through i18n.ts; plain English if it isn't loaded.
    function tr(en, vars) {
        const i18n = window.__safelyI18n;
        if (i18n)
            return i18n.t(en, vars);
        if (!vars)
            return en;
        return en.replace(/\{(\w+)\}/g, (whole, key) => Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole);
    }
    const UNSUPPORTED_TEXT = "Safely doesn't check this page — open a listing on a supported marketplace to scan it.";
    const SIGNIN_TEXT = "Sign in free to analyze this listing. You get 5 free scans every month — no credit card needed.";
    let panelVisible = false;
    let toolbarExpanded = false;
    let currentTab = "";
    let collapseTimer;
    let intentionallyClosed = false;
    let tabsHaveBeenBuilt = false;
    let pendingTabRegistrations = [];
    const tabIds = [];
    const tabTitles = {};
    // Redraws the current "couldn't analyze" / "scan limit" message when
    // the language changes. Null when no such message is showing.
    let renderFailedMessage = null;
    let renderScanLimitMessage = null;
    // The scraper's own "not supported" message, when it gave one.
    let unsupportedCustomMessage = null;
    // Small line icons for the sign-in / limit / error / not-a-listing
    // messages. The same drawings are used for the toolbar buttons.
    const ICON_INFO = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="8" x2="12" y2="12"></line><line x1="12" y1="16" x2="12.01" y2="16"></line></svg>';
    const ICON_SIGNIN = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h4a2 2 0 012 2v14a2 2 0 01-2 2h-4"></path><polyline points="10 17 15 12 10 7"></polyline><line x1="15" y1="12" x2="3" y2="12"></line></svg>';
    const ICON_CLOCK = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><path d="M12 6v6l4 2"></path></svg>';
    const ICON_WARNING = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>';
    // The same icon, larger, inside the message.
    function stateIcon(svg) {
        return ('<div class="safely-state-icon">' +
            svg.replace(/width="16" height="16"/, 'width="22" height="22"') +
            "</div>");
    }
    // Base DOM Structure — the unsupported notice exists from the start
    // as its own permanent piece of the panel, separate from tabsArea
    // (where the real tabs get built) - this way there's no possibility
    // of the real tabs ever appearing alongside it by accident.
    const root = document.createElement("div");
    root.id = "safely-root";
    root.innerHTML =
        '<div id="safely-panel">' +
            '<div class="safely-panel-header">' +
            '<img class="safely-panel-logo" src="' +
            chrome.runtime.getURL("icons/icon48.png") +
            '" alt="" />' +
            '<span class="safely-panel-title">Safely</span>' +
            '<span class="safely-usage-line" id="safely-usage-line" style="display:none"></span>' +
            '<button type="button" class="safely-lang-btn" id="safely-lang-btn"></button>' +
            '<div class="safely-close-btn" id="safely-close-btn">' +
            '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg>' +
            "</div>" +
            "</div>" +
            '<div class="safely-panel-tabs" id="safely-panel-tabs" style="display:none"></div>' +
            '<div class="safely-checking" id="safely-checking"></div>' +
            '<div class="safely-tabs-area" id="safely-tabs-area"></div>' +
            '<div class="safely-loading-overlay" id="safely-loading-overlay"><div class="safely-loading-dots"><span></span><span></span><span></span></div></div>' +
            '<div class="safely-tab-content safely-state" id="safely-tab-unsupported" style="display:none">' +
            stateIcon(ICON_INFO) +
            '<div class="safely-state-title" id="safely-unsupported-title"></div>' +
            '<div class="safely-state-text" id="safely-unsupported-message"></div>' +
            "</div>" +
            '<div class="safely-tab-content safely-state" id="safely-tab-signin-required" style="display:none">' +
            stateIcon(ICON_SIGNIN) +
            '<div class="safely-state-title" id="safely-signin-title"></div>' +
            '<div class="safely-state-text" id="safely-signin-text"></div>' +
            '<button type="button" data-safely-url="' +
            window.__safelyAPI.SITE_BASE +
            '/" class="safely-btn safely-btn-dark safely-btn-full" id="safely-signin-btn"></button>' +
            "</div>" +
            '<div class="safely-tab-content safely-state" id="safely-tab-analysis-failed" style="display:none">' +
            stateIcon(ICON_WARNING) +
            '<div class="safely-state-title" id="safely-failed-title"></div>' +
            '<div class="safely-state-text" id="safely-failed-message"></div>' +
            '<button class="safely-retry-btn safely-btn safely-btn-line safely-btn-full" id="safely-retry-btn">' +
            '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 4 1 10 7 10"></polyline><path d="M3.51 15a9 9 0 102.13-9.36L1 10"></path></svg>' +
            '<span id="safely-retry-text"></span>' +
            "</button>" +
            "</div>" +
            '<div class="safely-tab-content safely-state" id="safely-tab-scan-limit-reached" style="display:none">' +
            stateIcon(ICON_CLOCK) +
            '<div class="safely-state-title" id="safely-scan-limit-title"></div>' +
            '<div class="safely-state-text" id="safely-scan-limit-message"></div>' +
            '<button type="button" data-safely-url="' +
            window.__safelyAPI.SITE_BASE +
            '/dashboard/?manage_billing=1" class="safely-btn safely-btn-dark safely-btn-full" id="safely-see-plans-btn"></button>' +
            "</div>" +
            "</div>" +
            '<div id="safely-toolbar"><img class="safely-toolbar-letter" src="' +
            chrome.runtime.getURL("icons/icon48.png") +
            '" alt="Safely" /><div class="safely-toolbar-inner" id="safely-toolbar-inner">' +
            '<span class="safely-toolbar-label" id="safely-collapse-btn">Safely</span>' +
            "</div></div>" +
            '<span class="safely-status-dot" id="safely-status-dot" aria-hidden="true"></span>';
    document.body.appendChild(root);
    window.__safelyRoot = root;
    const panel = document.getElementById("safely-panel");
    const toolbar = document.getElementById("safely-toolbar");
    const statusDot = document.getElementById("safely-status-dot");
    let dotState = "";
    function setStatusDot(state) {
        dotState = state;
        statusDot.className = "safely-status-dot" + (state ? " safely-dot-" + state : "");
        toolbar.title =
            state === "run"
                ? tr("Checking this supplier")
                : state === "done"
                    ? tr("Result ready")
                    : "";
    }
    const collapseBtn = document.getElementById("safely-collapse-btn");
    const panelTabs = document.getElementById("safely-panel-tabs");
    const checkingView = document.getElementById("safely-checking");
    const closeBtn = document.getElementById("safely-close-btn");
    const langBtn = document.getElementById("safely-lang-btn");
    const tabsArea = document.getElementById("safely-tabs-area");
    const loadingOverlay = document.getElementById("safely-loading-overlay");
    const toolbarInner = document.getElementById("safely-toolbar-inner");
    const unsupportedContent = document.getElementById("safely-tab-unsupported");
    const unsupportedMessageEl = document.getElementById("safely-unsupported-message");
    const signinRequiredContent = document.getElementById("safely-tab-signin-required");
    const signinText = document.getElementById("safely-signin-text");
    const signinBtn = document.getElementById("safely-signin-btn");
    const analysisFailedContent = document.getElementById("safely-tab-analysis-failed");
    const usageLine = document.getElementById("safely-usage-line");
    const scanLimitReachedContent = document.getElementById("safely-tab-scan-limit-reached");
    const scanLimitMessage = document.getElementById("safely-scan-limit-message");
    const seePlansBtn = document.getElementById("safely-see-plans-btn");
    const failedMessage = document.getElementById("safely-failed-message");
    const retryBtn = document.getElementById("safely-retry-btn");
    const retryText = document.getElementById("safely-retry-text");
    const stateTitles = {
        "safely-unsupported-title": "Open a supplier listing",
        "safely-signin-title": "Sign in to check suppliers",
        "safely-failed-title": "This check didn't finish",
        "safely-scan-limit-title": "You've used this month's scans",
    };
    // "Sign in free" and "See plans" are buttons, not links, and open
    // Safely's page themselves. Some sites (Alibaba) add their own
    // tracking to every link on the page when it is clicked, which turned
    // the address into "https://safely.sh/?spm=a2700.details...". A
    // button is never changed by the site, so the plain address opens.
    [signinBtn, seePlansBtn].forEach((btn) => {
        if (!btn)
            return;
        btn.addEventListener("click", (e) => {
            e.preventDefault();
            e.stopPropagation();
            const url = btn.dataset.safelyUrl;
            if (url)
                window.open(url, "_blank", "noopener");
        });
    });
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
            const i18n = window.__safelyI18n;
            if (!i18n)
                return;
            i18n.setLang(i18n.getLang() === "pt-br" ? "en" : "pt-br");
        });
    }
    // ── Reserve icon positions: the 3 real tabs PLUS one dedicated
    // "unsupported" icon, kept as separate slots so exactly one relevant
    // set is ever visible at a time. ──
    const TAB_ORDER = ["risk", "intelligence", "protect"];
    const iconSlots = {};
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
    unsupportedIcon.innerHTML = ICON_INFO;
    toolbarInner.insertBefore(unsupportedIcon, collapseBtn);
    unsupportedIcon.addEventListener("click", (e) => {
        e.stopPropagation();
        togglePanel("unsupported");
    });
    const signinRequiredIcon = document.createElement("div");
    signinRequiredIcon.className = "safely-toolbar-icon";
    signinRequiredIcon.dataset.open = "signin-required";
    signinRequiredIcon.style.display = "none";
    signinRequiredIcon.innerHTML = ICON_SIGNIN;
    toolbarInner.insertBefore(signinRequiredIcon, collapseBtn);
    signinRequiredIcon.addEventListener("click", (e) => {
        e.stopPropagation();
        togglePanel("signin-required");
    });
    const scanLimitReachedIcon = document.createElement("div");
    scanLimitReachedIcon.className = "safely-toolbar-icon";
    scanLimitReachedIcon.dataset.open = "scan-limit-reached";
    scanLimitReachedIcon.style.display = "none";
    scanLimitReachedIcon.innerHTML = ICON_CLOCK;
    toolbarInner.insertBefore(scanLimitReachedIcon, collapseBtn);
    scanLimitReachedIcon.addEventListener("click", (e) => {
        e.stopPropagation();
        togglePanel("scan-limit-reached");
    });
    const analysisFailedIcon = document.createElement("div");
    analysisFailedIcon.className = "safely-toolbar-icon";
    analysisFailedIcon.dataset.open = "analysis-failed";
    analysisFailedIcon.style.display = "none";
    analysisFailedIcon.innerHTML = ICON_WARNING;
    toolbarInner.insertBefore(analysisFailedIcon, collapseBtn);
    analysisFailedIcon.addEventListener("click", (e) => {
        e.stopPropagation();
        togglePanel("analysis-failed");
    });
    const SPECIAL_STATES = ["unsupported", "signin-required", "analysis-failed", "scan-limit-reached"];
    // Every fixed text in the panel, in the current language. Runs once
    // now and again whenever the language changes.
    function applyStaticTexts() {
        const i18n = window.__safelyI18n;
        if (langBtn) {
            if (i18n) {
                langBtn.textContent = i18n.getLang() === "pt-br" ? "PT" : "EN";
                langBtn.title = tr("Change language");
            }
            else {
                langBtn.style.display = "none";
            }
        }
        if (unsupportedMessageEl) {
            unsupportedMessageEl.textContent = tr(unsupportedCustomMessage || UNSUPPORTED_TEXT);
        }
        if (signinText)
            signinText.textContent = tr(SIGNIN_TEXT);
        if (signinBtn)
            signinBtn.textContent = tr("Sign in free");
        if (retryText)
            retryText.textContent = tr("Reload");
        if (seePlansBtn)
            seePlansBtn.textContent = tr("See plans");
        if (scanLimitMessage && !renderScanLimitMessage) {
            scanLimitMessage.textContent = tr("You've used all your scans for this month.");
        }
        unsupportedIcon.title = tr("Not a listing page");
        signinRequiredIcon.title = tr("Sign in required");
        scanLimitReachedIcon.title = tr("Scan limit reached");
        analysisFailedIcon.title = tr("Couldn't analyze this listing");
        tabIds.forEach((id) => {
            if (iconSlots[id])
                iconSlots[id].title = tr(tabTitles[id] || id);
        });
        Object.keys(stateTitles).forEach((id) => {
            const el = document.getElementById(id);
            if (el)
                el.textContent = tr(stateTitles[id]);
        });
        panelTabs.querySelectorAll("[data-tab]").forEach((btn) => {
            btn.textContent = tr(tabTitles[btn.dataset.tab || ""] || "");
        });
        if (checking)
            renderChecking();
        if (renderFailedMessage)
            renderFailedMessage();
        if (renderScanLimitMessage)
            renderScanLimitMessage();
    }
    function switchTab(tab) {
        currentTab = tab;
        panelTabs.querySelectorAll("[data-tab]").forEach((btn) => {
            btn.classList.toggle("safely-active", btn.dataset.tab === tab);
        });
        if (SPECIAL_STATES.indexOf(tab) !== -1) {
            stopChecking();
            panelTabs.style.display = "none";
            tabIds.forEach((id) => {
                const el = document.getElementById("safely-tab-" + id);
                if (el)
                    el.style.display = "none";
            });
            unsupportedContent.style.display = tab === "unsupported" ? "block" : "none";
            signinRequiredContent.style.display = tab === "signin-required" ? "block" : "none";
            analysisFailedContent.style.display = tab === "analysis-failed" ? "block" : "none";
            scanLimitReachedContent.style.display = tab === "scan-limit-reached" ? "block" : "none";
        }
        else {
            if (!checking)
                panelTabs.style.display = "flex";
            unsupportedContent.style.display = "none";
            signinRequiredContent.style.display = "none";
            analysisFailedContent.style.display = "none";
            scanLimitReachedContent.style.display = "none";
            tabIds.forEach((id) => {
                const el = document.getElementById("safely-tab-" + id);
                if (el)
                    el.style.display = id === tab ? "block" : "none";
            });
        }
        playEnter(document.getElementById("safely-tab-" + tab));
        if (tabsArea)
            tabsArea.scrollTop = 0;
    }
    // A short fade-in for whatever just appeared.
    function playEnter(el) {
        if (!el)
            return;
        el.classList.remove("safely-enter");
        void el.offsetWidth;
        el.classList.add("safely-enter");
    }
    function togglePanel(tab) {
        if (panelVisible && currentTab === tab) {
            panelVisible = false;
            panel.classList.remove("safely-visible");
        }
        else {
            switchTab(tab);
            panelVisible = true;
            panel.classList.add("safely-visible");
            if (dotState === "done")
                setStatusDot("");
        }
    }
    function closePanel() {
        panelVisible = false;
        panel.classList.remove("safely-visible");
    }
    function collapseToolbar() {
        toolbarExpanded = false;
        panelVisible = false;
        toolbar.classList.remove("safely-toolbar-expanded");
        panel.classList.remove("safely-visible");
    }
    // The REAL tab-building logic - this only ever actually runs once
    // we're certain we're on a genuine listing. The title is kept in
    // English and translated when shown.
    function reallyAddTab(id, title, html, iconSvg, initFn) {
        tabIds.push(id);
        tabTitles[id] = title;
        const tabDiv = document.createElement("div");
        tabDiv.className = "safely-tab-content";
        tabDiv.id = "safely-tab-" + id;
        tabDiv.style.display = "none";
        tabDiv.innerHTML = html;
        tabsArea.appendChild(tabDiv);
        const tabBtn = document.createElement("button");
        tabBtn.type = "button";
        tabBtn.className = "safely-panel-tab";
        tabBtn.dataset.tab = id;
        tabBtn.textContent = tr(title);
        tabBtn.addEventListener("click", (e) => {
            e.stopPropagation();
            switchTab(id);
        });
        // Keep the bar in the same order as the toolbar icons.
        const later = Array.prototype.slice
            .call(panelTabs.children)
            .find((b) => TAB_ORDER.indexOf(b.dataset.tab || "") > TAB_ORDER.indexOf(id));
        panelTabs.insertBefore(tabBtn, later || null);
        const iconDiv = iconSlots[id];
        if (iconDiv) {
            iconDiv.title = tr(title);
            iconDiv.innerHTML = iconSvg;
            iconDiv.style.display = "flex";
        }
        if (id === "risk")
            switchTab(id);
        if (typeof initFn === "function")
            initFn(root);
    }
    // Until we know for certain this is a real listing page, calls from
    // tabs/risk.js, tabs/intelligence.js, and tabs/protect.js are only
    // QUEUED, never actually built into the DOM. This guarantees the
    // three real tabs can never appear on a page that isn't a listing.
    window.__safelyAddTab = function (id, title, html, iconSvg, initFn) {
        pendingTabRegistrations.push({ id, title, html, iconSvg, initFn });
    };
    function buildQueuedTabsIfNeeded() {
        if (tabsHaveBeenBuilt)
            return;
        tabsHaveBeenBuilt = true;
        window.__safelyAddTab = reallyAddTab;
        pendingTabRegistrations.forEach((t) => {
            reallyAddTab(t.id, t.title, t.html, t.iconSvg, t.initFn);
        });
        pendingTabRegistrations = [];
    }
    // Global helper to stop inputs from bubbling to the host page.
    window.__safelyPreventInputBubbling = function () {
        root.querySelectorAll("input, textarea, select").forEach((el) => {
            ["keydown", "keyup", "keypress"].forEach((evt) => {
                const stop = (e) => e.stopImmediatePropagation();
                el.removeEventListener(evt, stop, true);
                el.addEventListener(evt, stop, true);
            });
        });
    };
    // ── Switches between "real listing" and "not supported" - can be
    // called repeatedly as navigation happens. ──
    function updateSupportState() {
        const requestUrl = window.location.href;
        const isStillCurrentPage = () => window.location.href === requestUrl;
        const supported = window.__safelyScrapers.isListingPage();
        if (supported) {
            unsupportedIcon.style.display = "none";
            chrome.storage.local.get("safely_session_token", async (result) => {
                if (!isStillCurrentPage())
                    return;
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
                    if (tabIds.indexOf("risk") !== -1)
                        switchTab("risk");
                    window.__safelyResetState();
                    startChecking();
                    window.__safelyAPI.fetchAnalysis();
                }
                else {
                    TAB_ORDER.forEach((id) => {
                        if (iconSlots[id])
                            iconSlots[id].style.display = "none";
                    });
                    signinRequiredIcon.style.display = "flex";
                    analysisFailedIcon.style.display = "none";
                    scanLimitReachedIcon.style.display = "none";
                    if (usageLine)
                        usageLine.style.display = "none";
                    setStatusDot("");
                    switchTab("signin-required");
                }
            });
        }
        else {
            TAB_ORDER.forEach((id) => {
                if (iconSlots[id])
                    iconSlots[id].style.display = "none";
            });
            signinRequiredIcon.style.display = "none";
            analysisFailedIcon.style.display = "none";
            scanLimitReachedIcon.style.display = "none";
            unsupportedIcon.style.display = "flex";
            unsupportedCustomMessage =
                window.__safelyScrapers.getUnsupportedMessage?.() || null;
            if (unsupportedMessageEl) {
                unsupportedMessageEl.textContent = tr(unsupportedCustomMessage || UNSUPPORTED_TEXT);
            }
            setStatusDot("");
            switchTab("unsupported");
        }
    }
    // ── Waiting screen ──
    // While a scan runs, the checks Safely makes are listed one by one.
    // The server does them all at once and sends one answer, so the list
    // moves on a timer and the last line keeps spinning until the answer
    // comes. It never shows a check result before the real one arrives.
    const CHECK_STEPS = [
        "Reading the listing",
        "Checking the seller account",
        "Looking up the website",
        "Searching the web for reports",
        "Comparing the price",
        "Checking payment terms",
        "Working out the score",
    ];
    const STEP_EVERY_MS = 1400;
    let checking = false;
    let checkingStep = 0;
    let checkingTimer = null;
    let scanStartedAt = 0;
    // Builds the waiting screen once; showStep() then only moves the
    // highlight, so the spinner keeps turning smoothly.
    function renderChecking() {
        const r = 18;
        const arc = (2 * Math.PI * r * 0.3).toFixed(1);
        checkingView.innerHTML =
            '<div class="safely-wait-card">' +
                '<svg class="safely-wait-spin" viewBox="0 0 44 44"><circle cx="22" cy="22" r="' +
                r +
                '" fill="none" stroke="#2e2e33" stroke-width="4"/><circle cx="22" cy="22" r="' +
                r +
                '" fill="none" stroke="#ffb938" stroke-width="4" stroke-linecap="round" stroke-dasharray="' +
                arc +
                ' 200"/></svg>' +
                '<div><div class="safely-wait-now" id="safely-wait-now"></div>' +
                '<div class="safely-wait-sub">' +
                escapeHtml(tr("Checking this supplier. Usually a few seconds.")) +
                "</div></div></div>" +
                '<div class="safely-steps">' +
                CHECK_STEPS.map((name) => '<div class="safely-step"><span class="safely-step-dot"></span><span>' +
                    escapeHtml(tr(name)) +
                    "</span></div>").join("") +
                "</div>";
        showStep();
    }
    function showStep() {
        const nowEl = document.getElementById("safely-wait-now");
        if (nowEl)
            nowEl.textContent = tr(CHECK_STEPS[checkingStep]);
        checkingView.querySelectorAll(".safely-step").forEach((el, i) => {
            el.classList.toggle("safely-done", i < checkingStep);
            el.classList.toggle("safely-now", i === checkingStep);
            const dot = el.querySelector(".safely-step-dot");
            if (dot)
                dot.innerHTML = i < checkingStep ? SAFELY_CHECK_ICON : "";
        });
    }
    function startChecking() {
        stopChecking();
        checking = true;
        checkingStep = 0;
        scanStartedAt = Date.now();
        setStatusDot("run");
        panelTabs.style.display = "none";
        tabsArea.style.display = "none";
        checkingView.classList.add("safely-visible");
        renderChecking();
        checkingTimer = setInterval(() => {
            if (checkingStep < CHECK_STEPS.length - 1) {
                checkingStep += 1;
                showStep();
            }
        }, STEP_EVERY_MS);
    }
    function stopChecking() {
        if (checkingTimer)
            clearInterval(checkingTimer);
        checkingTimer = null;
        if (!checking)
            return;
        checking = false;
        checkingView.classList.remove("safely-visible");
        checkingView.innerHTML = "";
        tabsArea.style.display = "";
        if (SPECIAL_STATES.indexOf(currentTab) === -1 && tabIds.length > 0) {
            panelTabs.style.display = "flex";
        }
    }
    // ── Rows that open, "Show more" and ticks (used by both tabs) ──
    root.addEventListener("click", (e) => {
        const target = e.target;
        const tick = target.closest("[data-safely-tick]");
        if (tick) {
            const row = tick.closest(".safely-row");
            if (row) {
                const on = row.classList.toggle("safely-ticked");
                tick.setAttribute("aria-checked", on ? "true" : "false");
            }
            return;
        }
        const opener = target.closest("[data-safely-open]");
        if (opener) {
            const row = opener.closest(".safely-row");
            if (row) {
                const open = row.classList.toggle("safely-open");
                opener.setAttribute("aria-expanded", open ? "true" : "false");
            }
            return;
        }
        const more = target.closest("[data-safely-more]");
        if (more) {
            const list = more.closest(".safely-rows");
            if (!list)
                return;
            const all = list.classList.toggle("safely-show-all");
            const label = more.querySelector("span");
            if (label)
                label.textContent = all ? tr("Show less") : more.dataset.safelyMore || "";
        }
    });
    // ── Toolbar Hover Events ──
    toolbar.addEventListener("mouseenter", () => {
        clearTimeout(collapseTimer);
        if (intentionallyClosed)
            return;
        toolbarExpanded = true;
        toolbar.classList.add("safely-toolbar-expanded");
    });
    toolbar.addEventListener("mouseleave", (e) => {
        if (intentionallyClosed)
            return;
        if (e.relatedTarget && panel.contains(e.relatedTarget))
            return;
        collapseTimer = setTimeout(collapseToolbar, 200);
    });
    panel.addEventListener("mouseenter", () => {
        clearTimeout(collapseTimer);
    });
    panel.addEventListener("mouseleave", (e) => {
        if (e.relatedTarget && toolbar.contains(e.relatedTarget))
            return;
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
    document.addEventListener("click", (e) => {
        if (!root.contains(e.target)) {
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
    let rateLimitCountdownTimer = null;
    function formatCountdown(totalSeconds) {
        const minutes = Math.floor(totalSeconds / 60);
        const seconds = totalSeconds % 60;
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds;
    }
    globalThis.__safelyFormatCountdown = formatCountdown;
    function startRateLimitCountdown(seconds) {
        if (rateLimitCountdownTimer)
            clearInterval(rateLimitCountdownTimer);
        let remaining = seconds;
        function render() {
            if (failedMessage) {
                failedMessage.textContent =
                    remaining > 0
                        ? tr("You've checked several listings quickly. Try again in {time}.", {
                            time: formatCountdown(remaining),
                        })
                        : tr("You can check another listing now.");
            }
            if (retryBtn)
                retryBtn.style.display = remaining > 0 ? "none" : "flex";
        }
        renderFailedMessage = render;
        render();
        rateLimitCountdownTimer = setInterval(() => {
            remaining -= 1;
            if (remaining <= 0) {
                clearInterval(rateLimitCountdownTimer);
                rateLimitCountdownTimer = null;
                remaining = 0;
            }
            render();
        }, 1000);
    }
    // The last plan / usage the server sent, so the line can be redrawn
    // in another language without asking the server again.
    let lastUsage = null;
    function showUsageLine() {
        if (!usageLine)
            return;
        const text = globalThis.__safelyFormatUsageLine(lastUsage);
        usageLine.textContent = text;
        usageLine.style.display = text ? "inline" : "none";
    }
    async function refreshUsageLine() {
        if (!usageLine)
            return;
        lastUsage = await window.__safelyAPI.getScanUsage();
        showUsageLine();
    }
    // How long the scan took, shown at the top of the Risk tab. Runs
    // before the tabs redraw (this file loads before tabs/*.js).
    window.addEventListener("safely-data-ready", () => {
        const data = window.__safelyData;
        if (data && scanStartedAt) {
            data.scanSeconds = Math.max(1, Math.round((Date.now() - scanStartedAt) / 1000));
        }
    });
    window.addEventListener("safely-analysis-finished", (e) => {
        stopChecking();
        // Already looking at the panel: nothing new to point at.
        setStatusDot(panelVisible ? "" : "done");
        // Refresh the "63/5" scans-used line after every scan attempt.
        // It never blocks anything.
        refreshUsageLine();
        const reason = e.detail && e.detail.error;
        if (!reason)
            return; // success - tabs are already showing real data
        if (reason === "unauthorized") {
            signinRequiredIcon.style.display = "flex";
            TAB_ORDER.forEach((id) => {
                if (iconSlots[id])
                    iconSlots[id].style.display = "none";
            });
            switchTab("signin-required");
            return;
        }
        if (reason === "scan_limit_reached" || reason === "free_scan_limit_reached") {
            const scanLimit = e.detail.scanLimit;
            const resetsOn = e.detail.resetsOn;
            renderScanLimitMessage = () => {
                if (scanLimitMessage) {
                    scanLimitMessage.textContent = globalThis.__safelyScanLimitMessage(reason, scanLimit, resetsOn);
                }
            };
            renderScanLimitMessage();
            scanLimitReachedIcon.style.display = "flex";
            TAB_ORDER.forEach((id) => {
                if (iconSlots[id])
                    iconSlots[id].style.display = "none";
            });
            switchTab("scan-limit-reached");
            return;
        }
        if (reason === "rate_limited" && e.detail.retryAfterSeconds) {
            startRateLimitCountdown(e.detail.retryAfterSeconds);
        }
        else if (failedMessage) {
            renderFailedMessage = () => {
                if (failedMessage) {
                    failedMessage.textContent = tr("Couldn't analyze this listing right now. Please try again in a moment.");
                }
            };
            renderFailedMessage();
            if (retryBtn)
                retryBtn.style.display = "flex";
        }
        analysisFailedIcon.style.display = "flex";
        TAB_ORDER.forEach((id) => {
            if (iconSlots[id])
                iconSlots[id].style.display = "none";
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
    let textsForAnalysis = null;
    let textsByLanguage = {};
    function currentResultTexts(data) {
        return {
            subs: (data.signals || []).map((s) => s.sub || ""),
            descriptions: (data.riskFactors || []).map((f) => f.description || ""),
        };
    }
    async function showResultIn(language) {
        const data = window.__safelyData;
        if (!data || !data.analysisId)
            return;
        const analysisId = data.analysisId;
        const shownLanguage = data.textLanguage || "en";
        if (textsForAnalysis !== analysisId) {
            textsForAnalysis = analysisId;
            textsByLanguage = {};
        }
        if (!textsByLanguage[shownLanguage]) {
            textsByLanguage[shownLanguage] = currentResultTexts(data);
        }
        if (shownLanguage === language)
            return;
        let texts = textsByLanguage[language] || null;
        if (!texts) {
            if (loadingOverlay)
                loadingOverlay.classList.add("safely-visible");
            if (tabsArea)
                tabsArea.classList.add("safely-loading-blur");
            texts = await window.__safelyAPI.getResultTexts(analysisId, language);
            if (loadingOverlay)
                loadingOverlay.classList.remove("safely-visible");
            if (tabsArea)
                tabsArea.classList.remove("safely-loading-blur");
            if (!texts)
                return;
            textsByLanguage[language] = texts;
        }
        // Still the same result, and still the language that was asked for?
        const now = window.__safelyData;
        const i18n = window.__safelyI18n;
        if (!now || now.analysisId !== analysisId)
            return;
        if (i18n && i18n.getLang() !== language)
            return;
        const signals = now.signals || [];
        const factors = now.riskFactors || [];
        if (texts.subs.length !== signals.length || texts.descriptions.length !== factors.length) {
            return;
        }
        signals.forEach((s, i) => {
            s.sub = texts.subs[i];
        });
        factors.forEach((f, i) => {
            f.description = texts.descriptions[i];
        });
        now.textLanguage = language;
        window.dispatchEvent(new CustomEvent("safely-result-text-changed"));
    }
    // The language changed (EN / PT button, here or in another tab):
    // redraw every fixed text, then bring the result text along. The
    // tabs redraw themselves.
    window.addEventListener("safely-lang-changed", () => {
        applyStaticTexts();
        setStatusDot(dotState);
        if (lastUsage)
            showUsageLine();
        const i18n = window.__safelyI18n;
        if (i18n)
            showResultIn(i18n.getLang());
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
