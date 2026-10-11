"use strict";
// Session handling. Runs first, before anything else.
(function () {
    "use strict";
    const STORAGE_KEY = "safely_session_token";
    // Where the website saved the ad a visitor came from (site/ts/tracking.ts).
    const UTM_STORAGE_KEY = "safely_utm";
    // True right after a brand-new account signed in for the first time
    // (the server sends it to "/dashboard/?new=1#session=..." only then).
    let isFirstSignIn = false;
    function captureSessionFromUrl() {
        const hash = window.location.hash;
        if (hash.indexOf("#session=") === 0) {
            const token = hash.slice("#session=".length);
            if (token) {
                localStorage.setItem(STORAGE_KEY, token);
            }
            // "?new=1" counts only together with a fresh session, and is
            // removed from the address so a reload does not count it again.
            const search = new URLSearchParams(window.location.search);
            isFirstSignIn = search.get("new") === "1";
            search.delete("new");
            const rest = search.toString();
            history.replaceState(null, "", window.location.pathname + (rest ? "?" + rest : ""));
        }
    }
    // First sign-in of a new account: tells the Meta Pixel (a sign-up for
    // the ads) and saves the ad the person came from with the account.
    // Runs once - the "new=1" mark is removed from the address right away.
    async function recordFirstSignIn(token) {
        const fbq = window.fbq;
        if (typeof fbq === "function")
            fbq("track", "CompleteRegistration");
        let source = null;
        try {
            const saved = localStorage.getItem(UTM_STORAGE_KEY);
            source = saved ? JSON.parse(saved) : null;
        }
        catch (e) {
            source = null;
        }
        if (!source || !source.utm_source)
            return;
        try {
            const response = await fetch("/api/v1/auth/signup-source", {
                method: "POST",
                headers: { "Content-Type": "application/json", Authorization: "Bearer " + token },
                body: JSON.stringify({
                    utm_source: source.utm_source || null,
                    utm_medium: source.utm_medium || null,
                    utm_campaign: source.utm_campaign || null,
                    utm_content: source.utm_content || null,
                }),
            });
            if (response.ok)
                localStorage.removeItem(UTM_STORAGE_KEY);
        }
        catch (e) {
            console.error("Safely: could not save where this sign-up came from", e);
        }
    }
    // Reads a ?error=... left in the URL by a failed auth attempt (e.g.
    // clicking an already-used magic link) and shows an honest message
    // about it, without implying the person isn't genuinely signed in.
    function checkForAuthErrorInUrl() {
        const params = new URLSearchParams(window.location.search);
        const error = params.get("error");
        if (!error)
            return;
        const messages = {
            expired_link: "That sign-in link had already been used or expired. You're still signed in from before, so nothing else to do here.",
            server_error: "Something went wrong on our end during that last sign-in attempt. If you're seeing this and aren't signed in, please try again.",
            google_denied: "Google sign-in was cancelled.",
            google_exchange_failed: "Couldn't complete Google sign-in. Please try again.",
            state_mismatch: "That sign-in attempt couldn't be verified. Please try again.",
            google_email_mismatch: "That Google account's email doesn't match your account's email, so it wasn't connected.",
            google_already_linked: "That Google account is already connected to a different Safely account.",
            session_expired: "Your session had expired. Please sign in again.",
        };
        if (window.showToast) {
            window.showToast(messages[error] || "Something didn't go as expected. Please try again.", 5000);
        }
        else {
            console.warn("Safely: showToast not yet available:", messages[error] || error);
        }
        // Clean the URL right after reading it, so a refresh doesn't show
        // this same message again.
        history.replaceState(null, "", window.location.pathname);
    }
    function getToken() {
        return localStorage.getItem(STORAGE_KEY);
    }
    function clearToken() {
        localStorage.removeItem(STORAGE_KEY);
    }
    async function renderAuthState() {
        const token = getToken();
        const app = document.getElementById("app");
        const gate = document.getElementById("login-gate");
        if (!token) {
            if (app)
                app.classList.add("hidden");
            if (gate)
                gate.classList.remove("hidden");
            return;
        }
        try {
            const response = await fetch("/api/v1/me", {
                headers: { Authorization: "Bearer " + token },
            });
            if (response.ok) {
                if (app)
                    app.classList.remove("hidden");
                if (gate)
                    gate.classList.add("hidden");
                if (isFirstSignIn) {
                    isFirstSignIn = false;
                    recordFirstSignIn(token);
                }
            }
            else {
                clearToken();
                if (app)
                    app.classList.add("hidden");
                if (gate)
                    gate.classList.remove("hidden");
            }
        }
        catch (e) {
            clearToken();
            if (app)
                app.classList.add("hidden");
            if (gate)
                gate.classList.remove("hidden");
        }
    }
    window.safelyAuth = {
        getToken,
        clearToken,
        authHeader: () => {
            const token = getToken();
            return token ? { Authorization: "Bearer " + token } : {};
        },
        logout: async () => {
            const token = getToken();
            if (token) {
                try {
                    await fetch("/api/v1/auth/logout", {
                        method: "POST",
                        headers: { Authorization: "Bearer " + token },
                    });
                }
                catch (e) {
                    console.error("Safely: logout request failed", e);
                }
            }
            clearToken();
            window.location.href = "/";
        },
    };
    // Attaches the real session token to every HTMX request automatically.
    document.body.addEventListener("htmx:configRequest", (event) => {
        const token = getToken();
        if (token) {
            event.detail.headers["Authorization"] = "Bearer " + token;
        }
    });
    captureSessionFromUrl();
    renderAuthState().then(() => {
        checkForAuthErrorInUrl();
    });
    const logoutBtn = document.getElementById("btnLogout");
    if (logoutBtn) {
        logoutBtn.addEventListener("click", (e) => {
            e.preventDefault();
            window.safelyAuth.logout();
        });
    }
})();
