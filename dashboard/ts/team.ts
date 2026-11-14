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
    plan_ending: ["dash.team.err_plan_ending", "Your plan is ending. Press \"Keep my plan\" first to add seats."],
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
  const members = (data.members as any[]) || [];
  // Free with people still in the list: the team plan ended.
  const ended = !canInvite && members.length > 0;

  // Paid: "2 of 3 seats used" / "4 people · unlimited".
  // Free: "Only you", or "Your team plan has ended".
  const head = teamEl("div", "flex items-center justify-between gap-3");
  const seatsText = !canInvite
    ? ended
      ? t("dash.team.ended_title", "Your team plan has ended")
      : t("dash.team.free_title", "Only you")
    : limit === null
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
        ended
          ? t(
              "dash.team.ended_hint",
              "These people are on the Free plan now. They get your plan back when you subscribe again.",
            )
          : t(
              "dash.team.free_hint",
              "The Free plan is for 1 person. With Team, 3 people can use your plan; with Enterprise, as many as you need.",
            ),
      ),
    );
    if (!ended) return;
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
    // No paid plan (it ended): the member is on Free until the owner
    // subscribes again. Paid plan but too few seats: add a seat.
    const status = !m.has_seat
      ? data.plan === "Free"
        ? t("dash.team.status_on_free", "On the Free plan")
        : t("dash.team.status_no_seat", "No seat - add a seat to give access")
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
  // No new invites until the plan is paid again.
  if (!canInvite) return;

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

  // Extra seats (Team only): pick a number, then Add or Remove, with a
  // short confirm that says what it costs.
  if (data.can_buy_seats) {
    box.append(buildSeatControls(data));
  }
}

// What the owner can do with extra seats. The 3 seats in the plan can
// never be removed; an extra seat can be removed only when nobody uses it.
function seatPlan(data: any): { extra: number; maxAdd: number; maxRemove: number } {
  const extra: number = data.extra_seats || 0;
  const freeSeats = Math.max(0, (data.seat_limit || 0) - (data.seats_used || 0));
  return {
    extra,
    maxAdd: Math.max(0, (data.max_extra_seats || 0) - extra),
    maxRemove: Math.min(extra, freeSeats),
  };
}

function seatWord(n: number): string {
  return n === 1 ? t("dash.team.seat_one", "seat") : t("dash.team.seat_many", "seats");
}

// A small line icon (stroke follows the text colour).
function seatIcon(paths: string, size = 14): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2.2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.innerHTML = paths;
  return svg;
}

const SEAT_ICON_PLUS = '<path d="M12 5v14M5 12h14"/>';
const SEAT_ICON_MINUS = '<path d="M5 12h14"/>';
const SEAT_ICON_CARD = '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M2 10h20"/>';
const SEAT_ICON_INFO = '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>';

// Extra seats: a summary (in your plan / extra / total), then the -/+
// counter with "Add n" and, once there are extra seats, "Remove n".
// "Add n": the first time the payment page for n seats ($25 each a
// month); after that no page, the saved card is charged for the days
// left this month. "Remove n" takes away n free seats at once: no refund,
// nothing charged for them again.
function buildSeatControls(data: any): HTMLElement {
  const plan = seatPlan(data);
  const price: number = Number(data.seat_price_usd) || 0;
  const included = Math.max(0, (data.seat_limit || 0) - plan.extra);
  const wrap = teamEl("div", "mt-5 pt-4 border-t border-line");

  // Title + price pill.
  const head = teamEl("div", "flex items-center justify-between gap-3");
  head.append(teamEl("div", "text-[13px] font-bold", t("dash.team.extra_title", "Extra seats")));
  head.append(
    teamEl(
      "span",
      "px-2 py-0.5 rounded-full bg-surface2 border border-line text-[10.5px] font-semibold text-muted whitespace-nowrap",
      t("dash.team.per_seat", "${price} / seat / month").replace("{price}", String(price)),
    ),
  );
  wrap.append(head);

  // In your plan · Extra · Total
  const stats = teamEl("div", "grid grid-cols-3 mt-3 rounded-xl border border-line overflow-hidden");
  stats.setAttribute("data-seat-stats", "");
  const stat = (value: number, label: string, strong: boolean, last: boolean): HTMLElement => {
    const cell = teamEl("div", `px-3.5 py-2.5 ${strong ? "bg-surface2" : ""} ${last ? "" : "border-r border-line"}`);
    cell.append(teamEl("div", "text-[18px] font-bold leading-tight tabular-nums", String(value)));
    cell.append(teamEl("div", "text-[10.5px] font-semibold text-muted uppercase tracking-wide mt-0.5", label));
    return cell;
  };
  stats.append(
    stat(included, t("dash.team.stat_plan", "In your plan"), false, false),
    stat(plan.extra, t("dash.team.stat_extra", "Extra"), false, false),
    stat(included + plan.extra, t("dash.team.stat_total", "Total"), true, true),
  );
  wrap.append(stats);

  // [- n +]  [+ Add n]  [- Remove n]           +$50 / month
  const row = teamEl("div", "flex items-center gap-2 mt-3.5 flex-wrap");
  const counter = teamEl("div", "flex items-center h-9 rounded-xl border border-line bg-surface overflow-hidden");
  const stepBtn = "w-9 h-full flex items-center justify-center text-muted hover:text-ink hover:bg-surface2 transition disabled:opacity-30 disabled:hover:bg-transparent disabled:cursor-not-allowed";
  const minus = teamEl("button", stepBtn) as HTMLButtonElement;
  minus.append(seatIcon(SEAT_ICON_MINUS, 13));
  minus.setAttribute("aria-label", "-1");
  const count = teamEl("span", "w-9 text-center text-[13.5px] font-bold tabular-nums border-x border-line h-full flex items-center justify-center", "1");
  const plus = teamEl("button", stepBtn) as HTMLButtonElement;
  plus.append(seatIcon(SEAT_ICON_PLUS, 13));
  plus.setAttribute("aria-label", "+1");
  minus.setAttribute("data-seat-minus", "");
  plus.setAttribute("data-seat-plus", "");
  count.setAttribute("data-seat-count", "");
  counter.append(minus, count, plus);

  const btn = "h-9 px-3.5 rounded-xl text-[12px] font-bold inline-flex items-center gap-1.5 transition disabled:opacity-35 disabled:cursor-not-allowed";
  const add = teamEl("button", `${btn} bg-ink text-bg hover:opacity-90`) as HTMLButtonElement;
  add.setAttribute("data-seat-add", "");
  const addLabel = teamEl("span", "");
  add.append(seatIcon(SEAT_ICON_PLUS, 13), addLabel);
  const remove = teamEl("button", `${btn} bg-surface border border-line text-ink hover:bg-surface2`) as HTMLButtonElement;
  remove.setAttribute("data-seat-remove", "");
  const removeLabel = teamEl("span", "");
  remove.append(seatIcon(SEAT_ICON_MINUS, 13), removeLabel);
  [minus, plus, add, remove].forEach((b) => (b.type = "button"));
  const preview = teamEl("span", "ml-auto text-[11.5px] font-semibold text-muted tabular-nums");
  preview.setAttribute("data-seat-preview", "");
  row.append(counter, add);
  if (plan.extra > 0) row.append(remove);
  row.append(preview);
  wrap.append(row);

  const hint = teamEl("div", "text-[11px] text-amber mt-2 empty:hidden");
  hint.setAttribute("data-seat-hint", "");
  wrap.append(hint);

  // The rule, quietly.
  const note = teamEl("div", "flex items-start gap-1.5 text-[11px] text-muted mt-2.5 leading-relaxed");
  const noteIcon = seatIcon(SEAT_ICON_INFO, 12);
  noteIcon.classList.add("mt-0.5", "flex-shrink-0");
  note.append(
    noteIcon,
    teamEl(
      "span",
      "",
      t(
        "dash.team.seats_rule",
        "Removed seats stop right away, with no refund. The {included} seats in your plan can't be removed.",
      ).replace("{included}", String(included)),
    ),
  );
  wrap.append(note);

  // The confirm box: icon, title, text, buttons.
  const confirm = teamEl("div", "hidden mt-3.5 p-3.5 rounded-xl bg-surface2 border border-line");
  confirm.setAttribute("data-seat-confirm", "");
  const confirmTop = teamEl("div", "flex items-start gap-3");
  const confirmIcon = teamEl("div", "w-8 h-8 rounded-lg bg-surface border border-line flex items-center justify-center flex-shrink-0 text-ink");
  const confirmBody = teamEl("div", "min-w-0");
  const confirmTitle = teamEl("div", "text-[12.5px] font-bold");
  const confirmText = teamEl("div", "text-[11.5px] text-muted leading-relaxed mt-0.5");
  confirmBody.append(confirmTitle, confirmText);
  confirmTop.append(confirmIcon, confirmBody);
  const yes = teamEl("button", "h-8 px-3.5 rounded-lg bg-ink text-bg text-[11.5px] font-bold hover:opacity-90 transition disabled:opacity-40") as HTMLButtonElement;
  const no = teamEl(
    "button",
    "h-8 px-3.5 rounded-lg bg-surface border border-line text-[11.5px] font-bold hover:bg-surface3 transition",
    t("dash.common.cancel", "Cancel"),
  ) as HTMLButtonElement;
  [yes, no].forEach((b) => (b.type = "button"));
  const confirmButtons = teamEl("div", "flex justify-end gap-2 mt-3");
  confirmButtons.append(no, yes);
  confirm.append(confirmTop, confirmButtons);
  wrap.append(confirm);

  let n = 1;
  const most = Math.max(1, plan.maxAdd, plan.maxRemove);
  const draw = (): void => {
    count.textContent = String(n);
    minus.disabled = n <= 1;
    plus.disabled = n >= most;
    addLabel.textContent = t("dash.team.add_n", "Add {n}").replace("{n}", String(n));
    add.disabled = n > plan.maxAdd;
    removeLabel.textContent = t("dash.team.remove_n", "Remove {n}").replace("{n}", String(n));
    remove.disabled = n > plan.maxRemove;
    preview.textContent = t("dash.team.preview_add", "+${total} / month").replace("{total}", String(n * price));
    if (plan.extra > 0 && plan.maxRemove === 0) {
      hint.textContent = t(
        "dash.team.remove_all_used",
        "Every extra seat is in use. Remove a person first to remove a seat.",
      );
    } else if (plan.extra > 0 && n > plan.maxRemove) {
      hint.textContent = t("dash.team.remove_up_to", "You can remove up to {n} now (seats nobody uses).")
        .replace("{n}", String(plan.maxRemove));
    } else {
      hint.textContent = "";
    }
    confirm.classList.add("hidden");
  };
  minus.addEventListener("click", () => {
    n = Math.max(1, n - 1);
    draw();
  });
  plus.addEventListener("click", () => {
    n = Math.min(most, n + 1);
    draw();
  });

  const showConfirm = (icon: string, title: string, text: string, yesText: string): void => {
    confirmIcon.replaceChildren(seatIcon(icon, 15));
    confirmTitle.textContent = title;
    confirmText.textContent = text;
    yes.textContent = yesText;
    confirm.classList.remove("hidden");
  };

  let wanted = plan.extra;
  add.addEventListener("click", () => {
    wanted = plan.extra + n;
    showConfirm(
      SEAT_ICON_CARD,
      t("dash.team.add_title", "Add {n} {seats} · ${total} / month")
        .replace("{n}", String(n))
        .replace("{seats}", seatWord(n))
        .replace("{total}", String(n * price)),
      plan.extra === 0
        ? t(
            "dash.team.add_first_body",
            "You'll go to the payment page this one time. After that, new seats are added without it.",
          )
        : t(
            "dash.team.add_more_body",
            "No payment page: your saved card is charged now for the days left this month, then ${total} a month more with your other seats.",
          ).replace("{total}", String(n * price)),
      t("dash.team.add_yes", "Yes, add"),
    );
  });
  remove.addEventListener("click", () => {
    wanted = plan.extra - n;
    showConfirm(
      SEAT_ICON_MINUS,
      t("dash.team.remove_title", "Remove {n} {seats}")
        .replace("{n}", String(n))
        .replace("{seats}", seatWord(n)),
      t(
        "dash.team.remove_body",
        "Removed right away, and you won't be charged for {them} again. Money already paid is not refunded.",
      ).replace("{them}", n === 1 ? t("dash.team.it", "it") : t("dash.team.them", "them")),
      t("dash.team.remove_yes", "Yes, remove"),
    );
  });
  no.addEventListener("click", () => confirm.classList.add("hidden"));
  yes.addEventListener("click", async () => {
    yes.disabled = true;
    try {
      const res = await teamPost("/seats", { extra_seats: wanted });
      if (res.result === "checkout" && res.checkout_url) {
        window.location.href = res.checkout_url;
        return;
      }
      if (res.result === "added") {
        const added: number = res.added || 0;
        showToast(
          t("dash.team.seats_added", "{n} {seats} added.")
            .replace("{n}", String(added))
            .replace("{seats}", seatWord(added)),
        );
      }
      if (res.result === "removed") {
        const removed: number = res.removed || 0;
        showToast(
          t("dash.team.seats_removed", "{n} {seats} removed. You won't be charged for {them} again.")
            .replace("{n}", String(removed))
            .replace("{seats}", seatWord(removed))
            .replace("{them}", removed === 1 ? t("dash.team.it", "it") : t("dash.team.them", "them")),
        );
      }
      loadTeam();
    } catch (e) {
      confirm.classList.add("hidden");
      teamMessage(teamErrorText((e as Error).message), false);
    } finally {
      yes.disabled = false;
    }
  });
  draw();
  return wrap;
}

async function loadTeam(): Promise<void> {
  const box = document.getElementById("team-body");
  if (!box) return;
  // The History dropdown follows the team (team_history.ts).
  const reloadHistory = (window as any).safelyReloadHistoryChoices;
  if (typeof reloadHistory === "function") reloadHistory();
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
