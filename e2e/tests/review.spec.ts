import { test, expect } from "@playwright/test";
import { uniqueWord, createCardInOwnDeck, reviewOwnCard } from "./helpers";

test("grading a due card removes it from today's review queue", async ({ page }) => {
  const { word, deckName } = await createCardInOwnDeck(page, "review-spec");

  await reviewOwnCard(page, deckName, word);
  // reviewOwnCard asserts the card was actually offered and graded; that the
  // deck-scoped queue then shows "Nothing due" (rather than re-offering the
  // same card) is the UI-visible proof grading took effect. The exact
  // scheduling math — that "Good" moves due_at a day out, so this card
  // specifically won't reappear today — is asserted precisely and
  // non-flakily by crates/web/src/db.rs's
  // grading_removes_card_from_due_review_until_its_next_interval test
  // against an isolated database, rather than re-proven here.
  await page.goto("/review");
  await page.getByLabel("Deck").selectOption(deckName);
  await expect(page.getByText("Nothing due. Nice.")).toBeVisible();
});

// The exact ordering math (hardest-first sorts by lapses/difficulty, due-date sorts
// by due_at) is covered by crates/web/src/db.rs's #[sqlx::test] suite against
// isolated databases, where it can be asserted precisely. Here we only need
// to prove the dropdown is wired up: switching it re-queries without erroring
// and still shows a due/new count line, whatever cards are in the shared
// pool from other specs running concurrently.
test("switching review order re-queries the queue without erroring", async ({ page }) => {
  await page.goto("/review");
  for (const option of ["random", "hardest", "due"]) {
    await page.getByLabel("Order").selectOption(option);
    await expect(page.getByText("new available today")).toBeVisible();
  }
});

test("switching the deck filter re-queries the queue without erroring", async ({ page }) => {
  const deckName = uniqueWord("review-spec-deck");

  await page.goto("/decks");
  await page.getByLabel("New deck name").fill(deckName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("link", { name: deckName })).toBeVisible();

  await page.goto("/review");
  await page.getByLabel("Deck").selectOption(deckName);
  await expect(page.getByText("new available today")).toBeVisible();

  await page.getByLabel("Deck").selectOption("All decks");
  await expect(page.getByText("new available today")).toBeVisible();
});
