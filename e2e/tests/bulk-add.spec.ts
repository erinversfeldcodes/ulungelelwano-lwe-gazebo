import { test, expect } from "./fixtures";
import { uniqueWord } from "./helpers";

test("pasting a list creates every well-formed draft as a card, skipping the rest", async ({
  page,
}) => {
  const dogWord = uniqueWord("bulk-perro");
  const catWord = uniqueWord("bulk-gato");

  await page.goto("/bulk-add");
  await page
    .getByLabel("Paste one card per line: english,target word")
    .fill(`dog,${dogWord}\ncat,${catWord}\nthis line is malformed`);
  await page.getByRole("button", { name: "Parse" }).click();

  await expect(
    page.getByText('Skipped 1 line(s) that weren\'t "english,word": this line is malformed'),
  ).toBeVisible();
  await expect(page.getByText(`${dogWord} (dog)`)).toBeVisible();
  await expect(page.getByText(`${catWord} (cat)`)).toBeVisible();

  await page.getByRole("button", { name: "Create all cards" }).click();
  await expect(page).toHaveURL("/");

  // The word is behind each card's "Reveal" control now (see app.rs's
  // CardListPage) — hasText checks textContent, not visibility.
  await expect(page.locator("li", { hasText: dogWord })).toHaveCount(1);
  await expect(page.locator("li", { hasText: catWord })).toHaveCount(1);
  // Only the target word is ever stored on the card — not the English
  // used to search for an image — so it's not on the card list at all,
  // not even hidden behind Reveal.
  await expect(page.getByText("dog", { exact: true })).not.toBeVisible();
  await expect(page.getByText("cat", { exact: true })).not.toBeVisible();
});

test("each draft pre-fills its own image search with the parsed English word", async ({
  page,
}) => {
  const word = uniqueWord("bulk-prefill");

  await page.goto("/bulk-add");
  await page
    .getByLabel("Paste one card per line: english,target word")
    .fill(`elephant,${word}`);
  await page.getByRole("button", { name: "Parse" }).click();

  await expect(
    page.getByLabel("English translation (used only to search for an image — never stored on the card)"),
  ).toHaveValue("elephant");
});

test("parsing an empty or all-malformed list shows no drafts and no submit button", async ({
  page,
}) => {
  await page.goto("/bulk-add");
  await page
    .getByLabel("Paste one card per line: english,target word")
    .fill("nope\nalso nope");
  await page.getByRole("button", { name: "Parse" }).click();

  await expect(page.getByText(/Skipped 2 line\(s\)/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Create all cards" })).not.toBeVisible();
});
