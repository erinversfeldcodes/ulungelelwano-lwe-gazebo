import { test as base, expect } from "@playwright/test";

// page.goto is a full browser navigation, not a client-side router
// transition — every single call re-runs hydrate() (lib.rs) from scratch,
// not just the first one in a test. Any test that interacts with a
// server-action form right after a goto races hydration completing unless
// something waits for it first: observed directly in CI traces as a form
// submit falling through to a plain (unhydrated) native POST, well under
// a second after the goto resolved. Patching goto itself here, rather than
// requiring every call site to remember a wrapper, is the only way to
// actually close this for every test rather than just the ones that have
// already been caught racing. `data-hydrated` is set by hydrate() for
// exactly this reason.
export const test = base.extend({
  page: async ({ page }, use) => {
    const originalGoto = page.goto.bind(page);
    page.goto = (async (url, options) => {
      const response = await originalGoto(url, options);
      await page.locator('body[data-hydrated="true"]').waitFor();
      return response;
    }) as typeof page.goto;
    await use(page);
  },
});

export { expect };
