import { test as setup, expect } from "./fixtures";
import { Client } from "pg";
import * as fs from "fs";
import { RESEND_ALLOWED_EMAIL, TEST_PASSWORD } from "./helpers";

const TEST_EMAIL = RESEND_ALLOWED_EMAIL;

// Signs up (or, if this suite already ran against this database, logs in
// — password auth rejects a repeat signup for an email that already
// exists) as the one shared test account every other spec file's
// storageState depends on. Replaces the old magic-link version of this
// setup, which bypassed the slow part (actually receiving an email) by
// reading the login token straight out of Postgres instead of out of an
// inbox; password auth needs no such bypass, since signing in never
// involves an email round trip at all.
setup("authenticate", async ({ page }) => {
  await page.goto("/signup");
  await page.getByLabel("Name").fill("Test User");
  await page.getByLabel("Email").fill(TEST_EMAIL);
  await page.getByLabel("Password", { exact: true }).fill(TEST_PASSWORD);
  await page.getByLabel("Confirm password").fill(TEST_PASSWORD);
  await page.getByRole("button", { name: "Sign up" }).click();

  // Fresh database (every CI run): signup succeeds and the page navigates
  // to "/". A database this suite already ran against locally: signup is
  // rejected (the account already exists) and the page stays on /signup
  // showing the error — fall back to logging in with the same
  // credentials instead.
  await page.waitForURL("/", { timeout: 5_000 }).catch(() => {});
  if (page.url().includes("/signup")) {
    await expect(page.getByText(/already exists/i)).toBeVisible();
    await page.goto("/login");
    await page.getByLabel("Email").fill(TEST_EMAIL);
    await page.getByLabel("Password", { exact: true }).fill(TEST_PASSWORD);
    await page.getByRole("button", { name: "Sign in" }).click();
  }

  await expect(page).toHaveURL("/");

  // The daily new-card limit is real per-user product behavior (paces how
  // many unseen cards a learner is introduced to), not something the e2e
  // suite is testing — that's covered precisely, in isolation, by
  // crates/web/src/db.rs's #[sqlx::test] suite. Uncapping it here is just
  // a safety margin on top of helpers.ts's deck-scoped review pattern, so
  // no spec can ever be starved of a "new" card by the shared pool's
  // accumulated history.
  await removeDailyNewCardLimit(TEST_EMAIL);

  fs.mkdirSync(".auth", { recursive: true });
  await page.context().storageState({ path: ".auth/user.json" });
});

async function removeDailyNewCardLimit(email: string): Promise<void> {
  const client = new Client({ connectionString: process.env.DATABASE_URL });
  await client.connect();
  try {
    await client.query("UPDATE users SET daily_new_card_limit = NULL WHERE email = $1", [email]);
  } finally {
    await client.end();
  }
}
