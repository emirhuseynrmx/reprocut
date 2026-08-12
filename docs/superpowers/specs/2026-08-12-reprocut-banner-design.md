# ReproCut forensic banner design

## Objective

Replace `assets/reprocut-banner.svg` with a static, self-contained GitHub banner
that feels like the same visual system as `assets/reprocut-demo.gif`. A developer
should understand the product in under two seconds: ReproCut reduces an 18-file
failure reproduction to three retained files while preserving the same failure.

## Canvas and portability

- Keep the existing `1600 × 600` view box so the banner remains suitable for the
  README and social previews.
- Produce a single SVG with no network requests, scripts, raster images, filters,
  animation, or external fonts.
- Include an accessible `<title>` and `<desc>` that state the reduction and
  same-failure result.
- Use SVG primitives and portable font stacks only. Important meaning must not
  depend on font metrics, color alone, or unsupported effects.

## Visual system

The banner is a final-state forensic reduction record, not a generic startup
hero. It reuses the demo's technical-paper material and disciplined interface
language:

- paper: `#FBF9F3`;
- grid: `#D3D8DC`;
- ink: `#171A1D`;
- cobalt: `#3157D5`;
- signal red: `#BF4E3A`;
- muted text: `#66717D`.

The outer grid and bordered paper panel echo the GIF. Corners remain modest and
lines remain crisp. There are no gradients, glows, glass panels, faux 3D,
decorative particles, or unrelated iconography.

## Composition

The composition has two unequal columns separated by whitespace rather than a
decorative rule.

```text
┌──────────────────────────────────────────────────────────────────┐
│ REPRO/CUT                       FAILURE REDUCTION RECORD          │
│ ──────────────────────────────────────────────────────────────── │
│ $ reprocut minimize ./checkout   [file matrix: 18 total]         │
│ same failure, less project       [15 cut / muted, 3 retained]    │
│ 18  →  03                        [red cut trace avoids retained]  │
│ FILES   RETAINED                                                  │
│ ──────────────────────────────────────────────────────────────── │
│ SAME FAILURE · STRICT 3/3 · 24 CANDIDATES                        │
│ bug.py · checkout.py · fixtures/order.json   sha256:e9fc…        │
└──────────────────────────────────────────────────────────────────┘
```

The left column carries the thesis: wordmark, real command, plain-language
promise, and measured `18 → 03` result. The right column contains 18 file cards
in a six-by-three matrix. Fifteen cards are visibly rejected using muted strokes
and a restrained red cut mark. Three retained cards use cobalt borders and a
small internal line treatment matching the demo GIF.

## Signature element

A single signal-red cut trace moves through the rejected file population while
avoiding the three retained cards. It represents reduction without suggesting
that ReproCut mutates the source checkout. This is the one expressive device;
all surrounding typography and rules stay quiet.

## Copy and evidence

Copy is concrete and derived from checked-in demo evidence:

- `REPRO/CUT`;
- `FAILURE REDUCTION RECORD`;
- `$ reprocut minimize --root ./checkout`;
- `same failure, less project`;
- `18 FILES → 03 RETAINED`;
- `SAME FAILURE · STRICT 3/3 · 24 CANDIDATES`;
- `bug.py · checkout.py · fixtures/order.json`;
- a shortened form of the current demo fingerprint.

The SVG must not introduce a speed claim, global-minimum claim, or result that
disagrees with `demo/result/reduction.json`.

## Validation

Implementation is complete only when:

1. the SVG parses as XML and contains no external resource or script reference;
2. its view box remains `0 0 1600 600` and its accessible title/description are
   present;
3. the measured `18`, `03`, `24`, and `3/3` claims agree with checked-in evidence;
4. a rendered PNG is inspected at full size and at README-like width, with no
   clipped or overlapping text;
5. the banner is visually compared with the first frame of the demo GIF and
   clearly reads as the same product family;
6. the existing demo asset tests and repository verification remain green.

## Scope

This change replaces the static SVG banner only. It does not alter the demo GIF,
wordmark naming, README copy, evidence, CLI behavior, or release workflows.
