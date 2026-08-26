// Handles: magic-link sign-in, and the overlay's close button.
// Google sign-in no longer needs JS - #siGoogle is now a plain <a>
// pointing straight at /api/v1/auth/google.
(function () {
  const magicBtn = document.getElementById("siMagic") as HTMLButtonElement | null;
  const emailInput = document.getElementById("siEmail") as HTMLInputElement | null;
  const siCard = document.getElementById("siCard");
  const siOkMail = document.getElementById("siOkMail");

  if (magicBtn && emailInput) {
    magicBtn.addEventListener("click", () => {
      const email = emailInput.value.trim();
      if (!email || email.indexOf("@") === -1) {
        emailInput.focus();
        return;
      }

      magicBtn.disabled = true;
      const originalText = magicBtn.textContent;
      magicBtn.textContent = "Sending...";

      fetch("/api/v1/auth/magic-link", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email }),
      })
        .then((res) =>
          // The backend's own message is always safe to show directly,
          // success or failure - it's deliberately identical either
          // way, so nothing extra leaks through by trusting it here.
          res.text().then((rawText) => {
            let data: { message?: string } | null = null;
            try {
              data = JSON.parse(rawText);
            } catch {
              // Backend sent plain text, not JSON - use it directly.
            }
            if (!res.ok) {
              const message = data?.message || rawText || "Something went wrong.";
              throw new Error(message);
            }
            return data;
          }),
        )
        .then(() => {
          if (siOkMail) {
            siOkMail.textContent = "We sent a secure sign-in link to " + email + ".";
          }
          if (siCard) siCard.classList.add("sent");
        })
        .catch((err: Error) => {
          magicBtn.disabled = false;
          magicBtn.textContent = originalText;
          alert(err.message || "Something went wrong sending the link. Please try again.");
        });
    });

    emailInput.addEventListener("keydown", (e: KeyboardEvent) => {
      if (e.key === "Enter") magicBtn.click();
    });
  }

  // The close button only shows when embedded with ?closable=1 (the
  // landing page's overlay) - the dashboard's login gate never sets
  // this, since sign-in there is mandatory.
  const params = new URLSearchParams(window.location.search);
  const closeBtn = document.getElementById("siClose") as HTMLElement | null;
  if (params.get("closable") === "1" && closeBtn) {
    closeBtn.style.display = "";
    closeBtn.addEventListener("click", () => {
      window.parent.postMessage("safely:closeSignin", "*");
    });
  }
})();
