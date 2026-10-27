import { test, expect } from "@playwright/test";
import { uniqueWord } from "./helpers";

test("creating a deck and assigning a card to it scopes the card list filter", async ({ page }) => {
  const deckName = uniqueWord("decks-spec");
  const word = uniqueWord("decks-spec-card");

  await page.goto("/decks");
  await page.getByLabel("New deck name").fill(deckName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("link", { name: deckName })).toBeVisible();

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await page.getByLabel(deckName).check();
  await expect(page.getByLabel(deckName)).toBeChecked();
  // The checkbox's own DOM state updates synchronously on click, but the
  // AddCardToDeck server action it dispatches is async — wait for that
  // round trip before navigating away and relying on its effect.
  await page.waitForLoadState("networkidle");

  await page.goto("/");
  await page.getByLabel("Filter by deck").selectOption(deckName);
  await expect(page.getByText(word)).toBeVisible();
});

test("removing a card from a deck excludes it from that deck's filter again", async ({ page }) => {
  const deckName = uniqueWord("decks-spec-remove");
  const word = uniqueWord("decks-spec-remove-card");

  await page.goto("/decks");
  await page.getByLabel("New deck name").fill(deckName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("link", { name: deckName })).toBeVisible();

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await page.getByLabel(deckName).check();
  await expect(page.getByLabel(deckName)).toBeChecked();
  await page.waitForLoadState("networkidle");
  await page.getByLabel(deckName).uncheck();
  await expect(page.getByLabel(deckName)).not.toBeChecked();
  // Same as above: wait for RemoveCardFromDeck's round trip, not just the
  // checkbox's own immediate DOM state, before relying on its effect.
  await page.waitForLoadState("networkidle");

  await page.goto("/");
  await page.getByLabel("Filter by deck").selectOption(deckName);
  await expect(page.getByText(word)).not.toBeVisible();
});
