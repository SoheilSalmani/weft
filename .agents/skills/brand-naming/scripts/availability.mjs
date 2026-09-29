#!/usr/bin/env node
// Deterministic availability screening for naming candidates.
//
// Gathers evidence. It does not make the availability judgement, and it never
// registers, purchases or publishes anything.
//
// Every endpoint here was verified live on 2026-08-14. The three behaviours
// that are easy to get wrong, and that a guessed implementation gets wrong:
//   - RDAP servers must be resolved through the IANA bootstrap. A hand-written
//     endpoint returns a connection error, which is not "available".
//   - crates.io returns 403 to every request without a User-Agent, including
//     for names that are free. Read as "taken" that would be a false positive.
//   - PyPI and crates.io normalise separators and case (PEP 503), so
//     Flask-SQLAlchemy, flask_sqlalchemy and FLASK-SQLALCHEMY are one package.
//
// Usage:
//   node availability.mjs <name> [--tlds com,dev,io] [--registries npm,pypi,crates] [--json]
//
// Exit codes: 0 checks ran (whatever they found), 1 usage error.

const UA = "brand-naming-skill/1.0 (+availability screening)";
const TIMEOUT_MS = 12000;
const BOOTSTRAP = "https://data.iana.org/rdap/dns.json";

// States are deliberately not booleans. "unknown" is a real answer and must
// never be collapsed into "available".
const TAKEN = "taken";
const FREE = "free";
const UNKNOWN = "unknown";
const INVALID = "invalid";

// One in-flight request at a time, with a small gap. These are public services
// screening a handful of candidates; hammering them is both rude and a fast
// route to a 429 that would be misread as a result.
const GAP_MS = 150;
let chain = Promise.resolve();
const cache = new Map(); // per-run only. Availability is never cached across runs.

/**
 * A candidate must be a valid DNS label before any of this means anything.
 * Without this, "hello world" was reported as a free domain and a Cyrillic
 * homograph of a famous name was reported as available.
 */
function validateLabel(name) {
  if (!name || name.length > 63) return "must be 1 to 63 characters";
  // eslint-disable-next-line no-control-regex
  if (/[^\x00-\x7F]/.test(name)) {
    return "contains non-ASCII characters. Internationalised domains must be " +
      "punycode-encoded, and non-ASCII lookalikes of existing brands are a " +
      "homograph risk that this tool will not screen for you";
  }
  if (!/^[a-zA-Z0-9-]+$/.test(name)) return "may contain only letters, digits and hyphens";
  if (name.startsWith("-") || name.endsWith("-")) return "may not start or end with a hyphen";
  return null;
}

async function get(url, { headers = {} } = {}) {
  if (cache.has(url)) return cache.get(url);
  const run = chain.then(async () => {
    await new Promise((r) => setTimeout(r, GAP_MS));
    return doGet(url, headers);
  });
  chain = run.catch(() => {});
  const result = await run;
  cache.set(url, result);
  return result;
}

async function doGet(url, headers) {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), TIMEOUT_MS);
  try {
    const res = await fetch(url, {
      signal: ctl.signal,
      headers: { "user-agent": UA, accept: "application/json", ...headers },
      redirect: "follow",
    });
    return { status: res.status, ok: res.ok, res };
  } catch (err) {
    return { status: 0, ok: false, error: String(err.message || err) };
  } finally {
    clearTimeout(t);
  }
}

// ------------------------------------------------------------------ domains

let bootstrapCache = null;

async function rdapServerFor(tld) {
  if (!bootstrapCache) {
    const r = await get(BOOTSTRAP);
    if (!r.ok) return { error: `bootstrap unreachable (${r.status || r.error})` };
    bootstrapCache = await r.res.json();
  }
  for (const [tlds, servers] of bootstrapCache.services) {
    if (tlds.includes(tld)) return { server: servers[0].replace(/\/$/, "") };
  }
  return { error: `no RDAP server published for .${tld}` };
}

async function checkDomain(name, tld) {
  const domain = `${name}.${tld}`;
  const bad = validateLabel(name);
  if (bad) return { domain, state: INVALID, detail: bad };
  const { server, error } = await rdapServerFor(tld);
  if (error) return { domain, state: UNKNOWN, detail: error };

  const r = await get(`${server}/domain/${encodeURIComponent(domain)}`);
  if (r.status === 200) {
    let events = [];
    try {
      const body = await r.res.json();
      events = (body.events || []).map((e) => `${e.eventAction}:${(e.eventDate || "").slice(0, 10)}`);
    } catch { /* body shape varies by registry; status is the signal */ }
    return { domain, state: TAKEN, detail: events.join(" ") || "registered" };
  }
  if (r.status === 404) {
    // 404 means no registration record. Registry policy can still withhold the
    // name (reserved, premium, blocked), so this is necessary, not sufficient.
    return { domain, state: FREE, detail: "no registration record; confirm with a registrar for reserved or premium status" };
  }
  if (r.status === 429) return { domain, state: UNKNOWN, detail: "rate limited" };
  return { domain, state: UNKNOWN, detail: `RDAP returned ${r.status || r.error}` };
}

// ----------------------------------------------------------------- registries

/** PEP 503 normalisation. PyPI and crates.io both collapse separators. */
function normalizePython(n) {
  return n.toLowerCase().replace(/[-_.]+/g, "-");
}

const REGISTRIES = {
  npm: async (name) => {
    const lower = name.toLowerCase();
    const r = await get(`https://registry.npmjs.org/${encodeURIComponent(lower)}`);
    const note = lower !== name ? `npm requires lowercase; checked "${lower}"` : "";
    if (r.status === 200) return { state: TAKEN, detail: note || "published" };
    if (r.status === 404) return { state: FREE, detail: note || "no package" };
    return { state: UNKNOWN, detail: `HTTP ${r.status || r.error}` };
  },
  pypi: async (name) => {
    const norm = normalizePython(name);
    const r = await get(`https://pypi.org/pypi/${encodeURIComponent(norm)}/json`);
    const note = norm !== name ? `normalised to "${norm}" (PEP 503)` : "";
    if (r.status === 200) return { state: TAKEN, detail: note || "published" };
    if (r.status === 404) return { state: FREE, detail: note || "no project" };
    return { state: UNKNOWN, detail: `HTTP ${r.status || r.error}` };
  },
  crates: async (name) => {
    const norm = normalizePython(name);
    // A missing User-Agent yields 403 for every name, free or taken.
    const r = await get(`https://crates.io/api/v1/crates/${encodeURIComponent(norm)}`);
    if (r.status === 200) return { state: TAKEN, detail: "published" };
    if (r.status === 404) return { state: FREE, detail: "no crate" };
    if (r.status === 403) return { state: UNKNOWN, detail: "403; crates.io requires a User-Agent" };
    return { state: UNKNOWN, detail: `HTTP ${r.status || r.error}` };
  },
};

// ----------------------------------------------------------------------- cli

function usage(msg) {
  if (msg) console.error(`error: ${msg}\n`);
  console.error(`Screen a naming candidate for domain and registry availability.

  node availability.mjs <name> [options]

    --tlds com,dev,io          domains to check (default: com,dev,io)
    --registries npm,pypi      registries to check (default: none)
                               available: ${Object.keys(REGISTRIES).join(", ")}
    --json                     machine-readable output

Reports taken | free | unknown. "unknown" is never "free".
Gathers evidence only: it registers, purchases and publishes nothing.`);
  process.exit(msg ? 1 : 0);
}

function arg(flag, fallback) {
  const i = process.argv.indexOf(flag);
  return i === -1 ? fallback : process.argv[i + 1];
}

const name = process.argv[2];
if (!name || name.startsWith("--")) usage(name ? null : "a name is required");

const tlds = (arg("--tlds", "com,dev,io") || "").split(",").filter(Boolean);
const regs = (arg("--registries", "") || "").split(",").filter(Boolean);

for (const r of regs) if (!REGISTRIES[r]) usage(`unknown registry "${r}"`);

const report = {
  name,
  checked_at: new Date().toISOString(),
  domains: [],
  registries: {},
};

for (const tld of tlds) report.domains.push(await checkDomain(name, tld));
const bad = validateLabel(name);
for (const r of regs) {
  report.registries[r] = bad
    ? { state: INVALID, detail: bad }
    : await REGISTRIES[r](name);
}

if (process.argv.includes("--json")) {
  console.log(JSON.stringify(report, null, 2));
} else {
  console.log(`\n${name}   checked ${report.checked_at}\n`);
  for (const d of report.domains) {
    console.log(`  ${d.state.padEnd(7)} ${d.domain.padEnd(28)} ${d.detail}`);
  }
  for (const [r, v] of Object.entries(report.registries)) {
    console.log(`  ${v.state.padEnd(7)} ${r.padEnd(28)} ${v.detail}`);
  }
  if (report.domains.some((d) => d.state === INVALID) ||
      Object.values(report.registries).some((v) => v.state === INVALID)) {
    console.log(`\n  "invalid" means the candidate cannot be a domain label as written.`);
  }
  console.log(`\n  "free" means no registration record was found, not that the name is clear.`);
  console.log(`  Trademark, famous-brand and search-ownership screening are separate.\n`);
}
