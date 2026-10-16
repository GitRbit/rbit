// Renders the PWA / home-screen PNG icons from crates/rustit-web/assets/icon.svg.
// The PNGs are committed so `cargo build` works without Node; rerun after changing the SVG:
//
//   npm run icons
//
// * icon-192.png, icon-512.png   "any" purpose: the SVG as drawn (rounded square).
// * icon-maskable-512.png        "maskable": full-bleed background, glyph shrunk into the
//                                 central safe zone (a circle of 40 % radius) so any
//                                 platform mask shape keeps it intact.
// * apple-touch-icon.png (180)   full-bleed: iOS applies its own rounded mask and
//                                 renders transparency as black.
import { readFileSync, writeFileSync } from "node:fs";
import { Resvg } from "@resvg/resvg-js";

const assets = "crates/rustit-web/assets";
const source = readFileSync(`${assets}/icon.svg`, "utf8");

const background = source.match(/<rect[^>]*fill="([^"]+)"/)?.[1];
const glyph = source.match(/<g[\s\S]*<\/g>/)?.[0];
if (!background || !glyph) {
  throw new Error("icon.svg must contain a background <rect fill=…> and a <g> glyph");
}

/** A full-bleed square icon with the glyph scaled about the centre. */
function fullBleed(scale) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" fill="${background}"/>
  <g transform="translate(256 256) scale(${scale}) translate(-256 -256)">${glyph}</g>
</svg>`;
}

function render(svg, size, file) {
  const png = new Resvg(svg, { fitTo: { mode: "width", value: size } }).render().asPng();
  writeFileSync(`${assets}/${file}`, png);
  console.log(`${file}: ${size}x${size}, ${png.length} bytes`);
}

render(source, 192, "icon-192.png");
render(source, 512, "icon-512.png");
// The glyph's farthest point is ~180/256 of the half-width from the centre; 0.8 keeps
// it inside the 40 % safe-zone radius (204.8 px of 512) with room to spare.
render(fullBleed(0.8), 512, "icon-maskable-512.png");
render(fullBleed(0.9), 180, "apple-touch-icon.png");
