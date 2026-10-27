import { test, expect } from "@playwright/test";
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
  await page.getByRole("button", { name: "Save" }).click();

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
  await page.getByRole("button", { name: "Delete card" }).click();

  await expect(page).toHaveURL("/");
  await expect(page.getByText(word, { exact: true })).not.toBeVisible();
});
