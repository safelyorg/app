(function () {
  "use strict";
  if (document.getElementById("safely-root")) return;

  var panelVisible = false;
  var toolbarExpanded = false;
  var currentTab = "";
  var collapseTimer;
  var intentionallyClosed = false;

  var tabIds = [];
  var tabTitles = {};

  // Shared Data accessible by all tabs
  window.__safelyData = {
    riskScore: 22,
    seller: {
      name: "Ali Khan",
      handle: "@alikhan_93",
      accountAge: "3 years, 2 months",
      verification: "verified",
      totalDeals: 142,
      disputes: 0,
      completionRate: "96%",
      location: "Lahore, PK",
      lastActive: "2 hours ago",
      networkSummary: "Clean record on Safely network. No fraud reports found.",
      platforms: [
        { name: "Facebook", status: "Active · 3 yr", type: "active" },
        { name: "OLX", status: "Active · 1 yr", type: "active" },
        { name: "Instagram", status: "Not found", type: "none" },
        { name: "Safely network", status: "142 deals", type: "active" },
      ],
      monthlyActivity: [4, 6, 8, 9, 7, 11, 10, 5, 9, 8, 12, 3],
    },
    signals: [
      {
        label: "Price analysis",
        sub: "Rs. 85,000 vs market avg Rs. 88,000",
        value: "Normal range",
        type: "good",
      },
      {
        label: "Urgency language",
        sub: "Scanned listing text and captions",
        value: "None found",
        type: "good",
      },
      {
        label: "Advance payment request",
        sub: "Checked listing and comments",
        value: "Not detected",
        type: "neutral",
      },
      {
        label: "Account age",
        sub: "Cross-referenced with Safely records",
        value: "3 yr 2 mo",
        type: "info",
      },
      {
        label: "Duplicate listing",
        sub: "Checked across OLX and Facebook",
        value: "No duplicates",
        type: "good",
      },
      {
        label: "Image authenticity",
        sub: "AI-generated and stock photo check",
        value: "Original",
        type: "good",
      },
      {
        label: "Fraud pattern match",
        sub: "Checked against Safely scam database",
        value: "No match",
        type: "good",
      },
      {
        label: "Contact info in listing",
        sub: "Phone number detected in listing text",
        value: "Detected",
        type: "caution",
      },
    ],
  };

  // Base DOM Structure
  var root = document.createElement("div");
  root.id = "safely-root";
  root.innerHTML =
    '<div id="safely-panel">' +
    '<div class="safely-panel-header">' +
    '<span class="safely-panel-title" id="safely-panel-title">Safely</span>' +
    '<div class="safely-close-btn" id="safely-close-btn">\u00d7</div>' +
    "</div>" +
    '<div class="safely-tabs-area" id="safely-tabs-area"></div>' +
    "</div>" +
    '<div id="safely-toolbar"><span class="safely-toolbar-letter">S</span><div class="safely-toolbar-inner" id="safely-toolbar-inner">' +
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
  var toolbarInner = document.getElementById("safely-toolbar-inner");

  function switchTab(tab) {
    currentTab = tab;
    panelTitle.textContent = tabTitles[tab] || tab;
    tabIds.forEach(function (id) {
      var el = document.getElementById("safely-tab-" + id);
      if (el) el.style.display = id === tab ? "block" : "none";
    });
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

  // Global function for other files to register their tabs dynamically
  window.__safelyAddTab = function (id, title, html, iconSvg, initFn) {
    tabIds.push(id);
    tabTitles[id] = title;

    var tabDiv = document.createElement("div");
    tabDiv.className = "safely-tab-content";
    tabDiv.id = "safely-tab-" + id;
    tabDiv.style.display = "none";
    tabDiv.innerHTML = html;
    tabsArea.appendChild(tabDiv);

    var iconDiv = document.createElement("div");
    iconDiv.className = "safely-toolbar-icon";
    iconDiv.dataset.open = id;
    iconDiv.title = title;
    iconDiv.innerHTML = iconSvg;
    toolbarInner.insertBefore(iconDiv, collapseBtn);

    iconDiv.addEventListener("click", function (e) {
      e.stopPropagation();
      togglePanel(id);
    });

    // Automatically open the first tab that registers
    if (tabIds.length === 1) switchTab(id);
    if (typeof initFn === "function") initFn(root);
  };

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
})();
