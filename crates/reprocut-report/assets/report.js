(() => {
  "use strict";

  const root = document.documentElement;
  const copyButton = document.querySelector(".copy-command");
  const command = document.querySelector("#repro-command");
  const status = document.querySelector("#copy-status");
  const issueButton = document.querySelector(".download-issue");
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const reveal = () => {
    const forced = Number(root.dataset.demoProgress);
    const progress = Number.isFinite(forced) ? Math.min(1, Math.max(0, forced)) : 1;
    root.style.setProperty("--reveal", String(reducedMotion ? 1 : progress));
  };

  requestAnimationFrame(reveal);

  window.addEventListener("reprocut:frame", reveal);

  const fallbackCopy = (value) => {
    const input = document.createElement("textarea");
    input.value = value;
    input.setAttribute("readonly", "");
    input.style.position = "fixed";
    input.style.opacity = "0";
    document.body.append(input);
    input.select();
    const copied = document.execCommand("copy");
    input.remove();
    return copied;
  };

  copyButton?.addEventListener("click", async () => {
    const value = command?.textContent ?? "";
    try {
      if (!navigator.clipboard?.writeText) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(value);
      copyButton.textContent = "Copied";
      status.textContent = "Reproduction command copied to clipboard.";
    } catch {
      if (fallbackCopy(value)) {
        copyButton.textContent = "Copied";
        status.textContent = "Reproduction command copied with the compatibility fallback.";
      } else {
        status.textContent = "Clipboard unavailable. Select the command manually.";
      }
    }
  });

  issueButton?.addEventListener("click", () => {
    const encoded = issueButton.dataset.issue ?? "";
    const binary = atob(encoded);
    const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
    const url = URL.createObjectURL(new Blob([bytes], { type: "text/markdown;charset=utf-8" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = "issue.md";
    link.click();
    URL.revokeObjectURL(url);
    status.textContent = "GitHub issue Markdown downloaded.";
  });
})();
