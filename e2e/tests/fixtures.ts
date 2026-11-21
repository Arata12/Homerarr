import { test as base, expect } from "@playwright/test";

export const ADMIN = { name: "e2e-admin", password: "e2e-password-123" };
export const BOARD = "dashboard";

type Guard = { allow: (pattern: RegExp) => void };

// Fails any test whose pages log console errors, throw uncaught errors or trip the CSP.
export const test = base.extend<{ guard: Guard }>({
  guard: [
    async ({ context }, use) => {
      const errors: string[] = [];
      const allowed: RegExp[] = [];
      await context.addInitScript(() => {
        document.addEventListener("securitypolicyviolation", (e) =>
          console.error(`CSP violation: ${e.violatedDirective} blocked ${e.blockedURI}`),
        );
      });
      const watch = (page: import("@playwright/test").Page) => {
        page.on("console", (msg) => {
          if (msg.type() === "error") errors.push(`${page.url()}: ${msg.text()}`);
        });
        page.on("pageerror", (err) => errors.push(`${page.url()}: uncaught ${err.message}`));
      };
      context.pages().forEach(watch);
      context.on("page", watch);
      await use({ allow: (p) => allowed.push(p) });
      expect(errors.filter((e) => !allowed.some((p) => p.test(e)))).toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };
