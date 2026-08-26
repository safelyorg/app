"use strict";
// Handles: scroll-reveal animations, Escape-to-close, sign-in overlay
// wiring, pricing toggle, and stat count-up.
(function () {
    // Already signed in? Skip the sign-in screen and go straight to the
    // dashboard.
    const signinTriggers = document.querySelectorAll('label[for="si-toggle"]');
    signinTriggers.forEach((el) => {
        el.addEventListener("click", (e) => {
            const token = localStorage.getItem("safely_session_token");
            if (token) {
                e.preventDefault();
                window.location.href = "/dashboard/";
            }
        });
    });
    // Reveal-on-scroll for anything with .reveal.
    const revealEls = document.querySelectorAll(".reveal");
    if ("IntersectionObserver" in window) {
        const io = new IntersectionObserver((entries) => {
            entries.forEach((entry) => {
                if (entry.isIntersecting) {
                    entry.target.classList.add("in");
                    io.unobserve(entry.target);
                }
            });
        }, { threshold: 0.12, rootMargin: "0px 0px -7% 0px" });
        revealEls.forEach((el) => io.observe(el));
    }
    else {
        revealEls.forEach((el) => el.classList.add("in"));
    }
    // Escape closes the nav, sign-in overlay, and any open modal.
    addEventListener("keydown", (e) => {
        if (e.key !== "Escape")
            return;
        const nav = document.getElementById("nav-toggle");
        const si = document.getElementById("si-toggle");
        if (nav)
            nav.checked = false;
        if (si)
            si.checked = false;
        if (location.hash.indexOf("#m-") === 0) {
            history.replaceState(null, "", location.pathname + location.search);
        }
    });
    // The sign-in iframe asks us to close the overlay via postMessage,
    // since it can't reach outside itself.
    window.addEventListener("message", (e) => {
        if (e.data === "safely:closeSignin") {
            const si = document.getElementById("si-toggle");
            if (si)
                si.checked = false;
        }
    });
    // Strips the leftover "#_" from the address bar after a modal closes.
    window.addEventListener("hashchange", () => {
        if (window.location.hash === "#_") {
            history.replaceState(null, "", window.location.pathname + window.location.search);
        }
    });
    // Stat bar: counts each number up from 0 once it scrolls into view.
    const statNums = document.querySelectorAll(".statbar .num");
    if (statNums.length && "IntersectionObserver" in window) {
        const animateCount = (el) => {
            const raw = el.textContent?.trim() ?? "";
            const match = raw.match(/^(\d+)(.*)$/);
            if (!match)
                return;
            const target = parseInt(match[1], 10);
            const suffix = match[2];
            const duration = 1200;
            let startTime = null;
            function step(timestamp) {
                if (startTime === null)
                    startTime = timestamp;
                const progress = Math.min((timestamp - startTime) / duration, 1);
                const eased = 1 - Math.pow(1 - progress, 3);
                const current = Math.round(eased * target);
                el.textContent = current + suffix;
                if (progress < 1) {
                    requestAnimationFrame(step);
                }
                else {
                    el.textContent = target + suffix;
                }
            }
            requestAnimationFrame(step);
        };
        const statObserver = new IntersectionObserver((entries) => {
            entries.forEach((entry) => {
                if (entry.isIntersecting) {
                    animateCount(entry.target);
                    statObserver.unobserve(entry.target);
                }
            });
        }, { threshold: 0.4 });
        statNums.forEach((el) => statObserver.observe(el));
    }
    // Pricing toggle: Monthly / Yearly. Values already live in the HTML
    // via data-mo / data-yr, this just swaps which one shows.
    const billToggleBtns = document.querySelectorAll(".ptoggle-btn");
    if (billToggleBtns.length) {
        const billFields = document.querySelectorAll("[data-mo]");
        const thumb = document.querySelector(".ptoggle-thumb");
        // The thumb measures the active button's real box, since Monthly
        // and Yearly aren't the same width.
        const positionThumb = (period) => {
            if (!thumb)
                return;
            const activeBtn = Array.from(billToggleBtns).find((btn) => btn.dataset.bill === period);
            if (!activeBtn)
                return;
            thumb.style.width = activeBtn.offsetWidth + "px";
            thumb.style.transform = "translateX(" + activeBtn.offsetLeft + "px)";
        };
        const setBillingPeriod = (period) => {
            billToggleBtns.forEach((btn) => {
                btn.classList.toggle("active", btn.dataset.bill === period);
            });
            billFields.forEach((el) => {
                const value = period === "mo" ? el.dataset.mo : el.dataset.yr;
                if (value !== undefined)
                    el.textContent = value;
            });
            positionThumb(period);
        };
        billToggleBtns.forEach((btn) => {
            btn.addEventListener("click", () => {
                setBillingPeriod(btn.dataset.bill ?? "mo");
            });
        });
        positionThumb("mo");
        // Re-measure once the real web font has loaded, since a fallback
        // system font can measure narrower.
        if (document.fonts && document.fonts.ready) {
            document.fonts.ready.then(() => {
                const current = document.querySelector(".ptoggle-btn.active");
                positionThumb(current?.dataset.bill ?? "mo");
            });
        }
        window.addEventListener("resize", () => {
            const current = document.querySelector(".ptoggle-btn.active");
            positionThumb(current?.dataset.bill ?? "mo");
        });
    }
})();
// Shows a brief toast if this page was just reached after a successful
// account deletion (?account_deleted=1).
(function () {
    const params = new URLSearchParams(window.location.search);
    if (params.get("account_deleted") !== "1")
        return;
    const toast = document.createElement("div");
    toast.textContent = "Your account has been deleted.";
    toast.className = "account-deleted-toast";
    document.body.appendChild(toast);
    setTimeout(() => {
        toast.classList.add("fade-out");
        setTimeout(() => toast.remove(), 300);
    }, 3000);
    // Clean the URL so a refresh doesn't show this again.
    history.replaceState(null, "", window.location.pathname);
})();
