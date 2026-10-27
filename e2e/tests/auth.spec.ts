import { test, expect } from "@playwright/test";

// Runs signed out, deliberately ignoring the project's saved auth state.
test.use({ storageState: { cookies: [], origins: [] } });

test("visiting the app while signed out redirects to /login", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL("/login");
});

test("requesting a magic link shows a confirmation, not an error", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel("Email").fill(`auth-spec-${Date.now()}@example.com`);
  await page.getByRole("button", { name: "Send magic link" }).click();
  await expect(page.getByText("Check your email for a sign-in link.")).toBeVisible();
});
