document.addEventListener("DOMContentLoaded", function () {
  checkGoogleConnectResult();

  if (window.safelyAuth && window.safelyAuth.getToken()) {
    loadDashboardData();
  }

  var closeBtn = document.getElementById("detail-close");
  if (closeBtn) {
    closeBtn.addEventListener("click", closeDetailPanel);
  }

  var navHistory = document.getElementById("view-history");
  var navReports = document.getElementById("view-reports");
  if (navHistory) {
    navHistory.addEventListener("change", closeDetailPanel);
    navHistory.addEventListener("click", closeDetailPanel);
  }
  if (navReports) {
    navReports.addEventListener("change", closeDetailPanel);
    navReports.addEventListener("click", closeDetailPanel);
  }

  var mobileToggle = document.getElementById("mobile-nav-toggle");
  if (mobileToggle) {
    [navHistory, navReports].forEach(function (input) {
      if (input) {
        input.addEventListener("change", function () {
          mobileToggle.checked = false;
        });
      }
    });
  }

  var navSettings = document.getElementById("view-settings");
  if (navSettings) {
    navSettings.addEventListener("change", closeDetailPanel);
    navSettings.addEventListener("click", closeDetailPanel);
    navSettings.addEventListener("change", function () {
      if (!settingsLoaded) loadSettingsData();
    });

    if (navSettings.checked && !settingsLoaded) {
      loadSettingsData();
    }
  }

  window.addEventListener("pageshow", function () {
    var settingsRadio = document.getElementById("view-settings");
    if (settingsRadio && settingsRadio.checked && !settingsLoaded) {
      loadSettingsData();
    }
  });

  var settingsLink = document.getElementById("account-settings-link");
  if (settingsLink) {
    settingsLink.addEventListener("click", function () {
      var menu = document.getElementById("account-menu");
      if (menu) menu.open = false;
    });
  }

  var profileEditBtn = document.getElementById("profile-edit-btn");
  if (profileEditBtn) {
    profileEditBtn.addEventListener("click", function () {
      toggleProfileEdit(true);
    });
  }
  var profileCancelBtn = document.getElementById("profile-cancel-btn");
  if (profileCancelBtn) {
    profileCancelBtn.addEventListener("click", function () {
      toggleProfileEdit(false);
    });
  }
  var profileSaveBtn = document.getElementById("profile-save-btn");
  if (profileSaveBtn) {
    profileSaveBtn.addEventListener("click", saveProfileEdit);
  }
  var nameInput = document.getElementById("settings-name-input");
  if (nameInput) {
    nameInput.addEventListener("keydown", function (e) {
      if (e.key === "Enter") saveProfileEdit();
      if (e.key === "Escape") toggleProfileEdit(false);
    });
  }

  var deleteBtn = document.getElementById("delete-account-btn");
  var deleteConfirmBox = document.getElementById("delete-account-confirm");
  var deleteConfirmEmailEl = document.getElementById("delete-confirm-email");
  var deleteConfirmInput = document.getElementById("delete-confirm-input");
  var deleteConfirmBtn = document.getElementById("delete-confirm-btn");
  var deleteCancelBtn = document.getElementById("delete-cancel-btn");
  var deleteConfirmError = document.getElementById("delete-confirm-error");

  if (deleteBtn && deleteConfirmBox) {
    deleteBtn.addEventListener("click", function () {
      var accountEmail = document.getElementById("settings-email")
        ? document.getElementById("settings-email").textContent
        : "";
      if (deleteConfirmEmailEl) deleteConfirmEmailEl.textContent = accountEmail;
      if (deleteConfirmInput) deleteConfirmInput.value = "";
      if (deleteConfirmBtn) deleteConfirmBtn.disabled = true;
      if (deleteConfirmError) deleteConfirmError.classList.add("hidden");
      deleteConfirmBox.classList.remove("hidden");
      if (deleteConfirmInput) deleteConfirmInput.focus();
    });
  }

  if (deleteCancelBtn && deleteConfirmBox) {
    deleteCancelBtn.addEventListener("click", function () {
      deleteConfirmBox.classList.add("hidden");
    });
  }

  if (deleteConfirmInput && deleteConfirmBtn) {
    deleteConfirmInput.addEventListener("input", function () {
      var accountEmail = document.getElementById("settings-email")
        ? document.getElementById("settings-email").textContent.trim().toLowerCase()
        : "";
      var typed = deleteConfirmInput.value.trim().toLowerCase();
      deleteConfirmBtn.disabled = !(typed && typed === accountEmail);
    });
  }

  if (deleteConfirmBtn) {
    deleteConfirmBtn.addEventListener("click", async function () {
      deleteConfirmBtn.disabled = true;
      var originalText = deleteConfirmBtn.textContent;
      deleteConfirmBtn.textContent = "Deleting...";

      try {
        var res = await fetch(API_BASE + "/me", {
          method: "DELETE",
          headers: window.safelyAuth.authHeader(),
        });

        if (res.status === 401) {
          window.safelyAuth.logout();
          return;
        }
        if (!res.ok) throw new Error("Failed to delete account");

        window.safelyAuth.clearToken();
        window.location.href = "/";
      } catch (e) {
        if (deleteConfirmError) {
          deleteConfirmError.textContent =
            "Could not delete your account. Please try again.";
          deleteConfirmError.classList.remove("hidden");
        }
        deleteConfirmBtn.disabled = false;
        deleteConfirmBtn.textContent = originalText;
      }
    });
  }
  var googleConnectBtn = document.getElementById("google-connect-btn");
  if (googleConnectBtn) {
    wireGoogleButtonHover();
    googleConnectBtn.addEventListener("click", handleGoogleButtonClick);
  }

  var changePlanBtn = document.getElementById("change-plan-btn");
  var changePlanDropdown = document.getElementById("change-plan-dropdown");

  function togglePlanDropdown(show) {
    if (!changePlanDropdown) return;
    changePlanDropdown.classList.toggle("hidden", !show);
  }

  if (changePlanBtn && changePlanDropdown) {
    changePlanBtn.addEventListener("click", function (e) {
      e.stopPropagation();
      togglePlanDropdown(changePlanDropdown.classList.contains("hidden"));
    });

    document.querySelectorAll(".plan-option").forEach(function (opt) {
      opt.addEventListener("click", function () {
        var planName = opt.dataset.plan;
        var planPrice = opt.dataset.price;

        var nameEl = document.getElementById("current-plan-name");
        var priceEl = document.getElementById("current-plan-price");
        if (nameEl) nameEl.textContent = planName;
        if (priceEl) priceEl.textContent = planPrice + " \u00b7 Renews Aug 9, 2026";

        document.querySelectorAll(".plan-option .plan-check").forEach(function (c) {
          c.classList.add("hidden");
        });
        var check = opt.querySelector(".plan-check");
        if (check) check.classList.remove("hidden");

        togglePlanDropdown(false);
      });
    });

    document.addEventListener("click", function (e) {
      if (
        !changePlanDropdown.classList.contains("hidden") &&
        !changePlanDropdown.contains(e.target) &&
        e.target !== changePlanBtn
      ) {
        togglePlanDropdown(false);
      }
    });
  }
  var addPaymentBtn = document.getElementById("add-payment-btn");
  var paymentForm = document.getElementById("payment-form");
  if (addPaymentBtn && paymentForm) {
    addPaymentBtn.addEventListener("click", function () {
      paymentForm.classList.remove("hidden");
    });
  }
  var paymentCancelBtn = document.getElementById("payment-cancel-btn");
  if (paymentCancelBtn && paymentForm) {
    paymentCancelBtn.addEventListener("click", function () {
      paymentForm.classList.add("hidden");
    });
  }
  var cardNumberInput = document.getElementById("card-number-input");
  if (cardNumberInput) {
    cardNumberInput.addEventListener("input", function (e) {
      var digits = e.target.value.replace(/\D/g, "").slice(0, 16);
      var groups = digits.match(/.{1,4}/g) || [];
      e.target.value = groups.join(" ");
      updateCardBrandIcon(digits);
    });
  }

  var cardExpiryInput = document.getElementById("card-expiry-input");
  if (cardExpiryInput) {
    cardExpiryInput.addEventListener("input", function (e) {
      var raw = e.target.value.replace(/\D/g, "").slice(0, 4);

      if (raw.length === 0) {
        e.target.value = "";
        return;
      }

      if (raw.length === 1) {
        if (parseInt(raw, 10) >= 2) {
          e.target.value = "0" + raw + "/";
        } else {
          e.target.value = raw;
        }
        return;
      }

      var month = parseInt(raw.slice(0, 2), 10);
      if (month === 0) month = 1;
      if (month > 12) month = 12;
      var monthStr = month < 10 ? "0" + month : String(month);

      if (raw.length === 2) {
        e.target.value = monthStr + "/";
      } else {
        e.target.value = monthStr + "/" + raw.slice(2, 4);
      }
    });
  }

  var cardCvcInput = document.getElementById("card-cvc-input");
  if (cardCvcInput) {
    cardCvcInput.addEventListener("input", function (e) {
      e.target.value = e.target.value.replace(/\D/g, "").slice(0, 3);
    });
  }

  var paymentSaveBtn = document.getElementById("payment-save-btn");
  if (paymentSaveBtn) {
    paymentSaveBtn.addEventListener("click", function () {
      var digits = cardNumberInput
        ? cardNumberInput.value.replace(/\D/g, "")
        : "";
      var brand = detectCardBrand(digits);
      var last4 = digits.slice(-4);

      var iconEl = document.getElementById("payment-method-icon");
      var labelEl = document.getElementById("payment-method-label");

      if (last4.length === 4 && iconEl && labelEl) {
        if (brand === "visa") {
          iconEl.className =
            "w-9 h-6 rounded bg-[#1a1f71] flex items-center justify-center text-white text-[8px] font-black flex-shrink-0";
          iconEl.textContent = "VISA";
          labelEl.textContent = "Visa ending in " + last4;
        } else if (brand === "mastercard") {
          iconEl.className =
            "w-9 h-6 rounded bg-surface2 border border-line flex items-center justify-center flex-shrink-0";
          iconEl.innerHTML =
            '<svg width="20" height="12" viewBox="0 0 26 16"><circle cx="9" cy="8" r="7" fill="#EB001B"/><circle cx="17" cy="8" r="7" fill="#F79E1B" fill-opacity="0.9"/></svg>';
          labelEl.textContent = "Mastercard ending in " + last4;
        } else {
          iconEl.className =
            "w-9 h-6 rounded bg-surface2 border border-line flex items-center justify-center text-[9px] font-bold text-muted flex-shrink-0";
          iconEl.textContent = "CARD";
          labelEl.textContent = "Card ending in " + last4;
        }
        labelEl.classList.remove("text-muted");
      }

      if (paymentForm) paymentForm.classList.add("hidden");

      alert(
        "Payment processing isn't wired up yet - this is a design preview. " +
          "The card details above are shown for preview only and were not actually saved anywhere.",
      );
    });
  }

  var termsBtn = document.getElementById("terms-privacy-btn");
  var termsModal = document.getElementById("terms-privacy-modal");
  var termsClose = document.getElementById("terms-privacy-close");
  var termsBackdrop = document.getElementById("terms-privacy-backdrop");

  function toggleTermsModal(show) {
    if (!termsModal) return;
    termsModal.classList.toggle("hidden", !show);
    termsModal.classList.toggle("flex", show);
  }

  if (termsBtn) {
    termsBtn.addEventListener("click", function () {
      toggleTermsModal(true);
    });
  }
  if (termsClose) {
    termsClose.addEventListener("click", function () {
      toggleTermsModal(false);
    });
  }
  if (termsBackdrop) {
    termsBackdrop.addEventListener("click", function () {
      toggleTermsModal(false);
    });
  }
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") toggleTermsModal(false);
  });

  var avatarInput = document.getElementById("settings-avatar-input");
  if (avatarInput) {
    avatarInput.addEventListener("change", function (e) {
      var file = e.target.files && e.target.files[0];
      if (file) uploadAvatar(file);
    });
  }

  document.querySelectorAll(".detail-tab-btn").forEach(function (btn) {
    btn.addEventListener("click", function () {
      switchDetailTab(btn.dataset.detailTab);
    });
  });

  var searchBox = document.getElementById("search-box");
  if (searchBox) {
    searchBox.addEventListener("input", function (e) {
      var q = e.target.value.trim().toLowerCase();
      document.querySelectorAll("#history-rows tr").forEach(function (tr) {
        var haystack = tr.getAttribute("data-search") || "";
        tr.setAttribute(
          "data-search-hidden",
          q && haystack.indexOf(q) === -1 ? "true" : "false",
        );
      });
    });
  }

  document.addEventListener("click", function (e) {
    var menu = document.getElementById("account-menu");
    if (menu && menu.open && !menu.contains(e.target)) {
      menu.open = false;
    }
  });
});
