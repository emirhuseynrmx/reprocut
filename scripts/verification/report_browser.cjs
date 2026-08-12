"use strict";

const path = require("node:path");
const { pathToFileURL } = require("node:url");

const playwrightModule = process.env.REPROCUT_PLAYWRIGHT_MODULE;
const reportPath = process.argv[2];
if (!playwrightModule || !reportPath) {
  process.stderr.write(
    "usage: REPROCUT_PLAYWRIGHT_MODULE=/path/to/playwright node report_browser.cjs REPORT\n",
  );
  process.exit(2);
}

const { chromium } = require(playwrightModule);
const reportUrl = pathToFileURL(path.resolve(reportPath)).href;
const viewports = [
  { name: "desktop", width: 1440, height: 1000 },
  { name: "mobile", width: 390, height: 844 },
];

(async () => {
  const browser = await chromium.launch({ channel: "msedge", headless: true });
  const evidence = [];
  try {
    for (const viewport of viewports) {
      const context = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        reducedMotion: "reduce",
      });
      const page = await context.newPage();
      const errors = [];
      const requests = [];
      page.on("console", (message) => {
        if (message.type() === "error") errors.push(message.text());
      });
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("request", (request) => requests.push(request.url()));
      await page.goto(reportUrl, { waitUntil: "load" });
      await page.waitForFunction(
        () => getComputedStyle(document.documentElement).getPropertyValue("--reveal").trim() === "1",
      );

      const contract = await page.evaluate(() => ({
        horizontalOverflow: document.documentElement.scrollWidth > window.innerWidth,
        reveal: getComputedStyle(document.documentElement).getPropertyValue("--reveal").trim(),
        title: document.title,
      }));
      await page.locator(".copy-command").focus();
      const outline = await page.locator(".copy-command").evaluate((element) => {
        const style = getComputedStyle(element);
        return { style: style.outlineStyle, width: style.outlineWidth };
      });
      await page.locator(".copy-command").click();
      await page.waitForFunction(
        () => (document.querySelector("#copy-status")?.textContent ?? "").trim().length > 0,
      );
      const copyStatus = await page.locator("#copy-status").textContent();
      const downloadPromise = page.waitForEvent("download");
      await page.locator(".download-issue").click();
      const download = await downloadPromise;
      const suggestedFilename = download.suggestedFilename();

      if (errors.length > 0) throw new Error(`${viewport.name} browser errors: ${errors.join("; ")}`);
      if (requests.some((url) => !url.startsWith("file:"))) {
        throw new Error(`${viewport.name} made an external request`);
      }
      if (contract.horizontalOverflow) throw new Error(`${viewport.name} has horizontal overflow`);
      if (contract.reveal !== "1") throw new Error(`${viewport.name} reduced motion did not settle`);
      if (outline.style === "none" || outline.width === "0px") {
        throw new Error(`${viewport.name} keyboard focus is not visible`);
      }
      if (!copyStatus) throw new Error(`${viewport.name} copy action gave no accessible status`);
      if (suggestedFilename !== "issue.md") {
        throw new Error(`${viewport.name} issue download filename changed: ${suggestedFilename}`);
      }
      evidence.push({
        viewport: viewport.name,
        requests: requests.length,
        ...contract,
        outline,
        copyStatus,
        suggestedFilename,
      });
      await context.close();
    }
  } finally {
    await browser.close();
  }
  process.stdout.write(`${JSON.stringify(evidence)}\n`);
})().catch((error) => {
  process.stderr.write(`${error.stack ?? error}\n`);
  process.exitCode = 1;
});
