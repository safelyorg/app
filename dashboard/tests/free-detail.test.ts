import { describe, it, expect, vi, beforeEach } from "vitest";

// Copy of detail_view.ts's Free plan logic: the PDF and Details are
// for Team and Enterprise; the server sends full_result: false for Free.

let currentFullResult = true;
const showToast = vi.fn();

function fullFrom(data: { full_result?: boolean }): boolean {
  return data.full_result !== false;
}

async function downloadEvidencePdf(fetchFn: () => Promise<{ status: number; ok: boolean }>): Promise<string> {
  if (!currentFullResult) {
    showToast("The PDF report is on the Team and Enterprise plans.");
    return "toast";
  }
  const res = await fetchFn();
  if (res.status === 403) {
    showToast("The PDF report is on the Team and Enterprise plans.");
    return "toast";
  }
  return res.ok ? "download" : "error";
}

describe("detail view - Free plan", () => {
  beforeEach(() => {
    showToast.mockClear();
  });

  it("old answers without full_result count as full", () => {
    expect(fullFrom({})).toBe(true);
    expect(fullFrom({ full_result: true })).toBe(true);
    expect(fullFrom({ full_result: false })).toBe(false);
  });

  it("Free: no PDF request, a toast instead", async () => {
    currentFullResult = false;
    const fetchFn = vi.fn();
    expect(await downloadEvidencePdf(fetchFn)).toBe("toast");
    expect(fetchFn).not.toHaveBeenCalled();
    expect(showToast).toHaveBeenCalledOnce();
  });

  it("a 403 from the server shows the same toast", async () => {
    currentFullResult = true;
    const fetchFn = vi.fn().mockResolvedValue({ status: 403, ok: false });
    expect(await downloadEvidencePdf(fetchFn)).toBe("toast");
  });

  it("paid: the PDF downloads", async () => {
    currentFullResult = true;
    const fetchFn = vi.fn().mockResolvedValue({ status: 200, ok: true });
    expect(await downloadEvidencePdf(fetchFn)).toBe("download");
    expect(showToast).not.toHaveBeenCalled();
  });
});
