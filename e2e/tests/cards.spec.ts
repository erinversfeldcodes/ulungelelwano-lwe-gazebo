import { test, expect } from "./fixtures";
import { uniqueWord } from "./helpers";

test("creating a card shows it in the shared card list", async ({ page }) => {
  const word = uniqueWord("cards-spec");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByLabel("Tags (comma separated)").fill("e2e");
  await page.getByRole("button", { name: "Add" }).click();

  await expect(page).toHaveURL("/");
  await expect(page.getByText(word)).toBeVisible();
});

test("filtering by tag narrows the card list to matching cards", async ({ page }) => {
  const word = uniqueWord("cards-spec-tag");
  const tag = uniqueWord("e2e-tag");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByLabel("Tags (comma separated)").fill(tag);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.getByLabel("Filter by tag").selectOption(tag);
  await expect(page.getByText(word)).toBeVisible();
});

test("editing a card updates its word and is reflected in the list", async ({ page }) => {
  const word = uniqueWord("cards-spec-edit");
  const updated = `${word}-updated`;

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await page.getByLabel("Target-language word/phrase").fill(updated);
  // exact: true — the edit page's always-visible dictionary-details
  // section has its own "Save dictionary details" button, which "Save"
  // would otherwise substring-match too.
  await page.getByRole("button", { name: "Save", exact: true }).click();

  await expect(page).toHaveURL(/\/cards\/[0-9a-f-]+$/);
  await expect(page.getByLabel("Target-language word/phrase")).toHaveValue(updated);

  await page.goto("/");
  await expect(page.getByText(updated)).toBeVisible();
});

test("deleting a card removes it from the list", async ({ page }) => {
  const word = uniqueWord("cards-spec-delete");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  // Delete requires an arming click before the confirming one — see
  // app.rs's confirm_button.
  await page.getByRole("button", { name: "Delete card" }).click();
  await page.getByRole("button", { name: "Confirm delete?" }).click();

  await expect(page).toHaveURL("/");
  await expect(page.getByText(word, { exact: true })).not.toBeVisible();
});

test("an anonymous visitor sees a card read-only, with no edit form", async ({ page, browser }) => {
  const word = uniqueWord("cards-spec-anon");

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");
  const cardUrl = await page
    .locator("li", { hasText: word })
    .getByRole("link", { name: "Edit" })
    .getAttribute("href");
  if (!cardUrl) throw new Error(`couldn't find the card link for "${word}"`);

  // browser.newContext() doesn't inherit the project's `baseURL` the way
  // the `page` fixture does, so a relative href like cardUrl would fail to
  // resolve — build an absolute URL against the authenticated page's own
  // origin instead.
  const absoluteCardUrl = new URL(cardUrl, page.url()).toString();

  // The "chromium" project configures a default `storageState` (the signed-in
  // e2e user) for the `page`/`context` fixtures, and `browser.newContext()`
  // picks that ambient state up too unless told otherwise — so without this,
  // "anonPage" is silently signed in. Force a blank slate explicitly.
  const anonContext = await browser.newContext({ storageState: { cookies: [], origins: [] } });
  const anonPage = await anonContext.newPage();
  try {
    await anonPage.goto(absoluteCardUrl);
    await expect(anonPage.getByText(word, { exact: true })).toBeVisible();
    await expect(anonPage.getByLabel("Target-language word/phrase")).not.toBeVisible();
    await expect(anonPage.getByRole("button", { name: "Delete card" })).not.toBeVisible();
    // Scoped to the nav: the page also has a second "Sign in" link in its
    // own "Sign in to edit this card" prompt, so the bare role query matches
    // both and trips Playwright's strict mode.
    await expect(
      anonPage.getByRole("navigation").getByRole("link", { name: "Sign in" }),
    ).toBeVisible();
  } finally {
    await anonContext.close();
  }
});
