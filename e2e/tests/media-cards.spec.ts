import { test, expect } from "./fixtures";
import { recordClip, uniqueWord, withServerResponse } from "./helpers";

test("recording audio attaches a playable clip to a new card", async ({ page }) => {
  const word = uniqueWord("media-spec-audio");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await recordClip(page, "audio");

  // The recorded clip actually landed on the file input itself (not just
  // the preview player) — this is what the form submission below actually
  // uploads, via the same multipart path a manually-chosen file would use.
  const audioInput = page.locator('input[name="audio"]');
  await expect
    .poll(() => audioInput.evaluate((el: HTMLInputElement) => el.files?.length ?? 0))
    .toBe(1);

  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await expect(page.locator("audio")).toBeVisible();
});

test("recording video attaches a playable clip to a new card", async ({ page }) => {
  const word = uniqueWord("media-spec-video");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await recordClip(page, "video");

  const videoInput = page.locator('input[name="video"]');
  await expect
    .poll(() => videoInput.evaluate((el: HTMLInputElement) => el.files?.length ?? 0))
    .toBe(1);

  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await expect(page.locator("video")).toBeVisible();
});

test("removing an attached recording clears it from the card", async ({ page }) => {
  const word = uniqueWord("media-spec-remove");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await recordClip(page, "audio");
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await expect(page.locator("audio")).toBeVisible();

  await page.getByLabel("Remove audio").check();
  await page.getByRole("button", { name: "Save" }).click();

  await expect(page).toHaveURL(/\/cards\/[0-9a-f-]+$/);
  await expect(page.locator("audio")).not.toBeVisible();
  // The removal checkbox itself only appears when there's something to
  // remove (see app.rs) — gone now, same as the player.
  await expect(page.getByLabel("Remove audio")).not.toBeVisible();
});

test("a two-way card is reviewed in both directions, independently", async ({ page }) => {
  const word = uniqueWord("media-spec-two-way");
  const deckName = uniqueWord("media-spec-two-way-deck");

  // Scoped to its own deck, like helpers.ts's createCardInOwnDeck — the
  // shared review queue is contended by every spec running concurrently,
  // and this test cares about exact queue counts (2 new, then 1, then 0),
  // which only holds if nothing else's cards can land in it.
  await page.goto("/decks");
  await page.getByLabel("New deck name").fill(deckName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("link", { name: deckName })).toBeVisible();

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByLabel(/Two-way/).check();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await withServerResponse(page, "add_card_to_deck", () => page.getByLabel(deckName).check());
  await expect(page.getByLabel(deckName)).toBeChecked();

  await page.goto("/review");
  await page.getByLabel("Deck").selectOption(deckName);
  await expect(page.getByText("2 new available today")).toBeVisible();

  // Forward direction: the word is hidden behind Reveal.
  await expect(page.getByText(word, { exact: true })).not.toBeVisible();
  await page.getByText("Reveal").click();
  await expect(page.getByText(word, { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Good" }).click();
  await page.waitForLoadState("networkidle");

  // Reverse direction: the same card, but the word is the prompt (not
  // hidden), shown immediately rather than behind Reveal — scheduled and
  // graded completely independently of the forward pass above.
  await expect(page.getByText("1 new available today")).toBeVisible();
  await expect(page.getByText(word, { exact: true })).toBeVisible();
  await page.getByText("Reveal").click();
  await page.getByRole("button", { name: "Good" }).click();
  await page.waitForLoadState("networkidle");

  await expect(page.getByText("Nothing due. Nice.")).toBeVisible();
});
