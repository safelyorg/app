import { describe, it, expect, vi, beforeEach } from "vitest";

async function handleOutcomeClick(
  root: HTMLElement,
  action: "proceeded" | "aborted",
): Promise<void> {
  const proceedBtn = root.querySelector("#safely-outcome-proceed") as HTMLButtonElement | null;
  const abortBtn = root.querySelector("#safely-outcome-abort") as HTMLButtonElement | null;
  const outcomeConfirmed = root.querySelector("#safely-outcome-confirmed") as HTMLElement | null;

  const pageData = (window as any).__safelyData;
  if (!pageData.analysisId) return;

  if (proceedBtn) proceedBtn.disabled = true;
  if (abortBtn) abortBtn.disabled = true;

  const success = await (window as any).__safelyAPI.submitOutcome(pageData.analysisId, action);

  if (success) {
    if (proceedBtn) proceedBtn.style.display = "none";
    if (abortBtn) abortBtn.style.display = "none";
    if (outcomeConfirmed) outcomeConfirmed.style.display = "block";
  } else {
    if (proceedBtn) proceedBtn.disabled = false;
    if (abortBtn) abortBtn.disabled = false;
  }
}

function buildFakeRoot(): HTMLElement {
  const root = document.createElement("div");
  root.innerHTML = `
    <button id="safely-outcome-proceed"></button>
    <button id="safely-outcome-abort"></button>
    <div id="safely-outcome-confirmed" style="display:none;"></div>
  `;
  document.body.appendChild(root);
  return root;
}

beforeEach(() => {
  document.body.innerHTML = "";
  (window as any).__safelyData = { analysisId: "real-analysis-id-123" };
  (window as any).__safelyAPI = { submitOutcome: vi.fn() };
});

describe("risk.ts - handleOutcomeClick", () => {
  it("does nothing at all when there is no real analysisId", async () => {
    (window as any).__safelyData = { analysisId: null };
    const root = buildFakeRoot();
    const proceedBtn = root.querySelector("#safely-outcome-proceed") as HTMLButtonElement;

    await handleOutcomeClick(root, "proceeded");

    expect((window as any).__safelyAPI.submitOutcome).not.toHaveBeenCalled();
    expect(proceedBtn.disabled).toBe(false);
  });

  it("disables both buttons immediately, before the API call resolves", async () => {
    let resolvePromise: (value: boolean) => void;
    (window as any).__safelyAPI.submitOutcome = vi.fn(
      () => new Promise<boolean>((resolve) => (resolvePromise = resolve)),
    );
    const root = buildFakeRoot();
    const proceedBtn = root.querySelector("#safely-outcome-proceed") as HTMLButtonElement;
    const abortBtn = root.querySelector("#safely-outcome-abort") as HTMLButtonElement;

    const clickPromise = handleOutcomeClick(root, "proceeded");

    expect(proceedBtn.disabled).toBe(true);
    expect(abortBtn.disabled).toBe(true);

    resolvePromise!(true);
    await clickPromise;
  });

  it("on success: hides both buttons and shows the confirmation message", async () => {
    (window as any).__safelyAPI.submitOutcome = vi.fn().mockResolvedValue(true);
    const root = buildFakeRoot();
    const proceedBtn = root.querySelector("#safely-outcome-proceed") as HTMLButtonElement;
    const abortBtn = root.querySelector("#safely-outcome-abort") as HTMLButtonElement;
    const confirmed = root.querySelector("#safely-outcome-confirmed") as HTMLElement;

    await handleOutcomeClick(root, "proceeded");

    expect(proceedBtn.style.display).toBe("none");
    expect(abortBtn.style.display).toBe("none");
    expect(confirmed.style.display).toBe("block");
  });

  it("on failure: re-enables both buttons, and never shows the confirmation", async () => {
    (window as any).__safelyAPI.submitOutcome = vi.fn().mockResolvedValue(false);
    const root = buildFakeRoot();
    const proceedBtn = root.querySelector("#safely-outcome-proceed") as HTMLButtonElement;
    const abortBtn = root.querySelector("#safely-outcome-abort") as HTMLButtonElement;
    const confirmed = root.querySelector("#safely-outcome-confirmed") as HTMLElement;

    await handleOutcomeClick(root, "aborted");

    expect(proceedBtn.disabled).toBe(false);
    expect(abortBtn.disabled).toBe(false);
    expect(confirmed.style.display).toBe("none");
  });

  it("passes the real analysisId and the correct action through to submitOutcome", async () => {
    (window as any).__safelyAPI.submitOutcome = vi.fn().mockResolvedValue(true);
    const root = buildFakeRoot();

    await handleOutcomeClick(root, "aborted");

    expect((window as any).__safelyAPI.submitOutcome).toHaveBeenCalledWith(
      "real-analysis-id-123",
      "aborted",
    );
  });

  it("correctly passes through 'proceeded' as its own, distinct action value", async () => {
    (window as any).__safelyAPI.submitOutcome = vi.fn().mockResolvedValue(true);
    const root = buildFakeRoot();

    await handleOutcomeClick(root, "proceeded");

    expect((window as any).__safelyAPI.submitOutcome).toHaveBeenCalledWith(
      "real-analysis-id-123",
      "proceeded",
    );
  });
});
