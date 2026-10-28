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

  await expect(page.getByText(dogWord, { exact: true })).toBeVisible();
  await expect(page.getByText(catWord, { exact: true })).toBeVisible();
  // Only the target word shows on the card list — not the English used to
  // search for an image.
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
