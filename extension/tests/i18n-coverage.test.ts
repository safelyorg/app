import { describe, it, expect, beforeAll } from "vitest";
import { fakeChrome } from "./setup-chrome";
import { readText, runScript } from "./tab-helpers";

// Every text the panel shows must have a Portuguese version, so a PT
// buyer never sees a stray English line. Loads the real core/i18n.ts.

const SAME_IN_PORTUGUESE = ["Status", "1 link", "{n} links"];

let i18n: any;

beforeAll(async () => {
  (fakeChrome.storage as any).onChanged = { addListener: () => {} };
  (fakeChrome.storage.local as any).get = (_key: unknown, cb?: (r: any) => void) => {
    if (cb) cb({});
  };
  await runScript("../ts/core/i18n");
  i18n = (window as any).__safelyI18n;
  i18n.setLang("pt-br");
});

function hasPortuguese(en: string): boolean {
  return SAME_IN_PORTUGUESE.indexOf(en) !== -1 || i18n.t(en) !== en;
}

// Every tr("...") written straight into a file.
async function shownTexts(file: string): Promise<string[]> {
  const src = await readText("ts/" + file);
  return [...src.matchAll(/tr\(\s*"((?:[^"\\]|\\.)*)"/g)].map((m) => JSON.parse('"' + m[1] + '"'));
}

describe("Portuguese texts", () => {
  for (const file of ["tabs/risk.ts", "tabs/intelligence.ts", "core/panel.ts"]) {
    it("has every text in " + file, async () => {
      const missing = (await shownTexts(file)).filter((en) => !hasPortuguese(en));
      expect(missing).toEqual([]);
    });
  }

  it("has the texts that are kept in lists", () => {
    const texts = [
      // Score card
      "Low risk",
      "Caution",
      "High risk",
      "No serious warning signs.",
      "Check these points before you pay.",
      "Don't pay this supplier yet.",
      // Findings
      "Serious",
      "Pattern match",
      "Worth noting",
      // Report reasons
      "Took payment and never shipped",
      "Goods were fake or not as described",
      "Sent the wrong goods",
      "Stopped replying after payment",
      "Something else looked like a scam",
      // Report result
      "You reported this supplier.",
      "You already reported this supplier.",
      // Waiting screen
      "Reading the listing",
      "Checking the seller account",
      "Looking up the website",
      "Searching the web for reports",
      "Comparing the price",
      "Checking payment terms",
      "Working out the score",
      // Before you pay (supplier)
      "Use buyer protection if the platform offers it",
      "Match the business licence to the bank account",
      "Confirm bank details by phone or video",
      "Order a sample first",
      "Ask for a live video call",
      "Don't pay everything before shipment",
      "Do not pay by Western Union, MoneyGram or crypto",
      "Don't pay fees before an inspection",
    ];
    expect(texts.filter((en) => !hasPortuguese(en))).toEqual([]);
  });

  it("fills in numbers and names in Portuguese", () => {
    expect(i18n.t("Show {n} more", { n: 3 })).toBe("Mostrar mais 3");
    expect(i18n.t("Not found on {list}", { list: "Reddit" })).toBe("Não encontrado em Reddit");
  });

  it("goes back to English", () => {
    i18n.setLang("en");
    expect(i18n.t("You already reported this supplier.")).toBe(
      "You already reported this supplier.",
    );
    i18n.setLang("pt-br");
    expect(i18n.t("You already reported this supplier.")).toBe(
      "Você já denunciou este fornecedor.",
    );
  });
});
