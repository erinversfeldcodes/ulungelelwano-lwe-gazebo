import { test, expect } from "@playwright/test";
import { withServerResponse } from "./helpers";

// Deliberately saves a large limit, not a small one: the daily new-card cap
// is global per-user, and other specs run concurrently against the same
// shared review queue. A small limit here would starve them of new cards.
// (The cap's actual enforcement math is covered in isolation by
// crates/web/src/db.rs's #[sqlx::test] suite — this just proves the
// settings form saves and reloads the value.)
test("daily new card limit setting persists across a reload", async ({ page }) => {
  await page.goto("/settings");
  const input = page.getByLabel("New cards per day (blank = unlimited)");

  await input.fill("9999");
  await withServerResponse(page, "set_daily_new_card_limit", () =>
    page.getByRole("button", { name: "Save" }).click(),
  );
  await expect(input).toHaveValue("9999");

  await page.reload();
  await expect(input).toHaveValue("9999");

  // Restore unlimited (what auth.setup.ts sets up front) rather than the
  // app's documented default of 20: other specs run concurrently against
  // the same shared per-user review queue, and 20 would starve them of new
  // cards for the remainder of the run.
  await input.fill("");
  await withServerResponse(page, "set_daily_new_card_limit", () =>
    page.getByRole("button", { name: "Save" }).click(),
  );
  await expect(input).toHaveValue("");
});
