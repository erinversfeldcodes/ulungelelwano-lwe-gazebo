import { Page, expect } from "@playwright/test";

export function uniqueWord(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}

// Creates a card and a deck containing only that card. Scoping a spec's
// review to its own deck (rather than the shared pool's review queue) is
// what makes it safe under parallel execution: specs running concurrently
// no longer compete over which card the queue offers next, or risk one
// spec grading away another's target card while searching for its own.
export async function createCardInOwnDeck(
  page: Page,
  prefix: string,
): Promise<{ word: string; deckName: string }> {
  const word = uniqueWord(prefix);
  const deckName = uniqueWord(`${prefix}-deck`);

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
  // round trip before callers rely on the card actually being in the deck.
  await page.waitForLoadState("networkidle");

  return { word, deckName };
}

// Reveals and grades "Good" the one card in a deck created by
// createCardInOwnDeck, asserting it's actually that card.
export async function reviewOwnCard(page: Page, deckName: string, word: string): Promise<void> {
  await page.goto("/review");
  await page.getByLabel("Deck").selectOption(deckName);
  await expect(page.getByText("Reveal")).toBeVisible();
  await page.getByText("Reveal").click();
  await expect(page.getByText(word, { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Good" }).click();
  await page.waitForLoadState("networkidle");
}
