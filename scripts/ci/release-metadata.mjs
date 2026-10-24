import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { join } from "node:path";

const {
  GITHUB_RUN_NUMBER: run,
  GITHUB_SHA: sha,
  GITHUB_OUTPUT: output,
  RUNNER_TEMP: temp,
} = process.env;
if (
  !/^[1-9]\d*$/.test(run ?? "") ||
  !/^[a-f0-9]{40}$/.test(sha ?? "") ||
  !output ||
  !temp
) {
  throw new Error(
    "Release metadata requires a run number, full commit SHA, GITHUB_OUTPUT and RUNNER_TEMP.",
  );
}
const { version: base } = JSON.parse(readFileSync("package.json", "utf8"));
const [major, minor] = base.split(".");
const version = `${major}.${minor}.${run}`;
const tag = `build-${run}-${sha.slice(0, 7)}`;
const config = join(temp, "y5-release.json");
writeFileSync(config, JSON.stringify({ version }));
appendFileSync(output, `version=${version}\ntag=${tag}\nconfig=${config}\n`);
console.log(`Y5 ${version} (${tag}) from ${sha}`);
