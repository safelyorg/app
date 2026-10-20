import { describe, it, expect, beforeAll } from "vitest";
import { readText } from "./load-script";

// Every dashboard text used in code must have a Portuguese version in
// locales/pt-br.json, so a PT user never sees a stray English line.

let portuguese: Record<string, string> = {};

beforeAll(async () => {
  portuguese = JSON.parse(await readText("locales/pt-br.json"));
});

async function keysIn(file: string): Promise<string[]> {
  const source = await readText(file);
  return [...new Set([...source.matchAll(/"(dash\.[a-z0-9_.]+)"/g)].map((m) => m[1]))];
}

describe("Portuguese dashboard texts", () => {
  for (const file of ["ts/detail_view.ts", "ts/settings.ts", "ts/init.ts"]) {
    it("has every text used in " + file, async () => {
      const keys = await keysIn(file);
      expect(keys.filter((key) => !(key in portuguese))).toEqual([]);
    });
  }

  it("has the Light / Dark choice", () => {
    expect(portuguese["dash.settings.light"]).toBeTruthy();
    expect(portuguese["dash.settings.dark"]).toBeTruthy();
  });
});
