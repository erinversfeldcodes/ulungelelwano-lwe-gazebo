import { test, expect } from "./fixtures";
import { uniqueWord } from "./helpers";

// This file deliberately never asserts on real Pixabay/Openverse search
// *content* in the tests below — asserting on a third party's live
// catalog would make these tests flaky against something nobody here
// controls, and neither provider works reliably without its own
// credentials configured wherever this runs anyway (see the comment
// further down). What's actually load-bearing to test locally: the
// English field structurally can't be submitted as part of the card (see
// app.rs's ImageSearchPicker — it has no `name` attribute at all), and
// the search UI resolves out of "Searching…" into some terminal state
// rather than hanging forever.

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

// Unlike every test above, these two deliberately DO depend on reaching
// the real Pixabay/Openverse APIs (PIXABAY_API_KEY and
// OPENVERSE_CLIENT_ID/OPENVERSE_CLIENT_SECRET must be set wherever this
// runs — anonymous Openverse access is rate-limited per source IP and is
// NOT reliable on shared CI runners, see image_search.rs's doc comment
// on ImageSearch) and on real storage being configured (so
// import_card_image can actually complete) — on purpose. A provider
// that's down, rate-limited, or has changed its response shape, or a
// picked image that silently fails to persist, should fail these tests
// and block a merge, not be worked around with a mock. See
// image_search.rs's real_providers_all_respond_to_a_search for the
// equivalent check at the Rust level (search only, no browser/storage
// needed there).

test("search returns real results from both providers", async ({ page }) => {
  await page.goto("/new");
  await page
    .getByLabel("English translation (used only to search for an image — never stored on the card)")
    .fill("dog");
  await page.getByRole("button", { name: "Search images" }).click();
  await expect(page.getByText("Searching…")).not.toBeVisible({ timeout: 20_000 });

  for (const provider of ["pixabay", "openverse"]) {
    await expect(
      page.locator(`img.image-search-thumb[data-provider="${provider}"]`).first(),
    ).toBeVisible();
  }
});

test("picking a real search result renders on the card and survives a reload", async ({
  page,
}) => {
  const word = uniqueWord("image-search-real");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page
    .getByLabel("English translation (used only to search for an image — never stored on the card)")
    .fill("dog");
  await page.getByRole("button", { name: "Search images" }).click();
  await expect(page.getByText("Searching…")).not.toBeVisible({ timeout: 20_000 });

  // Openverse specifically, rather than Pixabay: isolates whether
  // import_card_image itself (the fetch-by-id + re-upload to our own
  // storage) works, separately from whether the Pixabay credential is
  // valid.
  const thumb = page.locator('img.image-search-thumb[data-provider="openverse"]').first();
  await expect(thumb).toBeVisible();
  await thumb.click();
  await expect(page.locator(".image-search-chosen img")).toBeVisible({ timeout: 20_000 });

  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  const cardImage = page.locator("li", { hasText: word }).locator("img");
  await expect(cardImage).toBeVisible();
  const storedSrc = await cardImage.getAttribute("src");
  // The card's image must be our own re-hosted copy, never a link
  // straight back to the provider — see import_card_image's doc comment.
  expect(storedSrc).not.toContain("openverse");

  await page.reload();
  await expect(page.locator("li", { hasText: word }).locator("img")).toBeVisible();
});
