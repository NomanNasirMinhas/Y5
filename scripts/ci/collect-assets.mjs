import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  existsSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { join, resolve, basename } from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";

const {
  BUILD_KIND: kind,
  BUILD_TARGET: target,
  RELEASE_VERSION: version,
} = process.env;
const formats = {
  "macos-universal": [["dmg", ".dmg"]],
  "windows-x64": [
    ["nsis", ".exe"],
    ["msi", ".msi"],
  ],
  "linux-x64": [
    ["deb", ".deb"],
    ["appimage", ".AppImage"],
  ],
  "linux-arm64": [
    ["deb", ".deb"],
    ["appimage", ".AppImage"],
  ],
};
if (
  !formats[kind] ||
  !/^[\w-]+$/.test(target ?? "") ||
  !/^\d+\.\d+\.\d+$/.test(version ?? "")
) {
  throw new Error("Invalid BUILD_KIND, BUILD_TARGET or RELEASE_VERSION.");
}
const bundle = resolve("src-tauri", "target", target, "release", "bundle");
const out = resolve("release-assets");
mkdirSync(out, { recursive: true });
const prefix = `Y5_${version}_${kind}`;
const assets = [];
const binaries = [];
function collect(source, name, format, signed = false) {
  const dest = join(out, name);
  copyFileSync(source, dest);
  assets.push(dest);
  const data = readFileSync(dest);
  const entry = {
    filename: name,
    tauri_filename: basename(source),
    format,
    target,
    size: data.length,
    sha256: createHash("sha256").update(data).digest("hex"),
  };
  if (signed) {
    if (!existsSync(`${source}.sig`))
      throw new Error(`Missing updater signature: ${source}.sig`);
    entry.signature = readFileSync(`${source}.sig`, "utf8").trim();
    if (!entry.signature)
      throw new Error(`Empty updater signature: ${source}.sig`);
    copyFileSync(`${source}.sig`, `${dest}.sig`);
    assets.push(`${dest}.sig`);
  }
  binaries.push(entry);
}
for (const [folder, ext] of formats[kind]) {
  const dir = join(bundle, folder);
  const files = readdirSync(dir).filter((name) => name.endsWith(ext));
  if (files.length !== 1)
    throw new Error(`Expected one ${ext} in ${dir}, found ${files.length}`);
  collect(join(dir, files[0]), `${prefix}${ext}`, folder, folder !== "dmg");
}
if (kind === "macos-universal") {
  const app = join(bundle, "macos", "Y5.app");
  execFileSync("codesign", ["--verify", "--deep", "--strict", app], {
    stdio: "inherit",
  });
  const architectures = execFileSync(
    "lipo",
    ["-archs", join(app, "Contents", "MacOS", "y5")],
    { encoding: "utf8" },
  )
    .trim()
    .split(/\s+/);
  if (!["arm64", "x86_64"].every((arch) => architectures.includes(arch))) {
    throw new Error(`macOS app is not universal: ${architectures.join(", ")}`);
  }
  const zip = join(out, `${prefix}.zip`);
  execFileSync("ditto", [
    "-c",
    "-k",
    "--sequesterRsrc",
    "--keepParent",
    app,
    zip,
  ]);
  assets.push(zip);
  const data = readFileSync(zip);
  binaries.push({
    filename: basename(zip),
    tauri_filename: "Y5.app",
    format: "zip",
    target,
    size: data.length,
    sha256: createHash("sha256").update(data).digest("hex"),
  });
  collect(
    join(bundle, "macos", "Y5.app.tar.gz"),
    `${prefix}.app.tar.gz`,
    "app",
    true,
  );
}
const inventory = join(out, `assets-${kind}.json`);
writeFileSync(
  inventory,
  JSON.stringify({ kind, version, binaries }, null, 2) + "\n",
);
assets.push(inventory);
const sums =
  assets
    .map(
      (file) =>
        `${createHash("sha256").update(readFileSync(file)).digest("hex")}  ${basename(file)}`,
    )
    .join("\n") + "\n";
writeFileSync(join(out, `SHA256SUMS-${kind}.txt`), sums);
console.log(`Prepared ${assets.length} ${kind} assets and checksums.`);
