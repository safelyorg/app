(function () {
  var form = document.getElementById("nl-form");
  var emailInput = document.getElementById("nl-email");
  var submitBtn = document.getElementById("nl-submit");
  var messageEl = document.getElementById("nl-message");

  if (!form) return;

  form.addEventListener("submit", async function (e) {
    e.preventDefault();

    var email = emailInput.value.trim();
    if (!email) return;

    var originalText = submitBtn.textContent;
    submitBtn.disabled = true;
    submitBtn.textContent = "Subscribing...";
    messageEl.textContent = "";
    messageEl.className = "nl-message";

    try {
      var res = await fetch("/api/v1/newsletter/subscribe", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: email }),
      });

      var data = await res.json().catch(function () {
        return {};
      });

      if (!res.ok) {
        throw new Error(data.message || "Something went wrong. Please try again.");
      }

      messageEl.textContent = data.message || "You're subscribed! Check your inbox to confirm.";
      messageEl.className = "nl-message success";
      form.reset();
    } catch (err) {
      messageEl.textContent = err.message || "Couldn't subscribe right now. Please try again.";
      messageEl.className = "nl-message error";
    } finally {
      submitBtn.disabled = false;
      submitBtn.textContent = originalText;
    }
  });
})();
