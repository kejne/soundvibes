import { expect, test } from "@playwright/test";

test("renders hero and quickstart", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByRole("heading", { name: /capture voice notes/i })).toBeVisible();
  await expect(page.getByRole("link", { name: /download from github releases/i })).toBeVisible();

  const quickstart = page.locator("[data-smoke='quickstart']");
  await expect(quickstart).toBeVisible();
  await expect(quickstart.getByRole("heading", { name: /two commands, done/i })).toBeVisible();
  await expect(quickstart.getByText("sv daemon start")).toBeVisible();
});
