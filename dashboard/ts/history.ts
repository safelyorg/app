async function loadDashboardData(): Promise<void> {
  const headers = (window as any).safelyAuth.authHeader();

  try {
    const meRes = await fetch(API_BASE + "/me", { headers });
    if (meRes.ok) {
      const meData = await meRes.json();
      updateSidebarUserName(meData.name);
      updateAvatar(meData.has_avatar);
    }
  } catch (e) {
    console.error("Safely: failed to load account name", e);
  }
}

function renderStats(): void {
  const checked = document.getElementById("stat-checked-num");
  const reported = document.getElementById("stat-reported-num");

  if (checked) {
    checked.textContent = document
      .querySelectorAll("#history-rows tr[data-id]")
      .length.toString();
  }
  if (reported) {
    reported.textContent = document.querySelectorAll("#report-rows tr[data-report]").length.toString();
  }
}
