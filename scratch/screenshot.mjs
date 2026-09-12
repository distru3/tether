import { chromium } from "playwright";
import fs from "fs";

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  // Mock Tauri invoke
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      invoke: (cmd, args) => {
        console.log("Mock invoke called:", cmd, args);
        if (cmd === "status") return Promise.resolve({ state: "Active", config: {} });
        if (cmd === "day_summary") return Promise.resolve({ total_seconds: 3600, limit_for: {}, apps: [], categories: [] });
        if (cmd === "list_schedules") return Promise.resolve({ schedules: [] });
        if (cmd === "list_allowlist") return Promise.resolve({ items: [] });
        if (cmd === "get_focus_session") return Promise.resolve({ session: {
          name: "Deep Work",
          started_at_utc: new Date().toISOString(),
          duration_minutes: 25,
          expires_utc: new Date(Date.now() + 25*60*1000).toISOString(),
          remaining_seconds: 1500
        }});
        if (cmd === "catalog") return Promise.resolve({ apps: [], categories: [], limits: [] });
        if (cmd === "weekly_summary") return Promise.resolve({ days: [] });
        if (cmd === "blocked_apps") return Promise.resolve({ blocked: [] });
        
        return Promise.resolve({});
      }
    };
  });

  await page.goto("http://localhost:1420");
  // Wait for the app to render
  await page.waitForTimeout(3000);
  
  const artifactPath = "C:/Users/kacc2/.gemini/antigravity/brain/7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b/scratch/ui_screenshot.png";
  await page.screenshot({ path: artifactPath, fullPage: true });
  
  console.log("Screenshot saved to", artifactPath);
  await browser.close();
})();
