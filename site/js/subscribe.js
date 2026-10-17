"use strict";
// Handles the subscribe form on /subscribe. Subscribing gives the free
// Procurement Red Flag Scorecard, but not on this page: Kit emails a
// confirmation link, and the PDF downloads only after the person clicks
// it. So after subscribing we only tell them to check their inbox.
(function () {
    const form = document.getElementById("nl-form");
    const emailInput = document.getElementById("nl-email");
    const submitBtn = document.getElementById("nl-submit");
    const messageEl = document.getElementById("nl-message");
    // The "check your inbox" text lives in the page, so it is translated
    // with the rest of the page (data-i18n), not fixed in English here.
    const checkInboxEl = document.getElementById("lead-check-inbox");
    const sendingEl = document.getElementById("lead-sending");
    if (!form || !emailInput || !submitBtn || !messageEl)
        return;
    form.addEventListener("submit", async (e) => {
        e.preventDefault();
        const email = emailInput.value.trim();
        if (!email)
            return;
        const originalText = submitBtn.textContent;
        submitBtn.disabled = true;
        submitBtn.textContent = sendingEl?.textContent?.trim() || "Sending...";
        messageEl.textContent = "";
        messageEl.className = "nl-message";
        try {
            const res = await fetch("/api/v1/newsletter/subscribe", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ email }),
            });
            const data = await res.json().catch(() => ({}));
            if (!res.ok) {
                throw new Error(data.message || "Something went wrong. Please try again.");
            }
            messageEl.textContent = checkInboxEl?.textContent?.trim() || data.message || "";
            messageEl.className = "nl-message success";
            form.reset();
        }
        catch (err) {
            const message = err instanceof Error ? err.message : "Couldn't subscribe right now. Please try again.";
            messageEl.textContent = message;
            messageEl.className = "nl-message error";
        }
        finally {
            submitBtn.disabled = false;
            submitBtn.textContent = originalText;
        }
    });
})();
