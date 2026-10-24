import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

export function createManifest(
  dir,
  { version, tag, repository, sha, date = new Date().toISOString() },
) {
  if (
    !/^\d+\.\d+\.\d+$/.test(version ?? "") ||
    !/^build-\d+-[a-f0-9]{7}$/.test(tag ?? "") ||
    !/^[\w.-]+\/[\w.-]+$/.test(repository ?? "") ||
    !/^[a-f0-9]{40}$/.test(sha ?? "")
  )
    throw new Error("Invalid release metadata");
  const base = `https://github.com/${repository}/releases/download/${tag}/`;
  const expected = {
    "linux-x64": ["deb", "appimage"],
    "linux-arm64": ["deb", "appimage"],
  };
  const platforms = {};
  const assets = [];
  const names = new Set();
  for (const [kind, formats] of Object.entries(expected)) {
    const inventory = JSON.parse(
      readFileSync(join(dir, `assets-${kind}.json`), "utf8"),
    );
    if (
      inventory.kind !== kind ||
      inventory.version !== version ||
      inventory.binaries.length !== formats.length
    )
      throw new Error(`Incomplete/mixed release: ${kind}`);
    for (const format of formats) {
      const matches = inventory.binaries.filter((a) => a.format === format);
      if (matches.length !== 1)
        throw new Error(`Missing or duplicate format ${kind}/${format}`);
      const asset = matches[0];
      if (!/^[\w.-]+$/.test(asset.filename) || names.has(asset.filename))
        throw new Error("Invalid/duplicate asset name");
      names.add(asset.filename);
      const bytes = readFileSync(join(dir, asset.filename));
      if (
        asset.size !== bytes.length ||
        asset.sha256 !== createHash("sha256").update(bytes).digest("hex")
      )
        throw new Error(`Checksum/size mismatch: ${asset.filename}`);
      const entry = {
        ...asset,
        kind,
        url: base + encodeURIComponent(asset.filename),
      };
      assets.push(entry);
      const signature = readFileSync(
        join(dir, `${asset.filename}.sig`),
        "utf8",
      ).trim();
      if (!signature || signature !== asset.signature)
        throw new Error(`Missing/mismatched signature: ${asset.filename}`);
      const targets = kind === "linux-x64" ? ["linux-x86_64"] : ["linux-aarch64"];
      for (const target of targets) {
        platforms[`${target}-${format}`] = entry;
        if (format === "appimage")
          platforms[target] = entry;
      }
    }
  }
  const inventoryFiles = readdirSync(dir).filter((n) =>
    /^assets-.*\.json$/.test(n),
  );
  if (inventoryFiles.length !== Object.keys(expected).length ||
      inventoryFiles.some((name) => !Object.keys(expected).some((kind) => name === `assets-${kind}.json`)))
    throw new Error("Unexpected platform inventories");
  return {
    schema_version: 1,
    version,
    tag,
    commit: sha,
    pub_date: date,
    notes: `Y5 ${version}. Includes the latest fixes and improvements.`,
    release_url: `https://github.com/${repository}/releases/tag/${tag}`,
    platforms,
    assets,
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const manifest = createManifest("release-assets", {
    version: process.env.RELEASE_VERSION,
    tag: process.env.RELEASE_TAG,
    repository: process.env.GITHUB_REPOSITORY,
    sha: process.env.GITHUB_SHA,
  });
  writeFileSync(
    "release-assets/latest_version.json",
    JSON.stringify(manifest, null, 2) + "\n",
  );
  console.log(
    `Generated latest_version.json with ${manifest.assets.length} binaries.`,
  );
}
