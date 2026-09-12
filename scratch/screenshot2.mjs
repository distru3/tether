import { chromium } from "playwright";

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  page.on("console", msg => console.log("PAGE LOG:", msg.text()));
  page.on("pageerror", err => console.log("PAGE ERROR:", err.message));

  // Mock Tauri invoke
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      invoke: (cmd, args) => {
        console.log("Mock invoke called:", cmd, args);
        if (cmd === "status") return Promise.resolve({ state: "Active", config: {} });
        if (cmd === "day_summary") return Promise.resolve({ total_seconds: 3600, limit_for: {}, apps: [], categories: [] });
        return Promise.resolve({});
      }
    };
    window.__TAURI__ = window.__TAURI_INTERNALS__;
    window.__TAURI_IPC__ = window.__TAURI_INTERNALS__.invoke;
  });

  await page.goto("http://localhost:1420");
  await page.waitForTimeout(3000);
  
  await browser.close();
})();
