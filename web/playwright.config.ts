import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  workers: 1,
  use: {
    baseURL: process.env.ORACLE_TEST_ORIGIN,
    browserName: "chromium",
    httpCredentials: {
      username: "operator",
      password: process.env.ORACLE_TEST_PASSWORD ?? "",
    },
  },
});
