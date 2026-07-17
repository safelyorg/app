/* (1) reveal-on-scroll via IntersectionObserver (CSS scroll-timelines aren't broadly supported yet);
   (2) Escape closes the menu / sign-in / modals (keyboard accessibility);
   (3) real auth wiring for the sign-in overlay;
   (4) pricing Monthly/Yearly toggle;
   (5) stat bar count-up animation. */
(function () {
  // If a valid-looking session token already exists, clicking Login/Get
  // Started should skip the sign-in screen entirely and go straight to
  // the dashboard - there's no reason to make an already-logged-in
  // person re-authenticate just because they clicked a button meant for
  // people who aren't signed in yet. This only checks that a token is
  // PRESENT, not that it's still valid server-side (an expired or
  // revoked token would just land them back on the dashboard's own
  // sign-in gate, which already handles that case correctly - this is
  // purely about skipping an unnecessary extra step for the common
  // case of someone who's actually still logged in).
  var signinTriggers = document.querySelectorAll('label[for="si-toggle"]');
  if (signinTriggers.length) {
    signinTriggers.forEach(function (el) {
      el.addEventListener("click", function (e) {
        var token = localStorage.getItem("safely_session_token");
        if (token) {
          e.preventDefault();
          window.location.href = "/dashboard/";
        }
      });
    });
  }

  var els = document.querySelectorAll(".reveal");
  if ("IntersectionObserver" in window) {
    var io = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (en) {
          if (en.isIntersecting) {
            en.target.classList.add("in");
            io.unobserve(en.target);
          }
        });
      },
      { threshold: 0.12, rootMargin: "0px 0px -7% 0px" },
    );
    els.forEach(function (el) {
      io.observe(el);
    });
  } else {
    els.forEach(function (el) {
      el.classList.add("in");
    });
  }

  addEventListener("keydown", function (e) {
    if (e.key !== "Escape") return;
    var nav = document.getElementById("nav-toggle");
    var si = document.getElementById("si-toggle");
    if (nav) nav.checked = false;
    if (si) si.checked = false;
    if (location.hash.indexOf("#m-") === 0) {
      history.replaceState(null, "", location.pathname + location.search);
    }
  });

  // ============================================================
  // Sign-in itself (Google + magic link) now lives entirely inside the
  // shared /signin.html, embedded here via <iframe> - this page only
  // needs to know how to CLOSE that overlay when the iframe's own close
  // button is clicked, since the iframe can't reach outside itself to
  // uncheck #si-toggle directly.
  // ============================================================
  window.addEventListener("message", function (e) {
    if (e.data === "safely:closeSignin") {
      var si = document.getElementById("si-toggle");
      if (si) si.checked = false;
    }
  });

  // ============================================================
  // Cleans up the address bar after closing a footer modal. The modals
  // themselves work by changing the URL fragment (#m-about, #_, etc.) -
  // pure CSS, no JS needed for that part - but leaving "#_" sitting
  // visibly in the address bar afterward looks like a broken/leftover
  // URL. This just quietly removes it right after the close happens,
  // without interfering with the modal's own show/hide behavior at all.
  // Kept independent of the pricing toggle below (or anything else)
  // since it has nothing to do with pricing specifically - it needs to
  // keep working regardless of which sections of the page exist.
  // ============================================================
  window.addEventListener("hashchange", function () {
    if (window.location.hash === "#_") {
      history.replaceState(null, "", window.location.pathname + window.location.search);
    }
  });

  // ============================================================
  // Stat bar: counts each number up from 0 to its real value the
  // moment the section scrolls into view, instead of just appearing
  // static. Reads the target straight from what's already in the
  // HTML (no extra markup needed) - splits the leading digits apart
  // from whatever comes after (%, s, +) and only animates the
  // numeric part, leaving the suffix untouched throughout.
  // ============================================================
  var statNums = document.querySelectorAll(".statbar .num");
  if (statNums.length && "IntersectionObserver" in window) {
    var animateCount = function (el) {
      var raw = el.textContent.trim();
      var match = raw.match(/^(\d+)(.*)$/);
      if (!match) return; // doesn't start with a number - leave it alone

      var target = parseInt(match[1], 10);
      var suffix = match[2];
      var duration = 1200;
      var startTime = null;

      function step(timestamp) {
        if (startTime === null) startTime = timestamp;
        var progress = Math.min((timestamp - startTime) / duration, 1);
        // ease-out curve, so it settles smoothly at the end instead
        // of stopping abruptly
        var eased = 1 - Math.pow(1 - progress, 3);
        var current = Math.round(eased * target);
        el.textContent = current + suffix;

        if (progress < 1) {
          requestAnimationFrame(step);
        } else {
          el.textContent = target + suffix; // guarantees the exact final value
        }
      }
      requestAnimationFrame(step);
    };

    var statObserver = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            animateCount(entry.target);
            statObserver.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.4 },
    );

    statNums.forEach(function (el) {
      statObserver.observe(el);
    });
  }

  // ============================================================
  // Pricing toggle: Monthly / Yearly billing switch. Every price
  // element already stores both values directly in the HTML
  // (data-mo="$29" data-yr="$279") - this just reads whichever one
  // matches the currently-clicked button and displays it. No math
  // happens here, the real numbers already live in the markup.
  // ============================================================
  var billToggleBtns = document.querySelectorAll(".ptoggle-btn");
  if (billToggleBtns.length) {
    var billFields = document.querySelectorAll("[data-mo]");
    var thumb = document.querySelector(".ptoggle-thumb");

    // Monthly and Yearly aren't the same width (Yearly carries the extra
    // "Save ~20%" badge), so the sliding thumb can't just be a fixed 50%
    // - it measures the actual active button's box each time and moves
    // to match it exactly.
    var positionThumb = function (period) {
      if (!thumb) return;
      var activeBtn = Array.prototype.filter.call(
        billToggleBtns,
        function (btn) {
          return btn.dataset.bill === period;
        },
      )[0];
      if (!activeBtn) return;
      thumb.style.width = activeBtn.offsetWidth + "px";
      thumb.style.transform = "translateX(" + activeBtn.offsetLeft + "px)";
    };

    var setBillingPeriod = function (period) {
      billToggleBtns.forEach(function (btn) {
        btn.classList.toggle("active", btn.dataset.bill === period);
      });
      billFields.forEach(function (el) {
        var value = period === "mo" ? el.dataset.mo : el.dataset.yr;
        if (value !== undefined) el.textContent = value;
      });
      positionThumb(period);
    };

    billToggleBtns.forEach(function (btn) {
      btn.addEventListener("click", function () {
        setBillingPeriod(btn.dataset.bill);
      });
    });

    // Position the thumb correctly on first load, matching whichever
    // button starts with the "active" class in the HTML.
    positionThumb("mo");

    // If the custom web font hadn't finished loading yet at the moment
    // above, the button was measured using a fallback system font -
    // often narrower than the real one, leaving the thumb visibly
    // short of the button's true edge (looking exactly like missing
    // padding, even though the padding itself is fine). Re-measuring
    // once fonts are confirmed ready corrects that regardless of how
    // fast or slow the font happened to load on this visit.
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(function () {
        var current = document.querySelector(".ptoggle-btn.active");
        positionThumb(current ? current.dataset.bill : "mo");
      });
    }

    // Also re-measure on window resize, since button widths can change
    // at different viewport sizes.
    window.addEventListener("resize", function () {
      var current = document.querySelector(".ptoggle-btn.active");
      positionThumb(current ? current.dataset.bill : "mo");
    });
  }
})();
