import { Page, expect } from "@playwright/test";

// Every environment that actually sends mail (preview, and prod) uses
// Resend's sandbox sender (onboarding@resend.dev) rather than a verified
// custom domain. That sandbox rejects any recipient other than the
// account owner's own exact address — no aliases, no other domains, not
// even "@example.com" — so any e2e test that triggers a real magic-link
// send (not just the DB-bypassing token lookup in auth.setup.ts) must
// target this literal address rather than a unique per-run one.
export const RESEND_ALLOWED_EMAIL = "erinversfeld@gmail.com";

export function uniqueWord(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}

// Leptos hydrates client-side only once hydrate_body (lib.rs) actually
// returns — well after the WASM bundle's own network fetch resolves, since
// the browser still has to compile and instantiate it first. Interacting
// with a server-action form before hydration completes falls through to a
// plain native HTML submission instead of a fetch; observed directly in a
// CI trace as a 302 response followed by a full page reload, which
// discards all client-side state, including whatever the action's result
// would have rendered. (An earlier version of this helper waited on the
// WASM fetch itself — confirmed by trace to still race: the form submit
// landed under 200ms after that fetch resolved, well short of hydration
// actually finishing.) `data-hydrated` is set by hydrate() for exactly
// this reason. Only matters on a cold cache (nothing fetched yet in this
// browser context), which is exactly what auth.setup.ts's "setup" project
// always is — the one place this has actually been seen to race.
export async function gotoAndWaitForHydration(page: Page, path: string): Promise<void> {
  await page.goto(path);
  await page.locator('body[data-hydrated="true"]').waitFor();
}

// A Leptos server action's fetch is dispatched asynchronously from its
// triggering DOM event (a checkbox's on:change, a form's submit), so the
// checkbox/input's own post-click state is just the browser's optimistic
// local echo — not confirmation the server received or committed anything.
// `waitForLoadState("networkidle")` is not a reliable stand-in for that
// confirmation either: if the fetch hasn't actually been dispatched yet
// when the idle check runs, it reports idle immediately. A real page
// navigation right after (page.goto, page.reload) then cancels that
// still-in-flight request outright — observed directly in CI traces as
// the request landing with status -1. Registering the expected response
// before triggering the action, and awaiting both together, closes that
// window for good instead of hoping a heuristic wait covers it.
export async function withServerResponse<T>(
  page: Page,
  fnName: string,
  trigger: () => Promise<T>,
): Promise<T> {
  const [, result] = await Promise.all([
    page.waitForResponse((res) => res.url().includes(`/api/${fnName}`) && res.ok()),
    trigger(),
  ]);
  return result;
}

// Creates a card and a deck containing only that card. Scoping a spec's
// review to its own deck (rather than the shared pool's review queue) is
// what makes it safe under parallel execution: specs running concurrently
// no longer compete over which card the queue offers next, or risk one
// spec grading away another's target card while searching for its own.
export async function createCardInOwnDeck(
  page: Page,
  prefix: string,
): Promise<{ word: string; deckName: string }> {
  const word = uniqueWord(prefix);
  const deckName = uniqueWord(`${prefix}-deck`);

  await page.goto("/decks");
  await page.getByLabel("New deck name").fill(deckName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByRole("link", { name: deckName })).toBeVisible();

  await page.goto("/new");
  await page.getByLabel("Target-language word/phrase").fill(word);
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page).toHaveURL("/");

  await page.locator("li", { hasText: word }).getByRole("link", { name: "Edit" }).click();
  await withServerResponse(page, "add_card_to_deck", () => page.getByLabel(deckName).check());
  await expect(page.getByLabel(deckName)).toBeChecked();

  return { word, deckName };
}

// Reveals and grades "Good" the one card in a deck created by
// createCardInOwnDeck, asserting it's actually that card.
export async function reviewOwnCard(page: Page, deckName: string, word: string): Promise<void> {
  await page.goto("/review");
  await page.getByLabel("Deck").selectOption(deckName);
  await expect(page.getByText("Reveal")).toBeVisible();
  await page.getByText("Reveal").click();
  await expect(page.getByText(word, { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Good" }).click();
  await page.waitForLoadState("networkidle");
}
