// Home page reviews: loads approved reviews and shows them as a moving
// row. The section stays hidden when there are none, so the page never
// shows an empty or made-up review block.

type PublicReview = {
  display_name: string;
  role: string | null;
  rating: number;
  body: string;
  photo_url: string;
};

// With fewer reviews than this, the cards sit still in the middle.
const REVIEWS_MIN_TO_MOVE = 3;
// Seconds each card takes to pass, so speed stays the same for any count.
const REVIEWS_SECONDS_PER_CARD = 7;

(function () {
  function buildCard(review: PublicReview, hidden: boolean): HTMLElement {
    const card = document.createElement("article");
    card.className = "rv-card";
    if (hidden) card.setAttribute("aria-hidden", "true");

    const rating = Math.max(1, Math.min(5, Math.round(review.rating)));
    const stars = document.createElement("div");
    stars.className = "rv-stars";
    stars.setAttribute("aria-label", rating + " / 5");
    stars.textContent = "★".repeat(rating);
    if (rating < 5) {
      const off = document.createElement("span");
      off.className = "off";
      off.textContent = "★".repeat(5 - rating);
      stars.appendChild(off);
    }

    // textContent (never innerHTML): review text is shown exactly as typed.
    const quote = document.createElement("p");
    quote.className = "rv-quote";
    quote.textContent = "“" + review.body + "”";

    const who = document.createElement("div");
    who.className = "rv-who";

    const photo = document.createElement("img");
    photo.className = "rv-photo";
    photo.src = review.photo_url;
    photo.alt = hidden ? "" : review.display_name;
    photo.width = 48;
    photo.height = 48;

    const text = document.createElement("div");
    const name = document.createElement("div");
    name.className = "rv-name";
    name.textContent = review.display_name;
    text.appendChild(name);
    if (review.role) {
      const role = document.createElement("span");
      role.className = "rv-role";
      role.textContent = review.role;
      text.appendChild(role);
    }

    who.appendChild(photo);
    who.appendChild(text);
    card.appendChild(stars);
    card.appendChild(quote);
    card.appendChild(who);
    return card;
  }

  async function loadReviews(): Promise<void> {
    const section = document.getElementById("reviews");
    const track = document.getElementById("reviews-track");
    if (!section || !track) return;

    let reviews: PublicReview[] = [];
    try {
      const res = await fetch("/api/v1/reviews");
      if (!res.ok) return;
      reviews = await res.json();
    } catch (e) {
      return; // no reviews shown if the request fails
    }
    if (!Array.isArray(reviews) || reviews.length === 0) return;

    const moving = reviews.length >= REVIEWS_MIN_TO_MOVE;
    track.className = "rv-track " + (moving ? "moving" : "static");
    reviews.forEach((r) => track.appendChild(buildCard(r, false)));
    if (moving) {
      // Second copy for a seamless loop; hidden from screen readers.
      reviews.forEach((r) => track.appendChild(buildCard(r, true)));
      track.style.setProperty("--rv-duration", reviews.length * REVIEWS_SECONDS_PER_CARD + "s");
    }
    section.hidden = false;
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", loadReviews);
  } else {
    loadReviews();
  }
})();
