"use strict";

const path = require("node:path");
const { pathToFileURL } = require("node:url");

const playwrightPath = process.env.REPROCUT_PLAYWRIGHT_MODULE;
if (!playwrightPath) {
  throw new Error("REPROCUT_PLAYWRIGHT_MODULE is required");
}
const { chromium } = require(playwrightPath);

const reportPath = path.resolve(process.argv[2]);
const frameDirectory = path.resolve(process.argv[3]);
const frameCount = 24;

const ease = (value) => 1 - Math.pow(1 - value, 3);

(async () => {
  const browser = await chromium.launch({ channel: "msedge", headless: true });
  try {
    const page = await browser.newPage({
      viewport: { width: 1200, height: 675 },
      deviceScaleFactor: 1,
      colorScheme: "light",
      reducedMotion: "no-preference",
    });
    await page.goto(pathToFileURL(reportPath).href, { waitUntil: "load" });
    await page.addStyleTag({
      content: "*,*::before,*::after{transition:none!important;animation:none!important}",
    });
    const cutTop = await page.locator(".cut-table").evaluate((element) => element.offsetTop);

    for (let index = 0; index < frameCount; index += 1) {
      let progress = 0;
      let scrollY = 0;
      if (index > 0 && index <= 11) {
        const phase = index / 11;
        scrollY = ease(phase) * Math.max(0, cutTop - 72);
        progress = phase * 0.15;
      } else if (index > 11) {
        scrollY = Math.max(0, cutTop - 72);
        progress = 0.15 + ((index - 11) / 12) * 0.85;
      }

      await page.evaluate(
        ({ forcedProgress, forcedScroll }) => {
          const root = document.documentElement;
          root.dataset.demoProgress = String(forcedProgress);
          root.style.setProperty("--reveal", String(forcedProgress));
          window.scrollTo(0, forcedScroll);
          window.dispatchEvent(new Event("reprocut:frame"));
        },
        { forcedProgress: progress, forcedScroll: scrollY },
      );
      await page.screenshot({
        path: path.join(frameDirectory, `frame-${String(index).padStart(2, "0")}.png`),
        animations: "disabled",
      });
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  process.stderr.write(`${error.stack ?? error}\n`);
  process.exitCode = 1;
});
