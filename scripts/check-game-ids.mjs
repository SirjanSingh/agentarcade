#!/usr/bin/env node
/**
 * The game list lives in two places: src/registry.ts (which maps ids to the
 * modules that draw them) and src-tauri/src/logic/games.rs (which the picker
 * chooses from). Rust cannot read the TypeScript without a build step nobody
 * wants for a list that changes a few times a year, so instead of generating
 * one from the other we make drift fail here — and in CI.
 *
 * Adding a game is: a file in src/games/, a line in REGISTRY, an id in GAME_IDS.
 * Miss the third and this is what tells you.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const TS_FILE = "src/registry.ts";
const RS_FILE = "src-tauri/src/logic/games.rs";

function read(relative) {
  try {
    return readFileSync(join(root, relative), "utf8");
  } catch (err) {
    fail(`could not read ${relative}: ${err.message}`);
  }
}

function fail(message) {
  console.error(`check-game-ids: ${message}`);
  process.exit(1);
}

/** The keys of the REGISTRY object literal. */
function typescriptIds(source) {
  const block = source.match(/export const REGISTRY[^=]*=\s*\{([\s\S]*?)\n\};/);
  if (!block) fail(`could not find the REGISTRY object literal in ${TS_FILE}`);
  return [...block[1].matchAll(/^\s*([A-Za-z0-9_]+)\s*:/gm)].map((m) => m[1]);
}

/** The string literals in the GAME_IDS slice. */
function rustIds(source) {
  const block = source.match(/pub const GAME_IDS:[^=]*=\s*&\[([\s\S]*?)\];/);
  if (!block) fail(`could not find the GAME_IDS slice in ${RS_FILE}`);
  return [...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

function duplicates(ids) {
  const seen = new Set();
  return ids.filter((id) => (seen.has(id) ? true : (seen.add(id), false)));
}

const ts = typescriptIds(read(TS_FILE));
const rs = rustIds(read(RS_FILE));

if (ts.length === 0) fail(`${TS_FILE} lists no games`);

for (const [label, ids] of [
  [TS_FILE, ts],
  [RS_FILE, rs],
]) {
  const dupes = duplicates(ids);
  if (dupes.length) fail(`${label} lists ${dupes.join(", ")} more than once`);
}

const missingInRust = ts.filter((id) => !rs.includes(id));
const missingInTs = rs.filter((id) => !ts.includes(id));

if (missingInRust.length || missingInTs.length) {
  const lines = ["the game lists have drifted apart."];
  if (missingInRust.length) {
    lines.push(`  in ${TS_FILE} but not ${RS_FILE}: ${missingInRust.join(", ")}`);
  }
  if (missingInTs.length) {
    lines.push(`  in ${RS_FILE} but not ${TS_FILE}: ${missingInTs.join(", ")}`);
  }
  fail(lines.join("\n"));
}

console.log(`check-game-ids: ${ts.length} games, both lists agree — ${ts.join(", ")}`);
