async function loadDashboardData() {
  var headers = window.safelyAuth.authHeader();

  try {
    var meRes = await fetch(API_BASE + "/me", { headers: headers });
    if (meRes.ok) {
      var meData = await meRes.json();
      updateSidebarUserName(meData.name);
      updateAvatar(meData.has_avatar);
    }
  } catch (e) {
    console.error("Safely: failed to load account name", e);
  }
}

function renderStats() {
  var checked = document.getElementById("stat-checked-num");
  var reported = document.getElementById("stat-reported-num");
  if (checked) {
    checked.textContent = document.querySelectorAll("#history-rows tr[data-id]").length;
  }
  if (reported) {
    reported.textContent = document.querySelectorAll("#report-rows tr").length;
  }
}
