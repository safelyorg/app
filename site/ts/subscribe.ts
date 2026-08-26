// Handles the newsletter subscribe form submission.
(function () {
  const form = document.getElementById("nl-form") as HTMLFormElement | null;
  const emailInput = document.getElementById("nl-email") as HTMLInputElement | null;
  const submitBtn = document.getElementById("nl-submit") as HTMLButtonElement | null;
  const messageEl = document.getElementById("nl-message") as HTMLElement | null;

  if (!form || !emailInput || !submitBtn || !messageEl) return;

  form.addEventListener("submit", async (e: SubmitEvent) => {
    e.preventDefault();
    const email = emailInput.value.trim();
    if (!email) return;

    const originalText = submitBtn.textContent;
    submitBtn.disabled = true;
    submitBtn.textContent = "Subscribing...";
    messageEl.textContent = "";
    messageEl.className = "nl-message";

    try {
      const res = await fetch("/api/v1/newsletter/subscribe", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email }),
      });

      const data: { message?: string } = await res.json().catch(() => ({}));

      if (!res.ok) {
        throw new Error(data.message || "Something went wrong. Please try again.");
      }

      messageEl.textContent = data.message || "You're subscribed! Check your inbox to confirm.";
      messageEl.className = "nl-message success";
      form.reset();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Couldn't subscribe right now. Please try again.";
      messageEl.textContent = message;
      messageEl.className = "nl-message error";
    } finally {
      submitBtn.disabled = false;
      submitBtn.textContent = originalText;
    }
  });
})();
