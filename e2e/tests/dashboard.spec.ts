import { test, expect, ConsoleMessage } from "@playwright/test";
import { readFileSync } from "fs";

// The workspace version, as the dashboard must show it (v<semver>).
const version = /\[workspace\.package\][^[]*?version\s*=\s*"([^"]+)"/s.exec(
  readFileSync(`${__dirname}/../../Cargo.toml`, "utf8"),
)![1];

test("dashboard shows the version and the live status", async ({ page }) => {
  const console: ConsoleMessage[] = [];
  page.on("console", (m) => {
    if (m.type() === "error" || m.type() === "warning") console.push(m);
  });
  page.on("pageerror", (e) => console.push({ type: () => "error", text: () => e.message } as ConsoleMessage));
  const bad: string[] = [];
  page.on("response", (r) => {
    if (r.status() >= 400) bad.push(`${r.status()} ${r.url()}`);
  });

  await page.goto("/");
  await expect(page.locator("#version")).toHaveText(`v${version}`);
  await expect(page.locator("#state")).toHaveText(process.env.WB_EXPECT_STATE ?? "Stena OK");
  await expect(page.locator("#rows")).toContainText("7680×1080 @ 59.978 Hz");
  await expect(page.locator("#rows")).toContainText("Arena - Screen 2 — pripojený");

  // The page polls /api/status every 2 s: the table must still be filled after a refresh.
  await page.waitForTimeout(2500);
  await expect(page.locator("#rows")).toContainText("Zmeškané vblanky (10 s)");

  // Headless Chromium does not fetch /favicon.ico; a headed browser does and logs a 404 when
  // the page declares no icon.
  await expect(page.locator('link[rel="icon"]')).toHaveCount(1);
  expect(bad).toEqual([]);
  expect(console.map((m) => `${m.type()}: ${m.text()}`)).toEqual([]);
});
