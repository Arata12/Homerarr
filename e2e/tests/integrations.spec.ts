import { expect, test } from "./fixtures";

test("creates and deletes an integration", async ({ page }) => {
  await page.goto("/manage/integrations");
  const kind = page.locator("#integration-kind-select");
  const secrets = page.locator("#integration-secret-fields");

  await kind.selectOption("sonarr");
  await expect(secrets.locator('[data-secret-kind="apiKey"]')).toBeVisible();
  await kind.selectOption("dashDot");
  await expect(secrets.locator("[data-secret-kind]")).toHaveCount(0);
  await expect(secrets).toContainText("No additional credentials required");

  // Saving does not contact the integration, so an unreachable URL is fine.
  const name = `e2e-${Date.now()}`;
  const form = page.locator("#integration-create-form");
  await form.locator('input[name="name"]').fill(name);
  await form.locator('input[name="url"]').fill("http://127.0.0.1:9");
  await form.locator('button[type="submit"]').click();

  const row = page.locator("#integrations-table tbody tr", { hasText: name });
  await expect(row).toBeVisible();
  await expect(row).toContainText("dashDot");
  await row.locator('button[data-method="DELETE"]').click();
  await expect(row).toHaveCount(0);
});
