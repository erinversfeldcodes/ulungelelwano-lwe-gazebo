import { test, expect } from "@playwright/test";

// Runs signed out, deliberately ignoring the project's saved auth state.
test.use({ storageState: { cookies: [], origins: [] } });

// Browsing the shared card pool is public; this also doubles as proof the
// page doesn't error out with no session cookie at all.
test("visiting the card list while signed out shows it, not a login redirect", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page).toHaveURL("/");
  await expect(page.getByRole("heading", { name: "Cards" })).toBeVisible();
});

test("visiting decks while signed out shows them, not a login redirect", async ({ page }) => {
  await page.goto("/decks");
  await expect(page).toHaveURL("/decks");
  await expect(page.getByRole("heading", { name: "Decks" })).toBeVisible();
  // Mutating decks is still auth-only — no create form for an anonymous visitor.
  await expect(page.getByLabel("New deck name")).not.toBeVisible();
});

for (const path of ["/new", "/review", "/stats", "/settings"]) {
  test(`visiting ${path} while signed out redirects to /login`, async ({ page }) => {
    await page.goto(path);
    await expect(page).toHaveURL("/login");
  });
}

test("requesting a magic link shows a confirmation, not an error", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel("Email").fill(`auth-spec-${Date.now()}@example.com`);
  await page.getByRole("button", { name: "Send magic link" }).click();
  await expect(page.getByText("Check your email for a sign-in link.")).toBeVisible();
});
