import { describe, it, expect, beforeEach } from "vitest";
import "../ts/scrapers/olx";

const scrapeOLX = () => (window as any).__safelyScrapers.scrapeOLX();

Object.defineProperty(HTMLElement.prototype, "innerText", {
  get() {
    return this.textContent;
  },
  set(value) {
    this.textContent = value;
  },
  configurable: true,
});

Object.defineProperty(HTMLElement.prototype, "innerText", {
  get() {
    return this.textContent;
  },
  set(value) {
    this.textContent = value;
  },
  configurable: true,
});

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("scrapeOLX - price parsing", () => {
  it("parses a plain, comma-separated price correctly", () => {
    document.body.innerHTML = `<span class="_24469da7">Rs 25,000</span>`;
    const data = scrapeOLX();
    expect(data.price).toBe(25000);
  });

  it("parses a 'crore' price and converts it to the real numeric value", () => {
    document.body.innerHTML = `<span class="_24469da7">Rs 1.5 crore</span>`;
    const data = scrapeOLX();
    expect(data.price).toBe(15000000);
  });

  it("parses a 'lac'/'lacs' price and converts it to the real numeric value", () => {
    document.body.innerHTML = `<span class="_24469da7">Rs 8 lacs</span>`;
    const data = scrapeOLX();
    expect(data.price).toBe(800000);
  });

  it("returns null when no price element exists on the page", () => {
    document.body.innerHTML = `<div>no price here</div>`;
    const data = scrapeOLX();
    expect(data.price).toBeNull();
  });
});

describe("scrapeOLX - listing_id", () => {
  it("extracts the listing ID from a real OLX URL", () => {
    Object.defineProperty(window, "location", {
      value: { href: "https://www.olx.com.pk/item/some-title-iid-1118276677" },
      writable: true,
    });
    const data = scrapeOLX();
    expect(data.listing_id).toBe("1118276677");
  });

  it("returns null when the URL has no iid- pattern", () => {
    Object.defineProperty(window, "location", {
      value: { href: "https://www.olx.com.pk/" },
      writable: true,
    });
    const data = scrapeOLX();
    expect(data.listing_id).toBeNull();
  });
});

describe("scrapeOLX - title and description", () => {
  it("extracts the title from the correct element", () => {
    document.body.innerHTML = `<h1 class="_75bce902">  Samsung Galaxy S23 Ultra  </h1>`;
    const data = scrapeOLX();
    expect(data.title).toBe("Samsung Galaxy S23 Ultra");
  });

  it("returns null title when the element is missing", () => {
    document.body.innerHTML = `<div>nothing here</div>`;
    const data = scrapeOLX();
    expect(data.title).toBeNull();
  });

  it("extracts the description from the correct nested element", () => {
    document.body.innerHTML = `<div class="_7a99ad24"><span>Excellent condition, no scratches.</span></div>`;
    const data = scrapeOLX();
    expect(data.description).toBe("Excellent condition, no scratches.");
  });
});

describe("scrapeOLX - image_urls", () => {
  it("takes only the first 3 images and filters to genuine olx URLs", () => {
    document.body.innerHTML = `
      <div class="image-gallery-slide"><img class="_66938426" src="https://img.olx.com.pk/1.jpg" /></div>
      <div class="image-gallery-slide"><img class="_66938426" src="https://img.olx.com.pk/2.jpg" /></div>
      <div class="image-gallery-slide"><img class="_66938426" src="https://other-cdn.com/3.jpg" /></div>
      <div class="image-gallery-slide"><img class="_66938426" src="https://img.olx.com.pk/4.jpg" /></div>
    `;
    const data = scrapeOLX();
    // Only the first 3 are ever considered, and among those, only
    // genuine olx URLs survive the filter - so image 3 (non-olx) is
    // dropped, and image 4 is never even looked at, since it's 4th.
    expect(data.image_urls).toEqual([
      "https://img.olx.com.pk/1.jpg",
      "https://img.olx.com.pk/2.jpg",
    ]);
  });

  it("returns null when no images are found at all", () => {
    document.body.innerHTML = `<div>no images</div>`;
    const data = scrapeOLX();
    expect(data.image_urls).toBeNull();
  });
});

describe("scrapeOLX - seller_name", () => {
  it("finds the seller name via the 'Posted by' label", () => {
    document.body.innerHTML = `
      <span class="_9083bec6">Posted by</span>
      <div><span>Ali Iqbal</span></div>
    `;
    const data = scrapeOLX();
    expect(data.seller_name).toBe("Ali Iqbal");
  });

  it("returns null when the 'Posted by' label is missing entirely", () => {
    document.body.innerHTML = `<div>nothing relevant</div>`;
    const data = scrapeOLX();
    expect(data.seller_name).toBeNull();
  });
});

describe("scrapeOLX - seller_join_date", () => {
  it("finds the join date via the structured 'Member Since' element", () => {
    document.body.innerHTML = `
      <span class="_9083bec6 _1fcb6673">Member Since</span>
      <span class="_8206696c b7af14b4">2020</span>
    `;
    const data = scrapeOLX();
    expect(data.seller_join_date).toBe("Member since 2020");
  });

  it("falls back to scanning the page's raw text when the structured element is missing", () => {
    document.body.innerHTML = `<div>Some text here. Member Since 2019. More text.</div>`;
    const data = scrapeOLX();
    expect(data.seller_join_date).toBe("Member since 2019");
  });

  it("returns null when neither the structured element nor the text pattern is found", () => {
    document.body.innerHTML = `<div>nothing about membership here</div>`;
    const data = scrapeOLX();
    expect(data.seller_join_date).toBeNull();
  });
});

describe("scrapeOLX - seller_profile_url and platform_id", () => {
  it("builds the full profile URL and extracts the platform ID", () => {
    document.body.innerHTML = `<a class="da952dfc" href="/profile/ali-iqbal-abc123"></a>`;
    const data = scrapeOLX();
    expect(data.seller_profile_url).toBe("https://www.olx.com.pk/profile/ali-iqbal-abc123");
    expect(data.platform_id).toBe("ali-iqbal-abc123");
  });

  it("returns null for both when the profile link is missing", () => {
    document.body.innerHTML = `<div>no profile link</div>`;
    const data = scrapeOLX();
    expect(data.seller_profile_url).toBeNull();
    expect(data.platform_id).toBeNull();
  });
});

describe("scrapeOLX - seller_location and seller_last_active", () => {
  it("finds the location via the SVG pin icon's parent element", () => {
    document.body.innerHTML = `<div>Lahore, Punjab<svg class="d0356874"></svg></div>`;
    const data = scrapeOLX();
    expect(data.seller_location).toBe("Lahore, Punjab");
  });

  it("extracts the last-active text from its specific aria-label", () => {
    document.body.innerHTML = `<span aria-label="Creation date">2 hours ago</span>`;
    const data = scrapeOLX();
    expect(data.seller_last_active).toBe("2 hours ago");
  });

  it("returns null for both when neither element is present", () => {
    document.body.innerHTML = `<div>nothing here</div>`;
    const data = scrapeOLX();
    expect(data.seller_location).toBeNull();
    expect(data.seller_last_active).toBeNull();
  });
});
