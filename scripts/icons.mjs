// Generate browser and installable-app icons from the original Rbit mascot.
// Commit the outputs so cargo build never requires Node. Run: npm run icons
import { readFileSync, writeFileSync } from "node:fs";
import { Resvg } from "@resvg/resvg-js";

const assets = new URL("../crates/rbit-web/assets/", import.meta.url);
const mascot = readFileSync(new URL("logo.png", assets)).toString("base64");

function render(size, file, scale = 1, background = false) {
  const edge = 512 * scale;
  const inset = (512 - edge) / 2;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 512 512">
    ${background ? '<rect width="512" height="512" fill="#f5f2eb"/>' : ""}
    <image x="${inset}" y="${inset}" width="${edge}" height="${edge}" preserveAspectRatio="xMidYMid meet" xlink:href="data:image/png;base64,${mascot}"/>
  </svg>`;
  const png = new Resvg(svg, { fitTo: { mode: "width", value: size } }).render().asPng();
  writeFileSync(new URL(file, assets), png);
  console.log(`${file}: ${size}x${size}, ${png.length} bytes`);
}

render(32, "icon-32.png");
render(192, "icon-192.png");
render(512, "icon-512.png");
// A square at 56% width fits entirely inside the maskable 40%-radius safe circle.
render(512, "icon-maskable-512.png", 0.56, true);
// iOS supplies its own rounded mask and needs an opaque background.
render(180, "apple-touch-icon.png", 0.85, true);
