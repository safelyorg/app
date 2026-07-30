(function () {
  // Google sign-in navigates the WHOLE TAB (window.top), not just this
  // iframe - the OAuth flow needs to run at the top level regardless of
  // which page embedded this sign-in surface, since it ends by
  // redirecting to /dashboard/#session=... which only makes sense as a
  // full-page navigation.
  var googleBtn = document.getElementById("siGoogle");
  if (googleBtn) {
    googleBtn.addEventListener("click", function () {
      window.top.location.href = "/api/v1/auth/google";
    });
  }
  var magicBtn = document.getElementById("siMagic");
  var emailInput = document.getElementById("siEmail");
  var siCard = document.getElementById("siCard");
  var siOkMail = document.getElementById("siOkMail");
  if (magicBtn && emailInput) {
    magicBtn.addEventListener("click", function () {
      var email = emailInput.value.trim();
      if (!email || email.indexOf("@") === -1) {
        emailInput.focus();
        return;
      }
      magicBtn.disabled = true;
      var originalText = magicBtn.textContent;
      magicBtn.textContent = "Sending...";
      fetch("/api/v1/auth/magic-link", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: email }),
      })
        .then(function (res) {
          // Reads the real body either way, success or failure - the
          // backend's own message is what actually gets shown, rather
          // than a fixed generic string guessed at here. This is safe:
          // the backend deliberately keeps its message identical
          // regardless of whether the email genuinely sent, so nothing
          // extra leaks through by trusting it directly here - we're
          // just correctly displaying whatever it already decided is
          // safe to say.
          return res.text().then(function (rawText) {
            var data = null;
            try {
              data = JSON.parse(rawText);
            } catch (e) {
              // Backend sent plain text, not JSON (one of our
              // AuthError variants) - use it directly as the message.
            }
            if (!res.ok) {
              var message =
                (data && data.message) || rawText || "Something went wrong.";
              throw new Error(message);
            }
            return data;
          });
        })
        .then(function () {
          if (siOkMail) {
            siOkMail.textContent =
              "We sent a secure sign-in link to " + email + ".";
          }
          if (siCard) siCard.classList.add("sent");
        })
        .catch(function (err) {
          magicBtn.disabled = false;
          magicBtn.textContent = originalText;
          alert(err.message || "Something went wrong sending the link. Please try again.");
        });
    });
    emailInput.addEventListener("keydown", function (e) {
      if (e.key === "Enter") magicBtn.click();
    });
  }
  // The X close button only makes sense in a dismissible context (the
  // landing page's overlay, where there's somewhere to return to) - it's
  // hidden by default and only shown when the embedding page explicitly
  // opts in via ?closable=1 on the iframe's src. The dashboard's login
  // gate never sets this, since there's nowhere sensible to "close" to
  // there - signing in is mandatory to see the dashboard at all.
  var params = new URLSearchParams(window.location.search);
  var closeBtn = document.getElementById("siClose");
  if (params.get("closable") === "1" && closeBtn) {
    closeBtn.style.display = "";
    closeBtn.addEventListener("click", function () {
      window.parent.postMessage("safely:closeSignin", "*");
    });
  }
})();
