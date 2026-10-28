import { test as setup, expect } from "@playwright/test";
import { Client } from "pg";
import * as fs from "fs";

// Must be exactly the Resend account owner's own address: the sandbox
// sender (onboarding@resend.dev, used in preview/local) rejects any other
// recipient outright — including "@example.com", which Resend blocks by
// name — so request_magic_link's send step would 422 before the UI ever
// reaches the "check your email" state, regardless of the DB layer.
const TEST_EMAIL = "erinversfeld@gmail.com";

// Bypasses the slow part (actually receiving an email) the same way
// ~/thestacks' Playwright auth setup bypasses its own login UI: drive the
// real request up to the point email delivery would happen, then read the
// token straight out of Postgres instead of out of an inbox. Unlike
// thestacks, this needs no dedicated test-helper route in the app itself —
// reading the token directly is simpler and keeps test-only logic out of
// production code entirely.
setup("authenticate", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel("Email").fill(TEST_EMAIL);
  await page.getByRole("button", { name: "Send magic link" }).click();
  await expect(page.getByText("Check your email for a sign-in link.")).toBeVisible();

  const token = await fetchLatestToken(TEST_EMAIL);
  await page.goto(`/auth/verify?token=${token}`);
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

async function fetchLatestToken(email: string): Promise<string> {
  const client = new Client({ connectionString: process.env.DATABASE_URL });
  await client.connect();
  try {
    const result = await client.query(
      "SELECT token FROM login_tokens WHERE email = $1 ORDER BY created_at DESC LIMIT 1",
      [email],
    );
    if (result.rows.length === 0) {
      throw new Error(`no login token found for ${email}`);
    }
    return result.rows[0].token as string;
  } finally {
    await client.end();
  }
}

async function removeDailyNewCardLimit(email: string): Promise<void> {
  const client = new Client({ connectionString: process.env.DATABASE_URL });
  await client.connect();
  try {
    await client.query("UPDATE users SET daily_new_card_limit = NULL WHERE email = $1", [email]);
  } finally {
    await client.end();
  }
}
