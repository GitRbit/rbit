// Imports tinted-theming base16/base24 colour schemes as Rbit themes.
//
//   git clone --depth 1 https://github.com/tinted-theming/schemes /tmp/schemes
//   npm run themes -- /tmp/schemes
//
// The schemes checkout is treated as untrusted data: only its YAML files are read, with
// a minimal parser for the flat format the schemes use; every colour must be `#rrggbb`,
// every slug `[a-z0-9-]+`, and names are length-limited and escaped for each output.
//
// Outputs (all committed, so builds and CI never need the network):
//   crates/rbit-web/assets/themes/<slug>.css  one theme, loaded only while it is active
//   crates/rbit-web/assets/themes/all.css     every theme, loaded only by the gallery
//   crates/rbit-web/assets/themes/LICENSE.txt scheme authors and the MIT licence
//   crates/rbit-web/src/themes/catalog.rs     the compile-time catalog
//
// Each scheme's palette is mapped onto Rbit's semantic tokens following the base16
// styling guidelines, then checked for contrast: body text (ink, ink-muted, status and
// syntax colours) must reach WCAG AA (4.5:1) against every background it sits on, faint
// text and comments 3:1, and the container border must stay visible. A colour that falls
// short is nudged towards the scheme's extreme (white on dark, black on light) just far
// enough; only schemes whose foreground and background are indistinguishable are dropped.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const THEMES_DIR = join(ROOT, "crates/rbit-web/assets/themes");
const CATALOG_RS = join(ROOT, "crates/rbit-web/src/themes/catalog.rs");

const SYSTEMS = ["base24", "base16"]; // base24 first: it wins when a scheme exists in both.
const SLUG_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const HEX_RE = /^#[0-9a-f]{6}$/;
const MAX_NAME = 80;
const MAX_AUTHOR = 200;

// ------------------------------------------------------------------ input

const source = process.argv[2];
if (!source) {
  console.error("usage: npm run themes -- <path to a tinted-theming/schemes checkout>");
  process.exit(2);
}
const commit = gitCommit(source);

function gitCommit(dir) {
  try {
    const sha = execFileSync("git", ["-C", dir, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
    if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error(`unexpected commit id ${sha}`);
    return sha;
  } catch (error) {
    console.error(`could not read the schemes commit: ${error.message}`);
    process.exit(1);
  }
}

/**
 * Parses the flat YAML the schemes use: top-level `key: value` pairs and one nested
 * `palette:` map. Comments, document markers and block-scalar continuation lines
 * (only `description` uses them) are skipped. Values may be single-, double- or unquoted.
 */
function parseScheme(text) {
  const scheme = { palette: {} };
  let inPalette = false;
  for (const raw of text.split(/\r?\n/)) {
    if (/^\s*(#|$)/.test(raw) || raw.startsWith("---")) continue;
    const indented = /^\s/.test(raw);
    const match = /^\s*([A-Za-z0-9_-]+):\s*(.*)$/.exec(raw);
    if (!indented) inPalette = false;
    if (!match) continue;
    const [, key, rest] = match;
    if (!indented) {
      if (key === "palette") inPalette = true;
      else scheme[key] = scalar(rest);
    } else if (inPalette) {
      scheme.palette[key] = scalar(rest);
    }
  }
  return scheme;
}

function scalar(rest) {
  const value = rest.trim();
  const quoted = /^"((?:[^"\\]|\\.)*)"|^'((?:[^']|'')*)'/.exec(value);
  if (quoted) return quoted[1] !== undefined ? quoted[1].replace(/\\(.)/g, "$1") : quoted[2].replace(/''/g, "'");
  return value.replace(/\s+#.*$/, "").trim();
}

/** Printable text only, whitespace collapsed, bounded length. */
function cleanText(value, max) {
  if (typeof value !== "string") return "";
  const cleaned = value.replace(/[\p{C}]/gu, " ").replace(/\s+/g, " ").trim();
  return cleaned.length > max ? `${cleaned.slice(0, max - 1)}…` : cleaned;
}

// ------------------------------------------------------------------ colour maths

const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
const toHex = (rgb) => `#${rgb.map((c) => Math.round(Math.min(1, Math.max(0, c)) * 255).toString(16).padStart(2, "0")).join("")}`;
const linear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const gamma = (c) => (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055);

function luminance(h) {
  const [r, g, b] = hex(h).map(linear);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a, b) {
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

function toOklab(h) {
  const [r, g, b] = hex(h).map(linear);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function fromOklab([L, a, b]) {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return toHex(
    [
      4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
      -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
      -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
    ].map((c) => gamma(Math.min(1, Math.max(0, c)))),
  );
}

/** `amount` of the way from `a` to `b`, interpolated in OKLab (like CSS `color-mix(in oklab)`). */
function mix(a, b, amount) {
  const [x, y] = [toOklab(a), toOklab(b)];
  return fromOklab(x.map((v, i) => v + (y[i] - v) * amount));
}

/** The smallest step from `from` towards `toward` whose contrast with `from` reaches `target`. */
function stepAway(from, toward, target) {
  if (contrast(from, toward) < target) return null;
  let [lo, hi] = [0, 1];
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    if (contrast(from, mix(from, toward, mid)) >= target) hi = mid;
    else lo = mid;
  }
  return mix(from, toward, hi);
}

/** `colour` nudged towards `extreme` until it reaches `target` contrast with every background. */
function ensure(colour, backgrounds, target, extreme) {
  const passes = (c) => backgrounds.every((bg) => contrast(c, bg) >= target);
  if (passes(colour)) return { value: colour, adjusted: false };
  for (let t = 0.02; t <= 1.0001; t += 0.02) {
    const candidate = mix(colour, extreme, t);
    if (passes(candidate)) return { value: candidate, adjusted: true };
  }
  return { value: extreme, adjusted: true };
}

/** Whether `c` lies on the same side of `base` as `ink` (lighter on dark schemes, darker on light). */
const inkSide = (c, base, ink) => Math.sign(luminance(c) - luminance(base)) === Math.sign(luminance(ink) - luminance(base));

// ------------------------------------------------------------------ token mapping

// Contrast targets. Elevation steps are measured against the container surface.
const AA = 4.5;
const FAINT = 3;
const STEP = { canvasLight: 1.05, canvasDark: 1.08, subtle: 1.05, hover: 1.12, line: 1.3, lineOnCanvas: 1.15, lineStrong: 1.65 };

/**
 * base16 → Rbit tokens. `--surface` (containers) is the scheme's own editor background
 * (base00), so code reads exactly as the scheme intends; the page `--canvas` sits one
 * step deeper. Raised and interactive backgrounds, borders and text are derived towards
 * the foreground, tinted by the scheme's own greys (base01–base03) where they fit.
 */
function mapScheme(p, dark) {
  const notes = [];
  const extreme = dark ? "#ffffff" : "#000000";
  const fg = p.base05;
  let surface = p.base00;

  // Page canvas: one step darker than the containers on both variants.
  let canvas;
  if (dark) {
    const deeper = [p.base11, p.base10].find(
      (c) => c && luminance(c) < luminance(surface) && contrast(c, surface) >= 1.04 && contrast(c, surface) <= 1.3,
    );
    canvas = deeper ?? stepAway(surface, "#000000", STEP.canvasDark);
    if (!canvas) {
      // base00 is (nearly) black already: lift the containers instead.
      canvas = surface;
      surface = stepAway(canvas, fg, STEP.canvasDark) ?? mix(canvas, fg, 0.08);
    }
  } else {
    canvas = stepAway(surface, mix(fg, "#000000", 0.5), STEP.canvasLight) ?? mix(surface, "#000000", 0.04);
  }

  const toward = (c) => (c && inkSide(c, surface, fg) && contrast(c, surface) >= 1.1 ? c : fg);
  const subtle = stepAway(surface, toward(p.base01), STEP.subtle) ?? mix(surface, fg, 0.04);
  const hover = stepAway(surface, toward(p.base02 ?? p.base01), STEP.hover) ?? mix(surface, fg, 0.1);
  let line = stepAway(surface, toward(p.base02), STEP.line) ?? mix(surface, fg, 0.25);
  if (contrast(line, canvas) < STEP.lineOnCanvas) line = stepAway(canvas, fg, STEP.lineOnCanvas) ?? line;
  const lineStrong = stepAway(surface, toward(p.base03), STEP.lineStrong) ?? mix(surface, fg, 0.4);

  const backgrounds = [canvas, surface, subtle, hover];
  const fix = (name, colour, bgs, target) => {
    const { value, adjusted } = ensure(colour, bgs, target, extreme);
    if (adjusted) notes.push(name);
    return value;
  };

  const ink = fix("ink", fg, backgrounds, AA);
  const inkMuted = fix("ink-muted", p.base04, [canvas, surface, subtle], AA);
  const inkFaint = fix("ink-faint", p.base03, [canvas, surface, subtle], FAINT);
  const code = [surface, subtle];
  const pick = (a, b) => (contrast(a, surface) >= contrast(b, surface) ? a : b);
  const danger = fix("danger", p.base08, [surface, subtle], AA);

  const tokens = {
    canvas,
    surface,
    subtle,
    hover,
    line,
    "line-strong": lineStrong,
    ink,
    "ink-muted": inkMuted,
    "ink-faint": inkFaint,
    inverted: ink,
    "inverted-hover": mix(ink, surface, 0.18),
    "inverted-ink": surface,
    success: fix("success", p.base0B, [surface, subtle], AA),
    danger,
    "danger-subtle": mix(surface, danger, 0.14),
    warning: fix("warning", pick(p.base0A, p.base09), [surface, subtle], AA),
    "syntax-comment": fix("syntax-comment", p.base03, code, FAINT),
    "syntax-keyword": fix("syntax-keyword", p.base0E, code, AA),
    "syntax-string": fix("syntax-string", p.base0B, code, AA),
    "syntax-number": fix("syntax-number", p.base09, code, AA),
    "syntax-function": fix("syntax-function", p.base0D, code, AA),
    "syntax-type": fix("syntax-type", p.base0A, code, AA),
    "syntax-tag": fix("syntax-tag", p.base08, code, AA),
    "syntax-attribute": fix("syntax-attribute", p.base0C, code, AA),
  };
  return { tokens, notes };
}

// ------------------------------------------------------------------ families

// Display names for families whose prefix the automatic grouping gets wrong or splits.
const FAMILY_OVERRIDES = [
  [/^ros[ée] pine/i, "Rosé Pine"],
  [/^tokyo night/i, "Tokyo Night"],
  [/^tokyo city/i, "Tokyo City"],
  [/^tokyodark/i, "Tokyo Night"],
  [/^one ?(dark|light)|^onedark/i, "One"],
  [/^gruvbox material/i, "Gruvbox Material"],
  [/^gruvbox/i, "Gruvbox"],
  [/^catppuccin/i, "Catppuccin"],
  [/^solarized/i, "Solarized"],
  [/^selenized/i, "Selenized"],
  [/^atelier/i, "Atelier"],
  [/^black metal/i, "Black Metal"],
  [/^github/i, "GitHub"],
  [/^material/i, "Material"],
  [/^everforest/i, "Everforest"],
  [/^kanagawa/i, "Kanagawa"],
  [/^nord/i, "Nord"],
  [/^tomorrow/i, "Tomorrow"],
  [/^horizon/i, "Horizon"],
  [/^windows/i, "Windows"],
  [/^papercolor/i, "PaperColor"],
  [/^penumbra/i, "Penumbra"],
  [/^precious/i, "Precious"],
  [/^sakura/i, "Sakura"],
  [/^silk/i, "Silk"],
  [/^standardized/i, "Standardized"],
  [/^stella/i, "Stella"],
  [/^summerfruit/i, "Summerfruit"],
  [/^tango/i, "Tango"],
  [/^vulcan/i, "Vulcan"],
  [/^equilibrium/i, "Equilibrium"],
  [/^harmonic16/i, "Harmonic16"],
  [/^humanoid/i, "Humanoid"],
  [/^mellow/i, "Mellow"],
  [/^oxocarbon/i, "Oxocarbon"],
  [/^primer/i, "Primer"],
  [/^da one/i, "Da One"],
  [/^brush trees/i, "Brush Trees"],
  [/^arroz con dulce/i, "Arroz con Dulce"],
  [/^boo-shnickle/i, "boo-shnickle"],
  [/^cerulean signal/i, "Cerulean Signal"],
  [/^builtin/i, "iTerm2 Builtin"],
];

// Words too generic to name a family on their own ("Red Alert" and "Red Sands" are unrelated).
const GENERIC_WORDS = new Set(["black", "blue", "dark", "dawn", "day", "deep", "green", "light", "night", "red", "space", "white"]);

const words = (name) => name.split(/[\s,()\-_/]+/).filter(Boolean);

/** Groups schemes by the shortest leading run of words they share with another scheme. */
function assignFamilies(schemes) {
  const prefixCounts = new Map();
  for (const scheme of schemes) {
    const w = words(scheme.name).map((x) => x.toLowerCase());
    for (let n = 1; n <= w.length; n++) {
      const key = w.slice(0, n).join(" ");
      prefixCounts.set(key, (prefixCounts.get(key) ?? 0) + 1);
    }
  }
  for (const scheme of schemes) {
    const override = FAMILY_OVERRIDES.find(([re]) => re.test(scheme.name));
    if (override) {
      scheme.family = override[1];
      continue;
    }
    const original = words(scheme.name);
    const lower = original.map((x) => x.toLowerCase());
    let family = scheme.name;
    for (let n = 1; n <= lower.length; n++) {
      const prefix = lower.slice(0, n);
      if (prefix.every((w) => GENERIC_WORDS.has(w))) continue;
      if ((prefixCounts.get(prefix.join(" ")) ?? 0) >= 2) {
        family = original.slice(0, n).join(" ");
        break;
      }
    }
    scheme.family = family.charAt(0).toUpperCase() + family.slice(1);
  }
}

// ------------------------------------------------------------------ light/dark counterparts

// Words that name a variant rather than a scheme: "Gruvbox dark, hard" and "Gruvbox light,
// hard" are one scheme, as are Catppuccin Latte and Mocha or Rosé Pine Dawn and Rosé Pine.
const VARIANT_WORDS = new Set([
  "dark", "light", "darker", "lighter", "day", "night", "dawn", "dusk", "morning", "evening", "moon", "storm",
  "mirage", "latte", "mocha", "frappe", "frappé", "macchiato", "dimmed",
]);

/** Lexicographic comparison of two equal-length number arrays. */
function compareRanks(a, b) {
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i];
  return 0;
}

/** The words that identify a scheme within its family, without variant words. */
function coreWords(scheme) {
  const family = new Set(words(scheme.family).map((w) => w.toLowerCase()));
  return new Set(words(scheme.name).map((w) => w.toLowerCase()).filter((w) => !family.has(w) && !VARIANT_WORDS.has(w)));
}

/**
 * Pairs each scheme with its closest sibling of the opposite variant in the same family, so
 * the header's light/dark toggle keeps the scheme ("Gruvbox dark, hard" ↔ "Gruvbox light,
 * hard"). Siblings must share their identifying words, or one of them must have none (the
 * family's plain member). Ties prefer the same system, then the most extreme background
 * (Catppuccin Latte → Mocha), then the shorter name.
 */
function assignCounterparts(schemes) {
  const lum = (s) => luminance(s.tokens.canvas);
  for (const scheme of schemes) {
    const core = coreWords(scheme);
    let best = null;
    for (const other of schemes) {
      if (other.family !== scheme.family || other.variant === scheme.variant) continue;
      const otherCore = coreWords(other);
      const shared = [...core].filter((w) => otherCore.has(w)).length;
      const differing = core.size + otherCore.size - 2 * shared;
      if (shared === 0 && core.size > 0 && otherCore.size > 0) continue;
      const rank = [
        differing === 0 ? 1 : 0,
        shared,
        -differing,
        other.system === scheme.system ? 1 : 0,
        other.variant === "dark" ? -lum(other) : lum(other),
        -other.name.length,
      ];
      if (!best || compareRanks(rank, best.rank) > 0) best = { other, rank };
    }
    scheme.counterpart = best?.other.slug ?? null;
  }
}

// ------------------------------------------------------------------ import

const dropped = [];
const adjusted = [];
const seenNames = new Map();
const schemes = [];

for (const system of SYSTEMS) {
  const dir = join(source, system);
  const files = readdirSync(dir).filter((f) => /\.ya?ml$/.test(f)).sort();
  for (const file of files) {
    const id = file.replace(/\.ya?ml$/, "");
    const label = `${system}/${file}`;
    if (!SLUG_RE.test(id)) {
      dropped.push(`${label}: file name is not a valid slug`);
      continue;
    }
    const raw = parseScheme(readFileSync(join(dir, file), "utf8"));
    if (raw.system !== system) {
      dropped.push(`${label}: declares system "${raw.system}"`);
      continue;
    }
    const keys = Array.from({ length: system === "base24" ? 24 : 16 }, (_, i) => `base${i.toString(16).toUpperCase().padStart(2, "0")}`);
    const palette = {};
    const invalid = keys.filter((key) => {
      const value = String(raw.palette[key] ?? "").toLowerCase();
      palette[key] = value;
      return !HEX_RE.test(value);
    });
    if (invalid.length) {
      dropped.push(`${label}: invalid colours ${invalid.join(", ")}`);
      continue;
    }
    const name = cleanText(raw.name, MAX_NAME);
    if (!name) {
      dropped.push(`${label}: no name`);
      continue;
    }
    const nameKey = name.toLowerCase().replace(/[^a-z0-9]+/g, "");
    if (seenNames.has(nameKey)) {
      dropped.push(`${label}: same scheme as ${seenNames.get(nameKey)}`);
      continue;
    }
    if (contrast(palette.base00, palette.base05) < 2) {
      dropped.push(`${label}: foreground and background are indistinguishable`);
      continue;
    }
    const dark = luminance(palette.base00) < luminance(palette.base05);
    const declared = raw.variant === "light" || raw.variant === "dark" ? raw.variant : null;
    const variant = dark ? "dark" : "light";
    if (declared && declared !== variant) adjusted.push(`${label}: declared ${declared}, colours say ${variant}`);

    const { tokens, notes } = mapScheme(palette, dark);
    for (const [token, value] of Object.entries(tokens)) {
      if (!HEX_RE.test(value)) throw new Error(`${label}: derived ${token} = ${value}`);
    }
    if (notes.length) adjusted.push(`${label}: raised contrast of ${notes.join(", ")}`);

    seenNames.set(nameKey, label);
    schemes.push({
      slug: `${system}-${id}`,
      name,
      author: cleanText(raw.author, MAX_AUTHOR) || "Unknown",
      system,
      variant,
      tokens,
      swatches: [palette.base08, palette.base09, palette.base0A, palette.base0B, palette.base0C, palette.base0D, palette.base0E, palette.base0F],
    });
  }
}

assignFamilies(schemes);
assignCounterparts(schemes);
schemes.sort((a, b) => (a.slug < b.slug ? -1 : 1));

// ------------------------------------------------------------------ output

const short = commit.slice(0, 12);
const cssComment = (text) => text.replace(/\*\//g, "* /");
const rule = (s) =>
  `[data-theme="${s.slug}"]{color-scheme:${s.variant};${Object.entries(s.tokens)
    .map(([k, v]) => `--${k}:${v}`)
    .join(";")}}`;

rmSync(THEMES_DIR, { recursive: true, force: true });
mkdirSync(THEMES_DIR, { recursive: true });
for (const s of schemes) {
  writeFileSync(join(THEMES_DIR, `${s.slug}.css`), `/* ${cssComment(s.name)} · tinted-theming/schemes@${short} · MIT */\n${rule(s)}\n`);
}
writeFileSync(
  join(THEMES_DIR, "all.css"),
  `/* ${schemes.length} themes from tinted-theming/schemes@${short} (MIT); see LICENSE.txt. Generated by scripts/themes.mjs. */\n${schemes.map(rule).join("\n")}\n`,
);

const licence = readFileSync(join(source, "LICENSE"), "utf8").replace(/\r\n/g, "\n").trim();
writeFileSync(
  join(THEMES_DIR, "LICENSE.txt"),
  [
    "Rbit's tinted themes are generated from the tinted-theming colour schemes:",
    `https://github.com/tinted-theming/schemes (commit ${commit}).`,
    "",
    licence,
    "",
    "Schemes and their authors:",
    "",
    ...schemes.map((s) => `  ${s.slug}: ${s.name} by ${s.author}`),
    "",
  ].join("\n"),
);

const rs = (text) => `"${text.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
const variantRs = (v) => (v === "dark" ? "Variant::Dark" : "Variant::Light");
const systemRs = (s) => (s === "base24" ? "System::Base24" : "System::Base16");
writeFileSync(
  CATALOG_RS,
  [
    `//! The tinted theme catalog: ${schemes.length} schemes from`,
    `//! [tinted-theming/schemes](https://github.com/tinted-theming/schemes) (MIT) at commit`,
    `//! \`${commit}\`.`,
    "//!",
    "//! Generated by `scripts/themes.mjs`; do not edit. Sorted by slug for binary search.",
    "",
    "use super::{System, TintedTheme, Variant};",
    "",
    "/// The tinted-theming/schemes commit this catalog was generated from.",
    `pub const SOURCE_COMMIT: &str = "${commit}";`,
    "",
    "pub static THEMES: &[TintedTheme] = &[",
    ...schemes.map(
      (s) =>
        `    TintedTheme { slug: ${rs(s.slug)}, name: ${rs(s.name)}, family: ${rs(s.family)}, variant: ${variantRs(s.variant)}, system: ${systemRs(s.system)}, author: ${rs(s.author)}, canvas: ${rs(s.tokens.canvas)}, ink: ${rs(s.tokens.ink)}, counterpart: ${s.counterpart ? `Some(${rs(s.counterpart)})` : "None"}, swatches: [${s.swatches.map(rs).join(", ")}] },`,
    ),
    "];",
    "",
  ].join("\n"),
);

// ------------------------------------------------------------------ report

const families = new Map();
for (const s of schemes) families.set(s.family, (families.get(s.family) ?? 0) + 1);
const grouped = [...families.values()].filter((n) => n > 1);
console.log(`imported ${schemes.length} themes from tinted-theming/schemes@${short}`);
console.log(`  ${schemes.filter((s) => s.variant === "dark").length} dark, ${schemes.filter((s) => s.variant === "light").length} light`);
console.log(`  ${schemes.filter((s) => s.counterpart).length} have a light/dark counterpart`);
if (process.env.THEMES_PAIRS) for (const s of schemes) if (s.counterpart) console.log(`  ${s.slug} -> ${s.counterpart}`);
console.log(`  ${grouped.length} families covering ${grouped.reduce((a, b) => a + b, 0)} themes; ${families.size - grouped.length} stand alone`);
if (process.env.THEMES_VERBOSE) for (const line of adjusted) console.log(`  adjusted ${line}`);
else console.log(`  ${adjusted.length} schemes adjusted for contrast or variant (THEMES_VERBOSE=1 lists them)`);
for (const line of dropped) console.log(`  dropped ${line}`);
