import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests",
  timeout: 30_000,
  use: { baseURL: process.env.WB_URL ?? "http://127.0.0.1:18930" },
});
