// panel.js — toolbar, panel shell, tab registration — the DOM/UI engine
(async function () {
  "use strict";
  if (document.getElementById("safely-root")) return;

  var panelVisible = false;
  var toolbarExpanded = false;
  var currentTab = "";
  var collapseTimer;
  var intentionallyClosed = false;
  var isCurrentlySupported = null; // null = not yet determined
  var tabsHaveBeenBuilt = false;
  var pendingTabRegistrations = [];

  var tabIds = [];
  var tabTitles = {};

  // Base DOM Structure — the unsupported notice exists from the start
  // as its own permanent piece of the panel, separate from tabsArea
  // (where the 3 real tabs get built) - this way there's no possibility
  // of the real tabs ever appearing alongside it by accident.
  var root = document.createElement("div");
  root.id = "safely-root";
  root.innerHTML =
    '<div id="safely-panel">' +
    '<div class="safely-panel-header">' +
    '<span class="safely-panel-title" id="safely-panel-title">Safely</span>' +
    '<div class="safely-close-btn" id="safely-close-btn">\u00d7</div>' +
    "</div>" +
    '<div class="safely-tabs-area" id="safely-tabs-area"></div>' +
    '<div class="safely-loading-overlay" id="safely-loading-overlay"><div class="safely-loading-dots"><span></span><span></span><span></span></div></div>' +
    '<div class="safely-tab-content" id="safely-tab-unsupported" style="display:none; padding: 20px; font-size: 13px; line-height: 1.5; color: #8a8a93;">' +
    "Safely isn't reading this page — it only activates on an actual " +
    "listing (like a specific item on OLX or a Facebook Marketplace " +
    "listing page), not a site's general pages." +
    "</div>" +
    '<div class="safely-tab-content" id="safely-tab-signin-required" style="display:none; padding: 20px; text-align: center;">' +
    '<div style="font-size:13px; line-height:1.6; color:#8a8a93; margin-bottom:16px;">' +
    "Sign in to Safely to analyze this listing. It only takes a moment, " +
    "and your risk history stays saved to your account." +
    "</div>" +
    '<a href="' +
    window.__safelyAPI.SITE_BASE +
    '" target="_blank" class="safely-signin-required-btn">Sign in to Safely</a>' +
    "</div>" +
    "</div>" +
    '<div id="safely-toolbar"><img class="safely-toolbar-letter" src="' +
    chrome.runtime.getURL("icons/icon48.png") +
    '" alt="Safely" /><div class="safely-toolbar-inner" id="safely-toolbar-inner">' +
    '<span class="safely-toolbar-label" id="safely-collapse-btn">Safely</span>' +
    "</div></div>";

  document.body.appendChild(root);
  window.__safelyRoot = root;

  var panel = document.getElementById("safely-panel");
  var toolbar = document.getElementById("safely-toolbar");
  var collapseBtn = document.getElementById("safely-collapse-btn");
  var panelTitle = document.getElementById("safely-panel-title");
  var closeBtn = document.getElementById("safely-close-btn");
  var tabsArea = document.getElementById("safely-tabs-area");
  var loadingOverlay = document.getElementById("safely-loading-overlay");
  var toolbarInner = document.getElementById("safely-toolbar-inner");
  var unsupportedContent = document.getElementById("safely-tab-unsupported");
  var signinRequiredContent = document.getElementById(
    "safely-tab-signin-required",
  );

  // Domain check lives entirely inside services/signals.rs on the
  // backend now, appearing as one more entry in the Intelligence tab's
  // listing signals - not shown separately here.

  // ── Reserve icon positions: the 3 real tabs PLUS one dedicated
  // "unsupported" icon, kept as separate slots so exactly one relevant
  // set is ever visible at a time - never a mix of both. ──
  var TAB_ORDER = ["risk", "intelligence", "protect"];
  var iconSlots = {};

  TAB_ORDER.forEach(function (id) {
    var iconDiv = document.createElement("div");
    iconDiv.className = "safely-toolbar-icon";
    iconDiv.dataset.open = id;
    iconDiv.style.display = "none";
    toolbarInner.insertBefore(iconDiv, collapseBtn);
    iconDiv.addEventListener("click", function (e) {
      e.stopPropagation();
      togglePanel(id);
    });
    iconSlots[id] = iconDiv;
  });

  var unsupportedIcon = document.createElement("div");
  unsupportedIcon.className = "safely-toolbar-icon";
  unsupportedIcon.dataset.open = "unsupported";
  unsupportedIcon.title = "Not a listing page";
  unsupportedIcon.style.display = "none";
  unsupportedIcon.innerHTML =
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="8" x2="12" y2="12"></line><line x1="12" y1="16" x2="12.01" y2="16"></line></svg>';
  toolbarInner.insertBefore(unsupportedIcon, collapseBtn);
  unsupportedIcon.addEventListener("click", function (e) {
    e.stopPropagation();
    togglePanel("unsupported");
  });

  var signinRequiredIcon = document.createElement("div");
  signinRequiredIcon.className = "safely-toolbar-icon";
  signinRequiredIcon.dataset.open = "signin-required";
  signinRequiredIcon.title = "Sign in required";
  signinRequiredIcon.style.display = "none";
  signinRequiredIcon.innerHTML =
    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h4a2 2 0 012 2v14a2 2 0 01-2 2h-4"></path><polyline points="10 17 15 12 10 7"></polyline><line x1="15" y1="12" x2="3" y2="12"></line></svg>';
  toolbarInner.insertBefore(signinRequiredIcon, collapseBtn);
  signinRequiredIcon.addEventListener("click", function (e) {
    e.stopPropagation();
    togglePanel("signin-required");
  });

  function switchTab(tab) {
    currentTab = tab;
    if (tab === "unsupported" || tab === "signin-required") {
      panelTitle.textContent = "Safely";
      tabIds.forEach(function (id) {
        var el = document.getElementById("safely-tab-" + id);
        if (el) el.style.display = "none";
      });
      unsupportedContent.style.display =
        tab === "unsupported" ? "block" : "none";
      signinRequiredContent.style.display =
        tab === "signin-required" ? "block" : "none";
    } else {
      panelTitle.textContent = tabTitles[tab] || tab;
      unsupportedContent.style.display = "none";
      signinRequiredContent.style.display = "none";
      tabIds.forEach(function (id) {
        var el = document.getElementById("safely-tab-" + id);
        if (el) el.style.display = id === tab ? "block" : "none";
      });
    }
    if (tabsArea) tabsArea.scrollTop = 0;
  }

  function togglePanel(tab) {
    if (panelVisible && currentTab === tab) {
      panelVisible = false;
      panel.classList.remove("safely-visible");
    } else {
      switchTab(tab);
      panelVisible = true;
      panel.classList.add("safely-visible");
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

  // The REAL tab-building logic, exactly as before - this only ever
  // actually runs once we're certain we're on a genuine listing.
  function reallyAddTab(id, title, html, iconSvg, initFn) {
    tabIds.push(id);
    tabTitles[id] = title;

    var tabDiv = document.createElement("div");
    tabDiv.className = "safely-tab-content";
    tabDiv.id = "safely-tab-" + id;
    tabDiv.style.display = "none";
    tabDiv.innerHTML = html;
    tabsArea.appendChild(tabDiv);

    var iconDiv = iconSlots[id];
    if (iconDiv) {
      iconDiv.title = title;
      iconDiv.innerHTML = iconSvg;
      iconDiv.style.display = "flex";
    }

    if (id === "risk") switchTab(id);
    if (typeof initFn === "function") initFn(root);
  }

  // Until we know for certain this is a real listing page, calls from
  // tabs/risk.js, tabs/intelligence.js, and tabs/protect.js (which run
  // and self-register on every page regardless, per the manifest) are
  // only QUEUED, never actually built into the DOM. This is what
  // guarantees the three real tabs can never appear - even briefly, even
  // due to a timing quirk - on a page that isn't a listing at all.
  window.__safelyAddTab = function (id, title, html, iconSvg, initFn) {
    pendingTabRegistrations.push({
      id: id,
      title: title,
      html: html,
      iconSvg: iconSvg,
      initFn: initFn,
    });
  };

  function buildQueuedTabsIfNeeded() {
    if (tabsHaveBeenBuilt) return;
    tabsHaveBeenBuilt = true;
    window.__safelyAddTab = reallyAddTab;
    pendingTabRegistrations.forEach(function (t) {
      reallyAddTab(t.id, t.title, t.html, t.iconSvg, t.initFn);
    });
    pendingTabRegistrations = [];
  }

  // Global helper to stop inputs from bubbling to the host page
  window.__safelyPreventInputBubbling = function () {
    root.querySelectorAll("input, textarea, select").forEach(function (el) {
      ["keydown", "keyup", "keypress"].forEach(function (evt) {
        el.removeEventListener(
          evt,
          function (e) {
            e.stopImmediatePropagation();
          },
          true,
        );
        el.addEventListener(
          evt,
          function (e) {
            e.stopImmediatePropagation();
          },
          true,
        );
      });
    });
  };

  // ── Switches between "real listing" and "not supported" - can be
  // called repeatedly as navigation happens, which is what makes this
  // work correctly on single-page-app sites without ever needing a
  // manual refresh. ──
  function updateSupportState() {
    var supported = window.__safelyScrapers.isListingPage();
    isCurrentlySupported = supported;

    if (supported) {
      unsupportedIcon.style.display = "none";

      // Real, chargeable AI analysis only ever runs for a signed-in
      // person - checking this here, before fetchAnalysis is ever
      // called, means an anonymous visitor never triggers a real
      // Claude API call at all. chrome.storage is what auth-bridge.js
      // (running on safely.sh) fills in whenever someone is actually
      // logged in there.
      chrome.storage.local.get("safely_session_token", function (result) {
        if (result.safely_session_token) {
          signinRequiredIcon.style.display = "none";
          buildQueuedTabsIfNeeded();
          TAB_ORDER.forEach(function (id) {
            if (iconSlots[id] && tabTitles[id]) {
              iconSlots[id].style.display = "flex";
            }
          });
          if (tabIds.indexOf("risk") !== -1) switchTab("risk");
          window.__safelyResetState();
          if (loadingOverlay) loadingOverlay.classList.add("safely-visible");
          if (tabsArea) tabsArea.classList.add("safely-loading-blur");
          window.__safelyAPI.fetchAnalysis();
        } else {
          TAB_ORDER.forEach(function (id) {
            if (iconSlots[id]) iconSlots[id].style.display = "none";
          });
          signinRequiredIcon.style.display = "flex";
          switchTab("signin-required");
        }
      });
    } else {
      TAB_ORDER.forEach(function (id) {
        if (iconSlots[id]) iconSlots[id].style.display = "none";
      });
      signinRequiredIcon.style.display = "none";
      unsupportedIcon.style.display = "flex";
      switchTab("unsupported");
    }
  }

  // ── Toolbar Hover Events ──
  toolbar.addEventListener("mouseenter", function () {
    clearTimeout(collapseTimer);
    if (intentionallyClosed) return;
    toolbarExpanded = true;
    toolbar.classList.add("safely-toolbar-expanded");
  });

  toolbar.addEventListener("mouseleave", function (e) {
    if (intentionallyClosed) return;
    if (e.relatedTarget && panel.contains(e.relatedTarget)) return;
    collapseTimer = setTimeout(collapseToolbar, 200);
  });

  panel.addEventListener("mouseenter", function () {
    clearTimeout(collapseTimer);
  });

  panel.addEventListener("mouseleave", function (e) {
    if (e.relatedTarget && toolbar.contains(e.relatedTarget)) return;
    collapseTimer = setTimeout(collapseToolbar, 200);
  });

  toolbar.addEventListener("click", function (e) {
    e.stopPropagation();
    if (intentionallyClosed) {
      intentionallyClosed = false;
      toolbarExpanded = true;
      toolbar.classList.add("safely-toolbar-expanded");
    }
  });

  // Clicking "Safely" label collapses and locks until mouse leaves
  collapseBtn.addEventListener("click", function (e) {
    e.stopPropagation();
    intentionallyClosed = true;
    collapseToolbar();
  });

  closeBtn.addEventListener("click", function (e) {
    e.stopPropagation();
    closePanel();
  });

  // Clicking outside closes panel and collapses toolbar (with lock)
  document.addEventListener("click", function (e) {
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

  // Once the mouse fully leaves the root, unlock hover-expand
  root.addEventListener("mouseleave", function () {
    if (intentionallyClosed) {
      setTimeout(function () {
        intentionallyClosed = false;
      }, 150);
    }
  });

  // ── Hides the loading overlay the moment fresh analysis data has
  // actually arrived and been rendered - the same event api.js already
  // dispatches for the tabs themselves to redraw, so this stays in
  // lockstep with real data being ready rather than a guessed delay. ──
  window.addEventListener("safely-data-ready", function () {
    if (loadingOverlay) loadingOverlay.classList.remove("safely-visible");
    if (tabsArea) tabsArea.classList.remove("safely-loading-blur");
  });

  // If someone signs in on a separate tab while this panel is showing
  // "sign in required," this picks that up the moment auth-bridge.js
  // relays the new token - no refresh needed on this tab at all.
  chrome.storage.onChanged.addListener(function (changes, area) {
    if (
      area === "local" &&
      changes.safely_session_token &&
      currentTab === "signin-required"
    ) {
      updateSupportState();
    }
  });

  // ── Initial check, then keep checking on every URL change ──
  updateSupportState();

  var lastUrl = window.location.href;
  new MutationObserver(function () {
    var currentUrl = window.location.href;
    if (currentUrl !== lastUrl) {
      lastUrl = currentUrl;
      updateSupportState();
    }
  }).observe(document.body, { subtree: true, childList: true });
})();
