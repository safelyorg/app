// The "Team" card in Settings: who can use this account's plan.
//
// - Free: 1 person (only you). The card says Team or Enterprise is
//   needed to invite people.
// - Team: 3 people (you + 2). Extra seats are $25 each and are bought
//   here (Creem checkout the first time, then changed in place).
// - Enterprise: unlimited people.
//
// Someone you invite uses your plan once they sign in to Safely with
// that email. A person who was invited sees whose team they are in and
// can leave it. The server is backend/handlers/team.rs.

let teamSeatsWanted = 0;

function teamHeaders(json: boolean): Record<string, string> {
  const auth = (window as any).safelyAuth.authHeader() as Record<string, string>;
  return json ? Object.assign({ "Content-Type": "application/json" }, auth) : auth;
}

// The message for an error code from the server (TeamError::code).
function teamErrorText(code: string): string {
  const texts: Record<string, [string, string]> = {
    invalid_email: ["dash.team.err_invalid_email", "That doesn't look like an email address."],
    own_email: ["dash.team.err_own_email", "That's your own email - you're already on your plan."],
    needs_team_plan: ["dash.team.err_needs_team", "You need the Team or Enterprise plan to invite people."],
    no_seats_left: ["dash.team.err_no_seats", "Every seat is used. Add an extra seat or remove someone first."],
    already_in_a_team: ["dash.team.err_in_a_team", "This person is already in a team."],
    has_own_plan: [
      "dash.team.err_has_own_plan",
      "This person already pays for their own plan. Ask them to cancel it first, so nobody pays twice.",
    ],
    not_found: ["dash.team.err_not_found", "That person isn't in your team."],
    too_many_seats: ["dash.team.err_too_many", "For more than 50 extra seats, the Enterprise plan is better."],
    remove_people_first: ["dash.team.err_remove_first", "You have more people than seats. Remove someone first."],
    seats_unavailable: ["dash.team.err_seats_unavailable", "Extra seats can't be bought yet. Please write to help@safely.sh."],
    payment_failed: ["dash.team.err_payment", "The payment page could not be opened. Please try again."],
  };
  const entry = texts[code];
  return entry ? t(entry[0], entry[1]) : t("dash.common.save_failed", "Could not save. Please try again.");
}

function teamEl<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  if (className) el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

function teamMessage(text: string, good: boolean): void {
  const box = document.getElementById("team-message");
  if (!box) return;
  box.textContent = text;
  box.className =
    "mt-3 px-3 py-2.5 rounded-lg text-[12px] font-semibold " +
    (good ? "bg-mint/10 text-mint" : "bg-coral/10 text-coral");
}

// Shows the plan again in "Plan & Billing" (init.ts), after something
// here changed which plan this person uses.
function reloadPlanCard(): void {
  const reload = (window as any).safelyReloadPlan;
  if (typeof reload === "function") reload();
}

async function teamPost(path: string, body: unknown): Promise<any> {
  const res = await fetch(API_BASE + "/team" + path, {
    method: "POST",
    headers: teamHeaders(true),
    body: JSON.stringify(body ?? {}),
  });
  if (res.status === 401) {
    (window as any).safelyAuth.logout();
    throw new Error("signed out");
  }
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(String(data.error || "failed"));
  return data;
}

function planName(plan: string): string {
  if (plan === "Team") return t("dash.plan.team", "Team");
  if (plan === "Enterprise") return t("dash.plan.enterprise", "Enterprise");
  return t("dash.team.free", "Free");
}

function renderMemberView(box: HTMLElement, data: any): void {
  const text = t("dash.team.member_of", "You use {email}'s {plan} plan.")
    .replace("{email}", data.owner_email || "")
    .replace("{plan}", planName(data.plan));
  box.append(teamEl("div", "text-[13px] font-semibold", text));
  if (!data.has_seat) {
    box.append(
      teamEl(
        "div",
        "text-[12px] text-coral mt-1.5",
        t("dash.team.no_seat_for_you", "Their plan has no seat for you right now, so you are on the Free plan."),
      ),
    );
  }
  const leaveBtn = teamEl(
    "button",
    "mt-3 px-3.5 py-1.5 rounded-lg bg-surface2 border border-line text-[12px] font-bold hover:bg-surface3 transition",
    t("dash.team.leave", "Leave team"),
  );
  leaveBtn.type = "button";
  leaveBtn.addEventListener("click", async () => {
    leaveBtn.disabled = true;
    try {
      await teamPost("/leave", {});
      showToast(t("dash.team.left", "You left the team."));
      loadTeam();
      reloadPlanCard();
    } catch (e) {
      teamMessage(teamErrorText((e as Error).message), false);
      leaveBtn.disabled = false;
    }
  });
  box.append(leaveBtn);
}

function renderOwnerView(box: HTMLElement, data: any): void {
  const limit: number | null = data.seat_limit;
  const used: number = data.seats_used;
  const canInvite = data.plan !== "Free" && limit !== 1;

  // "2 of 3 seats used" / "4 people · unlimited"
  const head = teamEl("div", "flex items-center justify-between gap-3");
  const seatsText =
    limit === null
      ? t("dash.team.seats_unlimited", "{used} people · unlimited seats").replace("{used}", String(used))
      : t("dash.team.seats_used", "{used} of {limit} seats used")
          .replace("{used}", String(used))
          .replace("{limit}", String(limit));
  head.append(teamEl("div", "text-[13px] font-bold", seatsText));
  head.append(teamEl("span", "text-[11px] text-muted", planName(data.plan)));
  box.append(head);

  if (!canInvite) {
    box.append(
      teamEl(
        "div",
        "text-[12px] text-muted mt-1.5 leading-relaxed",
        t(
          "dash.team.free_hint",
          "The Free plan is for 1 person. With Team, 3 people can use your plan; with Enterprise, as many as you need.",
        ),
      ),
    );
    return;
  }

  // The people invited
  const list = teamEl("div", "mt-3 divide-y divide-line border border-line rounded-xl");
  const you = teamEl("div", "flex items-center justify-between gap-3 px-3 py-2.5 text-[12.5px]");
  you.append(teamEl("span", "font-semibold", t("dash.team.you", "You (owner)")));
  list.append(you);
  (data.members as any[]).forEach((m) => {
    const row = teamEl("div", "flex items-center justify-between gap-3 px-3 py-2.5 text-[12.5px]");
    const left = teamEl("div", "min-w-0");
    left.append(teamEl("div", "font-semibold truncate", m.email));
    const status = !m.has_seat
      ? t("dash.team.status_no_seat", "No seat - add a seat to give access")
      : m.joined
        ? t("dash.team.status_joined", "Joined")
        : t("dash.team.status_invited", "Invited - not signed in yet");
    left.append(teamEl("div", "text-[11px] " + (m.has_seat ? "text-muted" : "text-coral"), status));
    row.append(left);
    const removeBtn = teamEl(
      "button",
      "flex-shrink-0 px-2.5 py-1 rounded-md text-coral text-[11px] font-semibold hover:bg-coral/10 transition",
      t("dash.team.remove", "Remove"),
    );
    removeBtn.type = "button";
    removeBtn.addEventListener("click", async () => {
      removeBtn.disabled = true;
      try {
        await teamPost("/remove", { email: m.email });
        loadTeam();
      } catch (e) {
        teamMessage(teamErrorText((e as Error).message), false);
        removeBtn.disabled = false;
      }
    });
    row.append(removeBtn);
    list.append(row);
  });
  box.append(list);

  // Invite one more
  const form = teamEl("form", "mt-3 flex gap-2");
  const input = teamEl("input", "flex-1 min-w-0 px-3 py-2 bg-surface2 border border-line rounded-lg text-[13px] outline-none focus:border-brand");
  input.type = "email";
  input.id = "team-invite-email";
  input.placeholder = t("dash.team.invite_placeholder", "colleague@company.com");
  input.autocomplete = "off";
  const inviteBtn = teamEl(
    "button",
    "flex-shrink-0 px-3.5 py-2 rounded-lg bg-ink text-bg text-[12px] font-bold hover:opacity-90 transition disabled:opacity-40",
    t("dash.team.invite", "Invite"),
  );
  inviteBtn.type = "submit";
  const full = limit !== null && used >= limit;
  input.disabled = full;
  inviteBtn.disabled = full;
  form.append(input, inviteBtn);
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    inviteBtn.disabled = true;
    try {
      await teamPost("/invite", { email: input.value });
      showToast(t("dash.team.invited", "Invite sent."));
      loadTeam();
    } catch (err) {
      teamMessage(teamErrorText((err as Error).message), false);
      inviteBtn.disabled = false;
    }
  });
  box.append(form);
  box.append(
    teamEl(
      "div",
      "text-[11px] text-muted mt-1.5 leading-relaxed",
      full
        ? t("dash.team.full_hint", "Every seat is used. Add an extra seat below, or remove someone.")
        : t("dash.team.invite_hint", "They get an email, and use your plan once they sign in to Safely with this address."),
    ),
  );

  // Extra seats (Team only)
  if (data.can_buy_seats) {
    teamSeatsWanted = data.extra_seats;
    const seats = teamEl("div", "mt-4 pt-3 border-t border-line flex items-center justify-between gap-3");
    const label = teamEl("div", "min-w-0");
    label.append(teamEl("div", "text-[12.5px] font-semibold", t("dash.team.extra_seats", "Extra seats")));
    label.append(
      teamEl(
        "div",
        "text-[11px] text-muted",
        t("dash.team.extra_price", "${price} per seat a month, on top of your 3 seats").replace(
          "{price}",
          String(data.seat_price_usd),
        ),
      ),
    );
    seats.append(label);
    const controls = teamEl("div", "flex items-center gap-1.5 flex-shrink-0");
    const minus = teamEl("button", "w-7 h-7 rounded-md bg-surface2 border border-line text-[14px] font-bold", "−");
    const count = teamEl("span", "w-7 text-center text-[13px] font-bold num", String(teamSeatsWanted));
    const plus = teamEl("button", "w-7 h-7 rounded-md bg-surface2 border border-line text-[14px] font-bold", "+");
    const save = teamEl(
      "button",
      "ml-1 px-3 py-1.5 rounded-lg bg-ink text-bg text-[11px] font-bold disabled:opacity-40",
      t("dash.team.save", "Save"),
    );
    [minus, plus, save].forEach((b) => (b.type = "button"));
    save.disabled = true;
    const show = (): void => {
      count.textContent = String(teamSeatsWanted);
      save.disabled = teamSeatsWanted === data.extra_seats;
    };
    minus.addEventListener("click", () => {
      teamSeatsWanted = Math.max(0, teamSeatsWanted - 1);
      show();
    });
    plus.addEventListener("click", () => {
      teamSeatsWanted = Math.min(data.max_extra_seats, teamSeatsWanted + 1);
      show();
    });
    save.addEventListener("click", async () => {
      save.disabled = true;
      try {
        const res = await teamPost("/seats", { extra_seats: teamSeatsWanted });
        if (res.result === "checkout" && res.checkout_url) {
          window.location.href = res.checkout_url;
          return;
        }
        showToast(t("dash.team.seats_saved", "Seats updated."));
        loadTeam();
      } catch (e) {
        teamMessage(teamErrorText((e as Error).message), false);
        show();
      }
    });
    controls.append(minus, count, plus, save);
    seats.append(controls);
    box.append(seats);
  }
}

async function loadTeam(): Promise<void> {
  const box = document.getElementById("team-body");
  if (!box) return;
  try {
    const res = await fetch(API_BASE + "/team", { headers: teamHeaders(false) });
    if (res.status === 401) return;
    if (!res.ok) throw new Error("failed");
    const data = await res.json();
    box.replaceChildren();
    // The "Loading..." look (small, grey) is only for the loading text.
    box.className = "text-ink";
    if (data.role === "member") renderMemberView(box, data);
    else renderOwnerView(box, data);
    const message = teamEl("div", "hidden");
    message.id = "team-message";
    box.append(message);
  } catch (e) {
    box.textContent = t("dash.team.load_failed", "Could not load your team.");
  }
}

// Back from Creem after buying extra seats.
(function () {
  const params = new URLSearchParams(window.location.search);
  if (params.get("seats") !== "success") return;
  const url = new URL(window.location.href);
  url.searchParams.delete("seats");
  history.replaceState(null, "", url.pathname + url.hash);
  window.addEventListener("load", () => {
    showToast(t("dash.team.seats_bought", "Thank you! Your extra seats are ready in a moment."));
  });
})();
