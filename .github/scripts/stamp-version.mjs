// Stamp the release version into the source files FROM the git tag, at CI build time.
//
// The bundle/installer version comes from src-tauri/tauri.conf.json (Tauri's bundler) and
// src-tauri/Cargo.toml — NOT from the tag. Running this in CI before the build makes the tag
// the single source of truth: `git tag v0.2.3 && git push` is enough, no manual version bump.
//
// Reads the tag from $GITHUB_REF_NAME (e.g. "v0.2.3"); no-ops if it isn't a vX.Y.Z tag (so
// branch / workflow_dispatch runs keep the committed version). Cargo.lock is left to `cargo`
// to refresh during the build (no --locked is used).
import { readFileSync, writeFileSync } from "node:fs";

const ref = process.env.GITHUB_REF_NAME ?? "";
const version = ref.replace(/^v/, "");

if (!/^\d+\.\d+\.\d+/.test(version)) {
  console.log(`"${ref}" is not a vX.Y.Z tag — leaving version files untouched.`);
  process.exit(0);
}

const edits = [
  ["package.json", /("version":\s*")[^"]+(")/],
  ["src-tauri/tauri.conf.json", /("version":\s*")[^"]+(")/],
  ["src-tauri/Cargo.toml", /^(version = ")[^"]+(")/m],
];

for (const [file, re] of edits) {
  const before = readFileSync(file, "utf8");
  const after = before.replace(re, `$1${version}$2`);
  if (after === before) {
    throw new Error(`Could not stamp version in ${file} (pattern not found)`);
  }
  writeFileSync(file, after);
}

console.log(`Stamped version ${version} into ${edits.length} files.`);
