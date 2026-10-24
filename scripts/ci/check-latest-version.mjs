import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { createManifest } from "./latest-version.mjs";

const meta = {
  version: "0.1.42",
  tag: "build-42-1234567",
  repository: "owner/sentinel",
  sha: "1234567" + "a".repeat(33),
};
function fixture(fn) {
  const dir = mkdtempSync(join(tmpdir(), "sentinel-manifest-"));
  try {
    for (const [kind, formats] of Object.entries({
      "linux-x64": ["deb", "appimage"],
      "linux-arm64": ["deb", "appimage"],
    })) {
      const binaries = formats.map((format) => {
        const filename = `${kind}.${format}`,
          bytes = Buffer.from(filename);
        writeFileSync(join(dir, filename), bytes);
        writeFileSync(join(dir, `${filename}.sig`), "signature");
        return {
          filename,
          format,
          signature: "signature",
          size: bytes.length,
          sha256: createHash("sha256").update(bytes).digest("hex"),
        };
      });
      writeFileSync(
        join(dir, `assets-${kind}.json`),
        JSON.stringify({ version: meta.version, kind, binaries }),
      );
    }
    fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
test("all four Linux binaries and architecture/installer-specific updater entries", () =>
  fixture((dir) => {
    const m = createManifest(dir, meta);
    assert.equal(m.assets.length, 4);
    assert.ok(Object.keys(m.platforms).every((key) => key.startsWith("linux-")));
    assert.equal(m.platforms["linux-x86_64-deb"].format, "deb");
    assert.equal(m.platforms["linux-aarch64"].format, "appimage");
    assert.equal(m.platforms["linux-aarch64-deb"].format, "deb");
    assert.equal(m.platforms["linux-x86_64"].format, "appimage");
    assert.ok(
      m.assets.every((a) => a.url.includes(`/releases/download/${meta.tag}/`)),
    );
  }));
test("reject corrupted downloads", () =>
  fixture((dir) => {
    writeFileSync(join(dir, "linux-x64.appimage"), "corrupt");
    assert.throws(() => createManifest(dir, meta), /Checksum/);
  }));
test("reject missing platforms and signatures", () =>
  fixture((dir) => {
    rmSync(join(dir, "linux-arm64.deb.sig"));
    assert.throws(() => createManifest(dir, meta));
    rmSync(join(dir, "assets-linux-arm64.json"));
    assert.throws(() => createManifest(dir, meta));
  }));
test("reject mixed versions and duplicate formats", () =>
  fixture((dir) => {
    const file = join(dir, "assets-linux-x64.json");
    const inventory = JSON.parse(readFileSync(file));
    inventory.version = "0.1.41";
    writeFileSync(file, JSON.stringify(inventory));
    assert.throws(() => createManifest(dir, meta), /mixed/);
    inventory.version = meta.version;
    inventory.binaries[1].format = "deb";
    writeFileSync(file, JSON.stringify(inventory));
    assert.throws(() => createManifest(dir, meta), /duplicate/);
  }));

test("reject unexpected platform inventories in a Linux-only release", () =>
  fixture((dir) => {
    writeFileSync(join(dir, "assets-windows-x64.json"), "{}");
    assert.throws(() => createManifest(dir, meta), /Unexpected platform inventories/);
  }));
