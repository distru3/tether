import { chromium } from "playwright";
import fs from "fs";

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.setViewportSize({ width: 1280, height: 900 });

  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      invoke: (cmd, args) => {
        if (cmd === "get_status") return Promise.resolve({
          agent_version: "0.1.0",
          tracker_backend: "win32",
          enforcement_backend: "win32",
          filter_backend: "hosts",
          tracking_available: true,
          blocks_encrypted_dns: true,
          strict_mode: false,
          pin_configured: false,
          show_hud_overlay: true,
          limit_cooldown_hours: 24,
          day_start_minutes: 0,
          idle_threshold_secs: 60,
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
            { start_minute: 540, end_minute: 660, app_id: 1, duration_seconds: 7200 }
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
            { id: 1, target: { kind: "category", id: 2 }, default_minutes: 60, weekday_minutes: [null, null, null, null, null, null, null], enabled: true },
            { id: 2, target: { kind: "app", id: 3 }, default_minutes: 45, weekday_minutes: [null, null, null, null, null, null, null], enabled: false }
          ],
          pending_limits: [
            { id: 101, target: { kind: "app", id: 1 }, default_minutes: 120, weekday_minutes: [null, null, null, null, null, null, null], enabled: true, effective_at_utc: 1789330000 }
          ]
        });
        if (cmd === "get_weekly_summary") return Promise.resolve({
          days: [],
          week_total_seconds: 0,
          prev_week_total_seconds: 0
        });
        if (cmd === "get_blocked_apps") return Promise.resolve({ blocked: [] });
        if (cmd === "get_focus_session") return Promise.resolve(null);
        if (cmd === "list_manual_blocks") return Promise.resolve({
          domains: [
            "snapchat.com", "pinterest.com", "reddit.com", "linkedin.com",
            "tumblr.com", "weibo.com", "vkontakte.ru", "whatsapp.com", "telegram.org",
            "facebook.com", "instagram.com", "tiktok.com", "youtube.com", "x.com"
          ]
        });
        if (cmd === "get_blocklists") return Promise.resolve({ blocklists: [] });
        if (cmd === "list_block_rules") return Promise.resolve({ rules: [] });
        if (cmd === "list_sites") return Promise.resolve({ sites: [] });
        if (cmd === "list_schedules") return Promise.resolve({ schedules: [] });
        if (cmd === "list_allowlist") return Promise.resolve({ items: [] });
        return Promise.resolve({});
      },
      metadata: { currentWindow: { label: "main" } },
      transformCallback: (cb) => cb,
    };
    window.__TAURI__ = window.__TAURI_INTERNALS__;
    window.__TAURI_IPC__ = window.__TAURI_INTERNALS__.invoke;
    localStorage.setItem("screentime_first_run_completed", "true");
  });

  const outDir = "C:/Users/kacc2/.gemini/antigravity/brain/7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b/scratch";

  await page.goto("http://localhost:1420");
  await page.waitForTimeout(1500);

  // 1. Limits View
  await page.click('button:has-text("App Limits")');
  await page.waitForTimeout(600);
  await page.screenshot({ path: `${outDir}/polish_limits.png`, fullPage: true });
  console.log("Captured polish_limits.png");

  // Open Limit Editor Dialog
  const addBtn = page.locator('button:has-text("Add New Limit")');
  if (await addBtn.count() > 0) {
    await addBtn.first().click();
    await page.waitForTimeout(500);
    // Toggle weekday overrides
    const weekBtn = page.locator('.weekday-toggle');
    if (await weekBtn.count() > 0) {
      await weekBtn.first().click();
      await page.waitForTimeout(300);
    }
    await page.screenshot({ path: `${outDir}/polish_limit_editor.png`, fullPage: true });
    console.log("Captured polish_limit_editor.png");
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);
  }

  // 2. Web Filter View
  await page.click('button:has-text("Web Filter")');
  await page.waitForTimeout(600);
  const domainPanel = page.locator('.web-filter-domain-panel');
  await domainPanel.scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
  await domainPanel.screenshot({ path: `${outDir}/polish_webfilter_table.png` });
  console.log("Captured polish_webfilter_table.png");

  // 3. Settings View
  await page.click('button:has-text("Settings")');
  await page.waitForTimeout(600);
  await page.screenshot({ path: `${outDir}/polish_settings.png`, fullPage: true });
  console.log("Captured polish_settings.png");

  // Open Set PIN modal
  const setPinBtn = page.locator('button:has-text("Set PIN")');
  if (await setPinBtn.count() > 0) {
    await setPinBtn.first().click();
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${outDir}/polish_pin_setup.png`, fullPage: true });
    console.log("Captured polish_pin_setup.png");
    // Close modal
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);
  }

  // 4. Return to Dashboard and open Categorize dialog if possible
  await page.click('button:has-text("Dashboard")');
  await page.waitForTimeout(600);
  const catBtn = page.locator('button[title*="Categorize"], button:has-text("Edit Category"), .btn-tag-edit, button:has-text("Categorize")');
  if (await catBtn.count() > 0) {
    await catBtn.first().click();
    await page.waitForTimeout(500);
    // Click category select to show custom dropdown
    const pickerTrigger = page.locator('.app-picker__trigger');
    if (await pickerTrigger.count() > 0) {
      await pickerTrigger.first().click();
      await page.waitForTimeout(300);
    }
    await page.screenshot({ path: `${outDir}/polish_categorize_dialog.png`, fullPage: true });
    console.log("Captured polish_categorize_dialog.png");
  }

  await browser.close();
})();
