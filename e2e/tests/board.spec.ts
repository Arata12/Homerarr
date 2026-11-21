import { BOARD, expect, test } from "./fixtures";

const boardUrl = `/boards/${BOARD}`;

test("renders the starter board", async ({ page }) => {
  await page.goto(boardUrl);
  await expect(page.locator('.grid-item[data-kind="app"]')).toHaveCount(4);
  await expect(page.locator('.grid-item[data-kind="clock"]').first()).toBeVisible();
  await expect(page.locator('.grid-item[data-kind="bookmarks"]')).toContainText("Useful Links");
});

test("adds a widget in edit mode and it persists", async ({ page }) => {
  await page.goto(boardUrl);
  const clocks = page.locator('.grid-item[data-kind="clock"]');
  const before = await clocks.count();

  await page.locator("#edit-mode-btn").click();
  await expect(page.locator("body")).toHaveClass(/edit-mode/);
  await page.locator('[data-editor-action="add-widget"]').click();
  await page
    .locator(".widget-picker-card")
    .filter({ has: page.locator(".widget-picker-kind", { hasText: /^clock$/ }) })
    .click();
  await page.locator('.editor-modal button[type="submit"]').click();

  // Saving reloads the page, still in edit mode.
  await expect(clocks).toHaveCount(before + 1);
  await page.locator('[data-editor-action="exit"]').click();
  await expect(page.locator("body")).not.toHaveClass(/edit-mode/);
  await page.reload();
  await expect(clocks).toHaveCount(before + 1);
});
