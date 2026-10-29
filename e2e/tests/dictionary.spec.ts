import { Page } from "@playwright/test";
import { test, expect } from "./fixtures";
import { createCardInOwnDeck, withServerResponse } from "./helpers";

// Saves dictionary details for the word a card was just created with, from
// that card's own edit page. Assumes `page` is already signed in and the
// card exists (e.g. via createCardInOwnDeck).
async function saveDictionaryDetails(
  page: Page,
  word: string,
  details: { partOfSpeech?: string; notes?: string; linkedWords?: string },
): Promise<void> {
  await page.goto("/");
  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await page.getByText("Dictionary details (optional)").click();
  if (details.partOfSpeech) {
    await page.getByLabel("Part of speech").fill(details.partOfSpeech);
  }
  if (details.notes) {
    await page.getByLabel("Notes").fill(details.notes);
  }
  if (details.linkedWords) {
    await page.getByLabel("Related words (comma separated)").fill(details.linkedWords);
  }
  await withServerResponse(page, "save_word_definition", () =>
    page.getByRole("button", { name: "Save dictionary details" }).click(),
  );
  await expect(page.getByText("Saved.")).toBeVisible();
}

// Reveals the one card in `deckName`'s review queue and clicks its word to
// open the dictionary popup — stops short of grading, since the tests here
// care about what the popup shows, not spaced-repetition scheduling.
async function revealAndOpenPopup(
  page: Page,
  deckName: string,
  word: string,
): Promise<void> {
  await page.goto("/review");
  await page.getByLabel("Deck").selectOption(deckName);
  await expect(page.getByText("Reveal")).toBeVisible();
  await page.getByText("Reveal").click();
  await page.locator(".clickable-word", { hasText: word }).click();
}

test("saved dictionary details appear in the popup when reviewing the card", async ({ page }) => {
  const { word, deckName } = await createCardInOwnDeck(page, "dict-spec-basic");
  await saveDictionaryDetails(page, word, { partOfSpeech: "noun", notes: "a test word" });

  await revealAndOpenPopup(page, deckName, word);
  const dialog = page.locator("dialog.word-dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("noun")).toBeVisible();
  await expect(dialog.getByText("a test word")).toBeVisible();
});

test("a word with no saved dictionary entry shows a graceful empty state", async ({ page }) => {
  const { word, deckName } = await createCardInOwnDeck(page, "dict-spec-empty");

  await revealAndOpenPopup(page, deckName, word);
  await expect(page.locator("dialog.word-dialog").getByText("No dictionary entry yet")).toBeVisible();
});

test("clicking a related word swaps the open dialog's content instead of opening a second one", async ({
  page,
}) => {
  const { word: wordA, deckName } = await createCardInOwnDeck(page, "dict-spec-a");
  const { word: wordB } = await createCardInOwnDeck(page, "dict-spec-b");

  await saveDictionaryDetails(page, wordB, { partOfSpeech: "a-distinguishing-part-of-speech" });
  await saveDictionaryDetails(page, wordA, { partOfSpeech: "noun", linkedWords: wordB });

  await revealAndOpenPopup(page, deckName, wordA);
  const dialog = page.locator("dialog.word-dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(wordB, { exact: true })).toBeVisible();

  await dialog.locator(".clickable-word", { hasText: wordB }).click();

  // Same dialog, new content — not a second one.
  await expect(page.locator("dialog.word-dialog")).toHaveCount(1);
  await expect(dialog.getByText("a-distinguishing-part-of-speech")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Back" })).toBeVisible();
});

test("opening and closing the popup never disturbs the review grade buttons", async ({ page }) => {
  const { word, deckName } = await createCardInOwnDeck(page, "dict-spec-grading");

  await revealAndOpenPopup(page, deckName, word);
  await expect(page.locator("dialog.word-dialog")).toBeVisible();
  await page.getByRole("button", { name: "Close" }).click();
  await expect(page.locator("dialog.word-dialog")).not.toBeVisible();

  await expect(page.getByRole("button", { name: "Good" })).toBeEnabled();
  await page.getByRole("button", { name: "Good" }).click();
  await page.waitForLoadState("networkidle");
});
