(function () {
  "use strict";
  function defaultData() {
    return {
      riskScore: 0,
      fraudReportCount: 0,
      seller: {
        name: "Unknown",
        handle: "",
        accountAge: "Unknown",
        verification: "unknown",
        location: "",
        lastActive: "Unknown",
        networkSummary: "Could not connect to Safely server.",
        monthlyActivity: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        platform: "unknown",
        platformId: null,
      },
      signals: [],
    };
  }
  window.__safelyData = defaultData();
  window.__safelyUpdateState = function (newData) {
    window.__safelyData = Object.assign({}, window.__safelyData, newData);
  };
  window.__safelyResetState = function () {
    window.__safelyData = defaultData();
  };
})();
