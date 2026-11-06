import { describe, it, expect } from "vitest";
import { fakeChrome } from "./setup-chrome";

import "../ts/background";

// background.ts: right after installing, open Safely's site with
// "?installed=1" so it can go back to the page the "Add to Chrome"
// button was clicked on and say thank you.
const onInstalled = fakeChrome.runtime.onInstalled.addListener.mock.calls[0][0];

describe("after installing", () => {
  it("opens the local site for a copy loaded unpacked", () => {
    fakeChrome.tabs.create.mockClear();
    fakeChrome.runtime.getManifest.mockReturnValue({});
    onInstalled({ reason: "install" });
    expect(fakeChrome.tabs.create).toHaveBeenCalledWith({ url: "http://localhost:3000/?installed=1" });
  });

  it("opens safely.sh for the Chrome Web Store version", () => {
    fakeChrome.tabs.create.mockClear();
    fakeChrome.runtime.getManifest.mockReturnValue({ update_url: "https://clients2.google.com/service/update2/crx" });
    onInstalled({ reason: "install" });
    expect(fakeChrome.tabs.create).toHaveBeenCalledWith({ url: "https://safely.sh/?installed=1" });
  });

  it("opens nothing on an update", () => {
    fakeChrome.tabs.create.mockClear();
    onInstalled({ reason: "update" });
    expect(fakeChrome.tabs.create).not.toHaveBeenCalled();
  });
});
