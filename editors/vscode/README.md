# ReproCut for VS Code and Cursor

This 0.1 extension is deliberately a thin local client. It writes a versioned
request, invokes `reprocut protocol run`, validates every JSONL event, and opens
the verified artifact. It contains no reducer and never downloads a binary.

Configure `reprocut.binary` if `reprocut` is not on `PATH`. The same extension
package works in Cursor through the compatible VS Code extension API.

Run its dependency-free protocol contracts with:

```console
node --test test/*.test.js
```
