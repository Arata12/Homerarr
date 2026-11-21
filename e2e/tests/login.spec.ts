import { ADMIN, expect, test } from "./fixtures";

test.use({ storageState: { cookies: [], origins: [] } });

test("rejects a wrong password, then signs in", async ({ page, guard }) => {
  // The browser logs the expected 401 of the failed attempt as a console error.
  guard.allow(/status of 401/);
  await page.goto("/auth/login");
  await page.locator("#name").fill(ADMIN.name);
  await page.locator("#password").fill("definitely-wrong");
  await page.locator('#login-form button[type="submit"]').click();
  await expect(page.locator(".toast.error")).toContainText("Invalid username or password");
  await expect(page).toHaveURL(/\/auth\/login$/);

  await page.locator("#password").fill(ADMIN.password);
  await page.locator('#login-form button[type="submit"]').click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.locator('[data-api-action="/api/auth/logout"]')).toBeAttached();
});
