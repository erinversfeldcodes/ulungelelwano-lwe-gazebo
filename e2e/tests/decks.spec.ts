import { test, expect } from "./fixtures";
import { uniqueWord, withServerResponse } from "./helpers";

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
  await withServerResponse(page, "add_card_to_deck", () => page.getByLabel(deckName).check());
  await expect(page.getByLabel(deckName)).toBeChecked();

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
  await withServerResponse(page, "add_card_to_deck", () => page.getByLabel(deckName).check());
  await expect(page.getByLabel(deckName)).toBeChecked();
  await withServerResponse(page, "remove_card_from_deck", () => page.getByLabel(deckName).uncheck());
  await expect(page.getByLabel(deckName)).not.toBeChecked();

  await page.goto("/");
  await page.getByLabel("Filter by deck").selectOption(deckName);
  await expect(page.getByText(word)).not.toBeVisible();
});

test("a newly created card automatically lands in the Default deck", async ({ page }) => {
  const word = uniqueWord("decks-spec-default");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.getByLabel("Filter by deck").selectOption("Default");
  await expect(page.getByText(word)).toBeVisible();
});

test("renaming a deck updates its name", async ({ page }) => {
  const deckName = uniqueWord("decks-spec-rename");
  const newName = uniqueWord("decks-spec-renamed");

  await page.goto("/decks");
  await page.getByLabel("New deck name").fill(deckName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("link", { name: deckName })).toBeVisible();

  const row = page.locator("li", { hasText: deckName });
  await row.locator('input[name="name"]').fill(newName);
  await row.getByRole("button", { name: "Rename" }).click();

  await expect(page.getByRole("link", { name: newName })).toBeVisible();
  await expect(page.getByRole("link", { name: deckName, exact: true })).not.toBeVisible();
});

test("deleting a deck removes it from the list", async ({ page }) => {
  const deckName = uniqueWord("decks-spec-delete");

  await page.goto("/decks");
  await page.getByLabel("New deck name").fill(deckName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("link", { name: deckName })).toBeVisible();

  await page.locator("li", { hasText: deckName }).getByRole("button", { name: "Delete" }).click();

  await expect(page.getByRole("link", { name: deckName })).not.toBeVisible();
});
