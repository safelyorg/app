// (1) reveal-on-scroll via IntersectionObserver (CSS scroll-timelines aren't broadly supported yet);
// (2) Escape closes the menu / sign-in / modals (keyboard accessibility). */
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

  document.getElementById("siMagic").addEventListener("click", function () {
    window.location.href = "/dashboard/";
  });

  document.getElementById("siGoogle").addEventListener("click", function () {
    window.location.href = "/dashboard/";
  });
})();
