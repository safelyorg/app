"use strict";
(function () {
    "use strict";
    const SAFELY_ENV = "local";
    const API_BASE = SAFELY_ENV === "local" ? "http://localhost:3000/api/v1" : "https://safely.sh/api/v1";
    // Same toggle, but for the actual website (not the /api/v1 path) -
    // kept as one source of truth alongside API_BASE.
    const SITE_BASE = SAFELY_ENV === "local" ? "http://localhost:3000" : "https://safely.sh";
    // Reads the session token auth-bridge.js relayed from the website.
    // Returns an empty object if the person isn't logged in.
    async function getAuthHeaders() {
        try {
            const result = await chrome.storage.local.get("safely_session_token");
            const token = result.safely_session_token;
            return token ? { Authorization: "Bearer " + token } : {};
        }
        catch (e) {
            return {};
        }
    }
    window.__safelyAPI = {
        SITE_BASE,
        analyze: async function (scrapedData) {
            try {
                const authHeaders = await getAuthHeaders();
                const response = await fetch(API_BASE + "/analyze", {
                    method: "POST",
                    headers: Object.assign({ "Content-Type": "application/json" }, authHeaders),
                    body: JSON.stringify(scrapedData),
                });
                const rawText = await response.text();
                if (!response.ok || rawText.startsWith("error")) {
                    console.error("Safely: backend error:", rawText.substring(0, 300));
                    if (response.status === 429) {
                        const match = rawText.match(/RATE_LIMITED:(\d+)/);
                        return {
                            error: "rate_limited",
                            retryAfterSeconds: match ? parseInt(match[1], 10) : null,
                        };
                    }
                    if (response.status === 401) {
                        return { error: "unauthorized" };
                    }
                    return null;
                }
                return JSON.parse(rawText);
            }
            catch (error) {
                console.error("Safely: failed to fetch analysis", error.message, error.stack);
                return null;
            }
        },
        submitReport: async function (reportData) {
            try {
                const authHeaders = await getAuthHeaders();
                const response = await fetch(API_BASE + "/report", {
                    method: "POST",
                    headers: Object.assign({ "Content-Type": "application/json" }, authHeaders),
                    body: JSON.stringify(reportData),
                });
                const rawText = await response.text();
                if (!response.ok || rawText.startsWith("error")) {
                    console.error("Safely: report error:", rawText.substring(0, 300));
                    return response.status === 401 ? { error: "unauthorized" } : null;
                }
                return JSON.parse(rawText);
            }
            catch (error) {
                console.error("Safely: failed to submit report", error.message, error.stack);
                return null;
            }
        },
        fetchAnalysis: async function () {
            const platform = window.__safelyScrapers.detectPlatform();
            if (platform === "unknown") {
                window.dispatchEvent(new CustomEvent("safely-analysis-finished"));
                return;
            }
            // Defense-in-depth: panel.js already gates this, but checking
            // again here keeps this safe even if something calls it directly.
            if (!window.__safelyScrapers.isListingPage()) {
                window.dispatchEvent(new CustomEvent("safely-analysis-finished"));
                return;
            }
            const listing_url = window.location.href;
            let scraped = {};
            if (platform === "olx") {
                await new Promise((resolve) => setTimeout(resolve, 1500));
                scraped = window.__safelyScrapers.scrapeOLX();
            }
            const domainCheck = window.__safelyScrapers.checkDomain();
            const payload = {
                platform,
                listing_url,
                seller_id: null,
                listing_id: scraped.listing_id || null,
                title: scraped.title || null,
                price: scraped.price || null,
                description: scraped.description || null,
                category: null,
                image_urls: scraped.image_urls || null,
                posted_date: null,
                platform_id: scraped.platform_id || null,
                seller_name: scraped.seller_name || null,
                seller_handle: null,
                seller_phone: null,
                seller_profile_url: scraped.seller_profile_url || null,
                seller_join_date: scraped.seller_join_date || null,
                seller_location: scraped.seller_location || null,
                seller_last_active: scraped.seller_last_active || null,
                domain_check_status: domainCheck ? domainCheck.status : null,
                domain_check_real_name: domainCheck ? domainCheck.realName : null,
                domain_check_real_domain: domainCheck ? domainCheck.realDomain : null,
                domain_check_current_domain: domainCheck
                    ? domainCheck.currentDomain || window.location.hostname
                    : null,
                domain_check_current_html: domainCheck ? domainCheck.currentDomainHtml || null : null,
                domain_check_real_html: domainCheck ? domainCheck.realDomainHtml || null : null,
            };
            const data = await window.__safelyAPI.analyze(payload);
            if (!data || data.error) {
                window.dispatchEvent(new CustomEvent("safely-analysis-finished", {
                    detail: {
                        error: data && data.error ? data.error : "generic",
                        retryAfterSeconds: data && data.retryAfterSeconds,
                    },
                }));
                return;
            }
            window.__safelyData = {
                riskScore: data.risk_score,
                fraudReportCount: data.fraud_report_count,
                seller: {
                    name: data.seller.name || "Unknown",
                    platform: data.seller.platform || scraped.platform || "unknown",
                    platformId: data.seller.platform_id || null,
                    handle: data.seller.handle || "",
                    accountAge: data.seller.account_age,
                    verification: data.seller.verification,
                    location: data.seller.location || "Unknown",
                    lastActive: scraped.seller_last_active || data.seller.last_active || "Unknown",
                    networkSummary: data.seller.network_summary,
                    monthlyActivity: data.seller.monthly_activity,
                },
                signals: data.signals.map((s) => ({
                    label: s.label,
                    sub: s.sub,
                    value: s.value,
                    type: s.type,
                })),
            };
            window.dispatchEvent(new CustomEvent("safely-data-ready"));
            window.dispatchEvent(new CustomEvent("safely-analysis-finished"));
        },
    };
})();
