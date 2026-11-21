import { ADMIN, BOARD, expect, test } from "./fixtures";

// Creates the first admin (plus the starter board) through /init on a fresh instance,
// or signs in when a previous run already did.
test("onboard admin", async ({ page }) => {
  await page.goto("/init");
  const register = page.locator('form[data-api-form="/api/auth/register"]');
  if (await register.count()) {
    await register.locator('input[name="name"]').fill(ADMIN.name);
    await register.locator('input[name="password"]').fill(ADMIN.password);
    await register.locator('input[name="boardName"]').fill(BOARD);
    await expect(register.locator('input[name="createStarterBoard"]')).toBeChecked();
    await register.locator('button[type="submit"]').click();
  } else {
    await page.goto("/auth/login");
    await page.locator("#name").fill(ADMIN.name);
    await page.locator("#password").fill(ADMIN.password);
    await page.locator('#login-form button[type="submit"]').click();
  }
  await expect(page).toHaveURL(/\/$/);
  await expect(page.locator('[data-api-action="/api/auth/logout"]')).toBeAttached();
  await page.context().storageState({ path: ".auth/admin.json" });
});
