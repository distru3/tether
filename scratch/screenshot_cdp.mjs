import { chromium } from "playwright";

(async () => {
  console.log("Connecting to Tauri CDP on port 9222...");
  const browser = await chromium.connectOverCDP("http://localhost:9222");
  const defaultContext = browser.contexts()[0];
  const page = defaultContext.pages()[0];
  
  if (!page) {
    console.log("No page found!");
    process.exit(1);
  }

  // Unconstrain the height to capture the full scrolling area
  await page.evaluate(() => {
    const style = document.createElement("style");
    style.innerHTML = `
      html, body, #root, .app-root, .app-layout, .app-main-content, .sheet {
        height: auto !important;
        min-height: auto !important;
        max-height: none !important;
        overflow: visible !important;
        position: static !important;
      }
    `;
    document.head.appendChild(style);
  });
  
  await page.waitForTimeout(1000); // Give it a sec to repaint
  
  const artifactPath = "C:/Users/kacc2/.gemini/antigravity/brain/7f37d91f-66d5-41f3-b9c1-bd4d2c44b88b/ui_screenshot_scrolled.png";
  await page.screenshot({ path: artifactPath, fullPage: true });
  
  console.log("Screenshot saved to", artifactPath);
  await browser.close();
})();
