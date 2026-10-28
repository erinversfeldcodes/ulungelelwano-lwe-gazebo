import { test, expect } from "./fixtures";
import { createCardInOwnDeck, reviewOwnCard } from "./helpers";

// total_cards and reviews_today are shared-pool aggregates, so other specs
// running concurrently can move them — this asserts the delta this test
// itself caused, not an absolute value.
test("grading a card increments today's logged review count", async ({ page }) => {
  const { word, deckName } = await createCardInOwnDeck(page, "stats-spec");

  await page.goto("/stats");
  const before = await reviewsLoggedToday(page);

  await reviewOwnCard(page, deckName, word);

  await page.goto("/stats");
  const after = await reviewsLoggedToday(page);
  expect(after).toBeGreaterThan(before);
});

async function reviewsLoggedToday(page: import("@playwright/test").Page): Promise<number> {
  const text = await page.locator("li", { hasText: "reviews logged today" }).textContent();
  const match = text?.match(/^(\d+)/);
  if (!match) throw new Error(`couldn't parse reviews-logged-today from "${text}"`);
  return Number(match[1]);
}
