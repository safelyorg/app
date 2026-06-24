(async function () {
  "use strict";
  if (document.getElementById("safely-root")) return;

  var panelVisible = false;
  var toolbarExpanded = false;
  var currentTab = "";
  var collapseTimer;
  var intentionallyClosed = false;

  var tabIds = [];
  var tabTitles = {};

  function detectPlatform() {
    var url = window.location.href;
    if (url.includes("olx.com.pk")) return "olx";
    if (url.includes("facebook.com")) return "facebook";
    return "unknown";
  }

  function scrapeOLX() {
    var data = {};

    // listing_id — extract from URL
    var urlMatch = window.location.href.match(/iid-(\d+)/);
    data.listing_id = urlMatch ? urlMatch[1] : null;

    // title
    var titleEl = document.querySelector("h1._75bce902");
    data.title = titleEl ? titleEl.innerText.trim() : null;

    // price — strip "Rs" and commas, convert to paisas
    var priceEl = document.querySelector("span._24469da7");
    if (priceEl) {
      var priceText = priceEl.innerText.trim();
      priceText = priceText.replace(/Rs\s*/i, "").replace(/,/g, "").trim();

      if (priceText.toLowerCase().includes("lac")) {
        var num = parseFloat(priceText.replace(/lac/i, "").trim());
        data.price = Math.round(num * 100000);
      } else {
        data.price = Math.round(parseFloat(priceText));
      }
    } else {
      data.price = null;
    }

    // description
    var descEl = document.querySelector("div._7a99ad24 span");
    data.description = descEl ? descEl.innerText.trim() : null;

    // seller name — navigate via "Posted by" label
    var postedByLabel = Array.from(
      document.querySelectorAll("span._9083bec6._1fcb6673"),
    ).find(function (el) {
      return el.innerText.trim() === "Posted by";
    });
    if (postedByLabel) {
      var nameContainer = postedByLabel.parentElement.nextElementSibling;
      var nameEl = nameContainer
        ? nameContainer.querySelector("span._8206696c.b7af14b4")
        : null;
      data.seller_name = nameEl ? nameEl.innerText.trim() : null;
    } else {
      data.seller_name = null;
    }

    // member since — find the span after "Member Since" label
    // member since
    var memberSinceLabel = Array.from(
      document.querySelectorAll("span._9083bec6._1fcb6673"),
    ).find(function (el) {
      return el.innerText.trim() === "Member Since";
    });
    if (memberSinceLabel) {
      var yearEl = memberSinceLabel.parentElement.querySelector(
        "span._8206696c.b7af14b4",
      );
      if (!yearEl) {
        yearEl = memberSinceLabel.nextElementSibling;
      }
      data.seller_join_date = yearEl
        ? "Member since " + yearEl.innerText.trim()
        : null;
    } else {
      data.seller_join_date = null;
    }

    // seller profile url and platform id
    var profileLink = document.querySelector("a.da952dfc");
    if (profileLink) {
      var href = profileLink.getAttribute("href");
      data.seller_profile_url = "https://www.olx.com.pk" + href;
      var profileMatch = href.match(/\/profile\/([^\/]+)/);
      data.seller_platform_id = profileMatch ? profileMatch[1] : null;
    } else {
      data.seller_profile_url = null;
      data.seller_platform_id = null;
    }

    return data;
  }

  window.__safelyData = {
    riskScore: 0,
    seller: {
      name: "Unknown",
      handle: "",
      accountAge: "Unknown",
      verification: "unknown",
      totalDeals: 0,
      disputes: 0,
      completionRate: "N/A",
      location: "",
      lastActive: "Unknown",
      networkSummary: "Could not connect to Safely server.",
      platforms: [],
      monthlyActivity: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    },
    signals: [],
  };

  async function fetchAnalysis() {
    var platform = detectPlatform();
    if (platform === "unknown") return;

    var listing_url = window.location.href;

    // scrape real data if on OLX
    var scraped = {};
    if (platform === "olx") {
      await new Promise(function (resolve) {
        setTimeout(resolve, 1500);
      });
      scraped = scrapeOLX();
    }

    try {
      var response = await fetch("http://localhost:3000/api/v1/analyze", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          platform: platform,
          listing_url: listing_url,
          seller_id: null,
          listing_id: scraped.listing_id || null,
          title: scraped.title || null,
          price: scraped.price || null,
          description: scraped.description || null,
          category: null,
          image_urls: null,
          posted_date: null,
          seller_platform_id: scraped.seller_platform_id || null,
          seller_name: scraped.seller_name || null,
          seller_handle: null,
          seller_phone: null,
          seller_profile_url: scraped.seller_profile_url || null,
          seller_join_date: scraped.seller_join_date || null,
          seller_location: null,
        }),
      });

      var rawText = await response.text();
      console.log("Safely: raw response status:", response.status);

      if (!response.ok) {
        console.error("Safely: backend error:", rawText);
        return;
      }

      var data = JSON.parse(rawText);

      window.__safelyData = {
        riskScore: data.risk_score,
        seller: {
          name: data.seller.name || "Unknown",
          handle: data.seller.handle || "",
          accountAge: data.seller.account_age,
          verification: data.seller.verification,
          totalDeals: data.seller.total_deals,
          disputes: data.seller.disputes,
          completionRate: data.seller.completion_rate,
          location: data.seller.location || "",
          lastActive: data.seller.last_active || "Unknown",
          networkSummary: data.seller.network_summary,
          platforms: data.seller.platforms,
          monthlyActivity: data.seller.monthly_activity,
        },
        signals: data.signals.map(function (s) {
          return {
            label: s.label,
            sub: s.sub,
            value: s.value,
            type: s.type,
          };
        }),
      };

      window.dispatchEvent(new CustomEvent("safely-data-ready"));
    } catch (error) {
      console.error(
        "Safely: failed to fetch analysis",
        error.message,
        error.stack,
      );
    }
  }

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

  // ── Reserve icon positions in left-to-right order BEFORE any tab loads ──
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

    var iconDiv = iconSlots[id];
    if (iconDiv) {
      iconDiv.title = title;
      iconDiv.innerHTML = iconSvg;
      iconDiv.style.display = "flex";
    }

    if (id === "risk") switchTab(id);
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

  await fetchAnalysis();
})();
