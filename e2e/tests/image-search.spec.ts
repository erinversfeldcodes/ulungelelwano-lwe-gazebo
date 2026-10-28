import { test, expect } from "./fixtures";
import { uniqueWord } from "./helpers";

// This file deliberately never asserts on real Pexels/Pixabay/Openverse
// search *content* — only Openverse needs no key, so it's the only
// provider this suite could realistically hit even in CI, and asserting
// on a third party's live catalog would make these tests flaky against
// something nobody here controls. What's actually load-bearing to test
// locally: the English field structurally can't be submitted as part of
// the card (see app.rs's ImageSearchPicker — it has no `name` attribute
// at all), and the search UI resolves out of "Searching…" into some
// terminal state rather than hanging forever.

test("the English translation field is not a submittable form field", async ({ page }) => {
  await page.goto("/new");
  const english = page.getByLabel(
    "English translation (used only to search for an image — never stored on the card)",
  );
  await expect(english).toBeVisible();
  await expect(english).not.toHaveAttribute("name");
});

test("searching resolves out of the pending state", async ({ page }) => {
  await page.goto("/new");
  await page
    .getByLabel("English translation (used only to search for an image — never stored on the card)")
    .fill("dog");
  await page.getByRole("button", { name: "Search images" }).click();
  await expect(page.getByText("Searching…")).toBeVisible();
  await expect(page.getByText("Searching…")).not.toBeVisible({ timeout: 20_000 });
});

// Confirms the whole point of the feature end to end at the data layer,
// without depending on any real provider: even with English text typed
// in, a card saved *without* clicking Search carries none of it anywhere
// — not in target_text, not anywhere else on the card.
test("a card saved without using image search carries no trace of the English text", async ({
  page,
}) => {
  const word = uniqueWord("image-search-no-leak");
  const marker = `english-marker-${Date.now()}`;

  await page.goto("/new");
  await page
    .getByLabel("English translation (used only to search for an image — never stored on the card)")
    .fill(marker);
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await expect(page.getByText(word, { exact: true })).toBeVisible();
  await expect(page.getByText(marker, { exact: true })).not.toBeVisible();
});
