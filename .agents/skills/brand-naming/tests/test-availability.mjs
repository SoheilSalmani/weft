#!/usr/bin/env node
// Offline tests for availability.mjs.
//
// Deliberately no live lookups: a suite that asserts "example.com is free"
// fails the day someone registers it. Validation short-circuits before any
// network call, so these cases are deterministic. Live behaviour is exercised
// by running the script directly.
//
// Run: node tests/test-availability.mjs

import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);
const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "..", "scripts", "availability.mjs");

let pass = 0, fail = 0;
const ok = (cond, label) => cond ? (pass++, console.log(`  ok   ${label}`))
                                 : (fail++, console.log(`  FAIL ${label}`));

async function json(args) {
  const { stdout } = await run("node", [SCRIPT, ...args, "--json"]);
  return JSON.parse(stdout);
}

console.log("validation, no network required:");

for (const [name, why] of [
  ["hello world", "spaces"],
  ["foo.bar", "dots"],
  ["-lead", "leading hyphen"],
  ["trail-", "trailing hyphen"],
  ["a".repeat(64), "over 63 characters"],
]) {
  const r = await json([name, "--tlds", "com", "--registries", "npm"]);
  ok(r.domains[0].state === "invalid", `rejects ${why}: ${JSON.stringify(name.slice(0, 20))}`);
  ok(r.registries.npm.state === "invalid", `registry also rejects ${why}`);
}

// The homograph case. A Cyrillic "а" looks identical to Latin "a"; reporting it
// as free would be the most dangerous possible false positive.
const homo = await json(["аpple", "--tlds", "com"]);
ok(homo.domains[0].state === "invalid", "rejects non-ASCII homograph");
ok(/homograph/i.test(homo.domains[0].detail), "explains the homograph risk");

console.log("\nstates are never booleans:");
const r = await json(["hello world", "--tlds", "com"]);
ok(!["true", "false", true, false].includes(r.domains[0].state), "state is a string, not a boolean");
ok(r.checked_at && !Number.isNaN(Date.parse(r.checked_at)), "report carries an ISO timestamp");

console.log("\nusage:");
try {
  await run("node", [SCRIPT]);
  ok(false, "exits non-zero with no name");
} catch (e) {
  ok(e.code === 1, "exits 1 with no name");
  ok(/name is required/.test(e.stderr), "explains why");
}

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
