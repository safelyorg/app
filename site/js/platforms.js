"use strict";
// The B2B platforms Safely works on - the ONE list for the website.
// Compiled to js/platforms.js like the other site scripts.
// To add a platform, add its name here. The scrolling strips, the
// platform chips, the platform names in the FAQ and every platform
// count ("10 platforms") on the homepage and the /try page update by
// themselves. Names are the same in English and Portuguese, so they are
// not translated.
(function () {
    "use strict";
    const PLATFORMS = [
        "Alibaba",
        "Made-in-China",
        "Global Sources",
        "TradeWheel",
        "ExportHub",
        "B2Brazil",
        "ThomasNet",
        "B2BMap",
        "Kompass",
        "Wholesale Central",
    ];
    window.SAFELY_PLATFORMS = PLATFORMS.slice();
    const CHECK = '<span class="ic"><svg viewBox="0 0 24 24" fill="none"><path d="M20 6L9 17l-5-5" stroke="#fff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" /></svg></span>';
    function escapeHtml(text) {
        const div = document.createElement("div");
        div.textContent = text;
        return div.innerHTML;
    }
    // The list starting at another platform, so two strips next to each
    // other do not show the same names side by side.
    function rotated(list, by) {
        const n = by % list.length;
        return list.slice(n).concat(list.slice(0, n));
    }
    // Homepage strip: a pill with a tick for each platform, put before the
    // "More platforms coming soon" pill. The strip has two copies of
    // everything (the second one hidden from screen readers) so the loop
    // has no gap.
    function fillStrip(track) {
        const soon = track.querySelectorAll(".mp.soon");
        if (soon.length === 0)
            return;
        Array.prototype.forEach.call(soon, function (soonPill, copy) {
            PLATFORMS.forEach(function (name) {
                const pill = document.createElement("span");
                pill.className = "mp";
                if (copy === 0)
                    pill.setAttribute("role", "listitem");
                else
                    pill.setAttribute("aria-hidden", "true");
                pill.innerHTML = CHECK + escapeHtml(name);
                track.insertBefore(pill, soonPill);
            });
        });
    }
    // Small strips in the "Expanding across B2B platforms" card: the names
    // twice, for a loop with no gap.
    function fillNames(track, by) {
        const list = rotated(PLATFORMS, by);
        let html = "";
        list.forEach(function (name) {
            html += "<span>" + escapeHtml(name) + "</span>";
        });
        list.forEach(function (name) {
            html += '<span aria-hidden="true">' + escapeHtml(name) + "</span>";
        });
        track.innerHTML = html;
    }
    // /try page chips.
    function fillChips(box) {
        let html = "";
        PLATFORMS.forEach(function (name) {
            html += '<span class="mp">' + escapeHtml(name) + "</span>";
        });
        box.innerHTML = html;
    }
    function each(selector, fn) {
        Array.prototype.forEach.call(document.querySelectorAll(selector), fn);
    }
    function render() {
        each('[data-platforms="strip"]', fillStrip);
        each('[data-platforms="mq"]', function (t) {
            fillNames(t, 0);
        });
        each('[data-platforms="mq-rev"]', function (t) {
            fillNames(t, Math.ceil(PLATFORMS.length / 2));
        });
        each('[data-platforms="chips"]', fillChips);
        // "Alibaba, Made-in-China, Global Sources, ..." in FAQ answers.
        each('[data-platforms="names"]', function (el) {
            el.textContent = PLATFORMS.join(", ");
        });
        // "10" wherever the number of platforms is shown.
        each('[data-platforms="count"]', function (el) {
            el.textContent = String(PLATFORMS.length);
        });
    }
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", render);
    }
    else {
        render();
    }
})();
