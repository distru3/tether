import { chromium } from "playwright";
import fs from "fs";

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.on("console", msg => console.log("PAGE LOG:", msg.text()));
  page.on("pageerror", err => console.log("PAGE ERROR:", err.message));
  await page.setViewportSize({ width: 1280, height: 850 });
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      invoke: (cmd, args) => {
        console.log("Mock invoke:", cmd, args);
        if (cmd === "get_status") return Promise.resolve({
          agent_version: "0.1.0",
          tracker_backend: "win32",
          enforcement_backend: "win32",
          filter_backend: "hosts",
          tracking_available: true,
          blocks_encrypted_dns: true,
          strict_mode: false,
          pin_configured: true,
          show_hud_overlay: false,
          limit_cooldown_hours: 24,
          day_start_minutes: 0,
          idle_threshold_secs: 300,
          wildcard_domains: true,
          path_level: false,
        });
        if (cmd === "get_day_summary") return Promise.resolve({
          day: 20260912,
          total_seconds: 14520,
          limit_for: {},
          apps: [
            { id: 1, key: "visual_studio_code", label: "Visual Studio Code", seconds: 7200, color: "#A55B4B", category_id: 1 },
            { id: 2, key: "google_chrome", label: "Google Chrome", seconds: 4320, color: "#DCA06D", category_id: 2 },
            { id: 3, key: "spotify", label: "Spotify", seconds: 3000, color: "#C27D60", category_id: 3 }
          ],
          categories: [
            { id: 1, key: "development", label: "Productivity & Office", seconds: 7200, color: "#A55B4B" },
            { id: 2, key: "browsing", label: "Social Media", seconds: 4320, color: "#DCA06D" },
            { id: 3, key: "media", label: "Music & Audio", seconds: 3000, color: "#C27D60" }
          ],
          intervals: [
            { start_minute: 540, end_minute: 660, app_id: 1, duration_seconds: 7200 },
            { start_minute: 660, end_minute: 732, app_id: 2, duration_seconds: 4320 }
          ]
        });
        if (cmd === "get_catalog") return Promise.resolve({
          apps: [
            { id: 1, key: "visual_studio_code", display_name: "Visual Studio Code" },
            { id: 2, key: "google_chrome", display_name: "Google Chrome" },
            { id: 3, key: "spotify", display_name: "Spotify" }
          ],
          categories: [
            { id: 1, name: "Productivity & Office", kind: "limitable" },
            { id: 2, name: "Social Media", kind: "limitable" },
            { id: 3, name: "Games", kind: "limitable" }
          ],
          limits: [
            { id: 1, target: { kind: "category", id: 2 }, default_minutes: 60, weekday_minutes: [null, null, null, null, null, null, null], enabled: true }
          ],
          pending_limits: []
        });
        if (cmd === "get_weekly_summary") return Promise.resolve({
          days: [
            { day: 20260906, total_seconds: 12000 },
            { day: 20260907, total_seconds: 18000 },
            { day: 20260908, total_seconds: 15400 },
            { day: 20260909, total_seconds: 21000 },
            { day: 20260910, total_seconds: 19500 },
            { day: 20260911, total_seconds: 16000 },
            { day: 20260912, total_seconds: 14520 }
          ],
          week_total_seconds: 116420,
          prev_week_total_seconds: 108000
        });
        if (cmd === "get_blocked_apps") return Promise.resolve({ blocked: [] });
        if (cmd === "get_focus_session") return Promise.resolve(null);
        if (cmd === "get_blocklists") return Promise.resolve({ blocklists: [] });
        if (cmd === "list_block_rules") return Promise.resolve({ rules: [] });
        if (cmd === "list_sites") return Promise.resolve({ sites: [] });
        if (cmd === "list_schedules") return Promise.resolve({ schedules: [] });
        if (cmd === "list_allowlist") return Promise.resolve({ items: [] });
        if (cmd === "weekly_summary") return Promise.resolve({
          days: [
            { day: "2026-09-06", total_seconds: 12000 },
            { day: "2026-09-07", total_seconds: 18000 },
            { day: "2026-09-08", total_seconds: 15400 },
            { day: "2026-09-09", total_seconds: 21000 },
            { day: "2026-09-10", total_seconds: 19500 },
            { day: "2026-09-11", total_seconds: 16000 },
            { day: "2026-09-12", total_seconds: 14520 }
          ]
        });
        if (cmd === "blocked_apps") return Promise.resolve({ blocked: [] });
        if (cmd === "get_blocklists") return Promise.resolve({ blocklists: [] });
        if (cmd === "list_block_rules") return Promise.resolve({ rules: [] });
        if (cmd === "list_sites") return Promise.resolve({ sites: [] });

        return Promise.resolve({});
      },
      metadata: {
        currentWindow: { label: "main" }
      },
      transformCallback: (cb) => cb,
    };
    window.__TAURI__ = window.__TAURI_INTERNALS__;
    window.__TAURI_IPC__ = window.__TAURI_INTERNALS__.invoke;
    localStorage.setItem("screentime_first_run_completed", "true");
  });

  await page.goto("http://localhost:1420");
  // Wait for the app to render
  await page.waitForTimeout(1500);
  
  // Click on App Limits tab
  await page.click('button:has-text("App Limits")');
  await page.waitForTimeout(800);

  // Click on Add New Limit button
  const addBtn = page.locator('button:has-text("Add New Limit"), button:has-text("Add Limit")');
  if (await addBtn.count() > 0) {
    await addBtn.first().click();
    await page.waitForTimeout(500);
    // Open picker
    const pickerTrigger = page.locator('.app-picker__trigger');
    if (await pickerTrigger.count() > 0) {
      await pickerTrigger.first().click();
      await page.waitForTimeout(400);
    }
  }
  
  const artifactPath = "C:/Users/kacc2/.gemini/antigravity/brain/7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b/scratch/ui_screenshot_limits.png";
  await page.screenshot({ path: artifactPath, fullPage: true });
  
  console.log("Screenshot saved to", artifactPath);
  await browser.close();
})();
