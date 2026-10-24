import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
  readdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const scripts = resolve("scripts/ci");
const root = mkdtempSync(join(tmpdir(), "y5-release-tests-"));
function run(script, cwd, env = {}) {
  return spawnSync(process.execPath, [join(scripts, script)], {
    cwd,
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
}
try {
  writeFileSync(
    join(root, "package.json"),
    JSON.stringify({ version: "0.1.0" }),
  );
  const metadataEnv = {
    GITHUB_RUN_NUMBER: "42",
    GITHUB_SHA: "1234567890abcdef1234567890abcdef12345678",
    GITHUB_OUTPUT: join(root, "outputs"),
    RUNNER_TEMP: root,
  };
  assert.equal(run("release-metadata.mjs", root, metadataEnv).status, 0);
  const output = readFileSync(metadataEnv.GITHUB_OUTPUT, "utf8");
  assert.match(output, /version=0\.1\.42\n/);
  assert.match(output, /tag=build-42-1234567\n/);
  assert.deepEqual(
    JSON.parse(readFileSync(join(root, "y5-release.json"))),
    { version: "0.1.42" },
  );
  writeFileSync(metadataEnv.GITHUB_OUTPUT, "");
  assert.equal(run("release-metadata.mjs", root, metadataEnv).status, 0);
  assert.equal(
    readFileSync(metadataEnv.GITHUB_OUTPUT, "utf8"),
    output,
    "reruns must use the same tag",
  );
  assert.notEqual(
    run("release-metadata.mjs", root, {
      ...metadataEnv,
      GITHUB_RUN_NUMBER: "invalid",
    }).status,
    0,
  );
  for (const [kind, target, formats] of [
    [
      "windows-x64",
      "x86_64-pc-windows-msvc",
      [
        ["nsis", ".exe"],
        ["msi", ".msi"],
      ],
    ],
    [
      "linux-x64",
      "x86_64-unknown-linux-gnu",
      [
        ["deb", ".deb"],
        ["appimage", ".AppImage"],
      ],
    ],
    [
      "linux-arm64",
      "aarch64-unknown-linux-gnu",
      [
        ["deb", ".deb"],
        ["appimage", ".AppImage"],
      ],
    ],
  ]) {
    const cwd = join(root, kind);
    for (const [folder, ext] of formats) {
      const dir = join(
        cwd,
        "src-tauri/target",
        target,
        "release/bundle",
        folder,
      );
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, `Y5 test${ext}`),
        `test artifact for ${kind}${ext}`,
      );
      writeFileSync(join(dir, `Y5 test${ext}.sig`), "test-signature");
    }
    const env = {
      BUILD_KIND: kind,
      BUILD_TARGET: target,
      RELEASE_VERSION: "0.1.42",
    };
    const result = run("collect-assets.mjs", cwd, env);
    assert.equal(result.status, 0, result.stderr);
    const assets = join(cwd, "release-assets");
    assert.equal(readdirSync(assets).length, 6);
    for (const line of readFileSync(
      join(assets, `SHA256SUMS-${kind}.txt`),
      "utf8",
    )
      .trim()
      .split("\n")) {
      const [expected, name] = line.split("  ");
      assert.equal(
        createHash("sha256")
          .update(readFileSync(join(assets, name)))
          .digest("hex"),
        expected,
      );
    }
    const extra = join(
      cwd,
      "src-tauri/target",
      target,
      "release/bundle",
      formats[0][0],
      `unexpected${formats[0][1]}`,
    );
    writeFileSync(extra, "unexpected");
    assert.notEqual(
      run("collect-assets.mjs", cwd, env).status,
      0,
      "ambiguous bundles must fail closed",
    );
  }
  console.log(
    "Release helper checks passed: automatic versions/tags, reruns, all non-macOS artifact names, checksums, invalid inputs, and ambiguous bundles.",
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
