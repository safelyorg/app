/* (1) reveal-on-scroll via IntersectionObserver (CSS scroll-timelines aren't broadly supported yet);
   (2) Escape closes the menu / sign-in / modals (keyboard accessibility);
   (3) real auth wiring for the sign-in overlay. */
(function () {
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
  // Google sign-in: this is a real navigation, not a fetch. Clicking
  // sends the browser to the backend, which 302s to Google's actual
  // consent screen. The backend handles everything after that and
  // eventually lands the person on /dashboard#session=<token>.
  // ============================================================
  var googleBtn = document.getElementById("siGoogle");
  if (googleBtn) {
    googleBtn.addEventListener("click", function () {
      window.location.href = "/api/v1/auth/google";
    });
  }

  // ============================================================
  // Magic link: this one does need a real fetch, since we have to
  // wait for the backend to confirm the email was accepted before
  // showing "check your email" - a plain link/redirect can't do that.
  // ============================================================
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
          if (!res.ok) throw new Error("Request failed");
          return res.json();
        })
        .then(function () {
          if (siOkMail) {
            siOkMail.textContent =
              "We sent a secure sign-in link to " + email + ".";
          }
          if (siCard) siCard.classList.add("sent");
        })
        .catch(function () {
          magicBtn.disabled = false;
          magicBtn.textContent = originalText;
          alert("Something went wrong sending the link. Please try again.");
        });
    });
  }
})();
