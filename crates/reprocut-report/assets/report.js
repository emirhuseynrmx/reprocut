(() => {
  "use strict";

  const root = document.documentElement;
  const copyButton = document.querySelector(".copy-command");
  const command = document.querySelector("#repro-command");
  const status = document.querySelector("#copy-status");
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const reveal = () => {
    const forced = Number(root.dataset.demoProgress);
    const progress = Number.isFinite(forced) ? Math.min(1, Math.max(0, forced)) : 1;
    root.style.setProperty("--reveal", String(reducedMotion ? 1 : progress));
  };

  requestAnimationFrame(reveal);

  window.addEventListener("reprocut:frame", reveal);

  copyButton?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(command?.textContent ?? "");
      copyButton.textContent = "Copied";
      status.textContent = "Reproduction command copied to clipboard.";
    } catch {
      status.textContent = "Clipboard unavailable. Select the command manually.";
    }
  });
})();
