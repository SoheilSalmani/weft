//! Weft Hub client: `hub:owner/name[@version]` refs resolve through a
//! sha256-verified local cache (`~/.weft/hub/…`); immutability makes cache
//! hits skip the network entirely. Publishing tars the template directory
//! and PUTs it. The registry URL comes from `--registry` or `WEFT_HUB_URL`.

use std::io::Write;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

/// A parsed `hub:owner/name[@version]` ref.
#[derive(Debug, PartialEq)]
pub struct HubRef {
    pub owner: String,
    pub name: String,
    pub version: Option<String>,
}

impl std::fmt::Display for HubRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hub:{}/{}", self.owner, self.name)?;
        if let Some(v) = &self.version {
            write!(f, "@{v}")?;
        }
        Ok(())
    }
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
}

/// Parse a hub ref; `None` when the string isn't one.
pub fn parse_ref(s: &str) -> Option<Result<HubRef>> {
    let rest = s.strip_prefix("hub:")?;
    Some((|| {
        let (path, version) = match rest.split_once('@') {
            Some((p, v)) => (p, Some(v.to_owned())),
            None => (rest, None),
        };
        let (owner, name) = path
            .split_once('/')
            .with_context(|| format!("hub refs are `hub:owner/name[@version]`, got `{s}`"))?;
        if !valid_slug(owner) || !valid_slug(name) {
            bail!("owner and name must be slugs ([a-z0-9_-]+) in `{s}`");
        }
        if let Some(v) = &version {
            semver::Version::parse(v).with_context(|| format!("`{v}` is not semver in `{s}`"))?;
        }
        Ok(HubRef {
            owner: owner.to_owned(),
            name: name.to_owned(),
            version,
        })
    })())
}

/// The registry base URL: `--registry` flag beats `WEFT_HUB_URL`.
pub fn registry_url(flag: Option<&str>) -> Result<String> {
    flag.map(str::to_owned)
        .or_else(|| std::env::var("WEFT_HUB_URL").ok())
        .map(|u| u.trim_end_matches('/').to_owned())
        .context("no registry configured; set WEFT_HUB_URL or pass --registry")
}

pub fn cache_root() -> Result<Utf8PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(Utf8PathBuf::from(home).join(".weft").join("hub"))
}

#[derive(serde::Deserialize)]
struct IndexDoc {
    versions: Vec<IndexVersion>,
}

#[derive(serde::Deserialize)]
struct IndexVersion {
    version: String,
    sha256: String,
    #[serde(default)]
    yanked: bool,
}

fn get(url: &str) -> Result<Vec<u8>> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("GET {url}: {e}"))?;
    let mut bytes = Vec::new();
    std::io::copy(
        &mut response.body_mut().as_reader(),
        &mut std::io::Cursor::new(&mut bytes).get_mut(),
    )?;
    Ok(bytes)
}

/// Highest non-yanked version satisfying `req` → (version, sha256).
pub fn resolve_req(
    registry: &str,
    owner: &str,
    name: &str,
    req: &semver::VersionReq,
) -> Result<(String, String)> {
    let url = format!("{registry}/api/v1/index/{owner}/{name}");
    let bytes = get(&url).with_context(|| format!("resolving hub:{owner}/{name}"))?;
    let index: IndexDoc = serde_json::from_slice(&bytes).context("parsing registry index")?;
    let mut best: Option<(semver::Version, &IndexVersion)> = None;
    for entry in &index.versions {
        if entry.yanked {
            continue;
        }
        let Ok(v) = semver::Version::parse(&entry.version) else {
            continue;
        };
        if req.matches(&v) && best.as_ref().map(|(b, _)| v > *b).unwrap_or(true) {
            best = Some((v, entry));
        }
    }
    let (version, entry) = best
        .with_context(|| format!("hub:{owner}/{name}: no published version satisfies `{req}`"))?;
    Ok((version.to_string(), entry.sha256.clone()))
}

/// Resolve a ref against the registry index → (version, sha256).
pub fn resolve(registry: &str, r: &HubRef) -> Result<(String, String)> {
    let url = format!("{registry}/api/v1/index/{}/{}", r.owner, r.name);
    let bytes = get(&url).with_context(|| format!("resolving {r}"))?;
    let index: IndexDoc = serde_json::from_slice(&bytes).context("parsing registry index")?;
    let entry = match &r.version {
        Some(v) => index
            .versions
            .iter()
            .find(|e| e.version == *v)
            .with_context(|| format!("{r}: version {v} does not exist"))?,
        None => index
            .versions
            .iter()
            .rev()
            .find(|e| !e.yanked)
            .with_context(|| format!("{r}: every version is yanked"))?,
    };
    Ok((entry.version.clone(), entry.sha256.clone()))
}

/// The cached, unpacked template directory for an exact version — download
/// + verify on first use. Returns the directory to treat as the template.
pub fn ensure_cached(
    registry: &str,
    r: &HubRef,
    version: &str,
    sha256: &str,
) -> Result<Utf8PathBuf> {
    let dir = cache_root()?.join(&r.owner).join(&r.name).join(version);
    if dir.join("weft.toml").is_file() {
        return Ok(dir);
    }
    let url = format!(
        "{registry}/api/v1/templates/{}/{}/{version}/download",
        r.owner, r.name
    );
    let bytes = get(&url).with_context(|| format!("downloading {r}@{version}"))?;
    let actual = hex(&Sha256::digest(&bytes));
    if actual != sha256 {
        bail!(
            "sha256 mismatch for {r}@{version}: registry index says {sha256}, \
             downloaded {actual} — refusing to install"
        );
    }
    // Unpack into a temp sibling, then atomically move into place.
    let staging = dir.with_extension("staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let gz = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let rel = entry.path()?.to_string_lossy().to_string();
        if rel.starts_with('/') || rel.split('/').any(|seg| seg == "..") {
            bail!("tarball entry escapes the template root: {rel:?}");
        }
        entry.unpack_in(staging.as_std_path())?;
    }
    if !staging.join("weft.toml").is_file() {
        bail!("downloaded archive is not a weft template (no weft.toml)");
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.parent().expect("cache path has parent"))?;
    std::fs::rename(&staging, &dir).context("moving download into the cache")?;
    eprintln!(
        "downloaded {r}@{version} ({} bytes, sha256 verified)",
        bytes.len()
    );
    Ok(dir)
}

/// Resolve + cache in one step; returns (template dir, resolved ref string).
pub fn fetch(registry: &str, r: &HubRef) -> Result<(Utf8PathBuf, String)> {
    // Exact pinned version already cached → fully offline.
    if let Some(v) = &r.version {
        let dir = cache_root()?.join(&r.owner).join(&r.name).join(v);
        if dir.join("weft.toml").is_file() {
            return Ok((dir, r.to_string()));
        }
    }
    let (version, sha256) = resolve(registry, r)?;
    let dir = ensure_cached(registry, r, &version, &sha256)?;
    Ok((dir, format!("hub:{}/{}@{version}", r.owner, r.name)))
}

/// Build the publish tarball: weft.toml + patches/ + presets/.
pub fn pack(template_dir: &Utf8Path) -> Result<Vec<u8>> {
    if !template_dir.join("weft.toml").is_file() {
        bail!("`{template_dir}` is not a weft template (no weft.toml)");
    }
    let mut builder = tar::Builder::new(Vec::new());
    builder
        .append_path_with_name(template_dir.join("weft.toml"), "weft.toml")
        .context("adding weft.toml")?;
    // The lockfile pins child versions so consumers compose the same tree.
    if template_dir.join(weft_engine::lock::LOCK_FILE).is_file() {
        builder
            .append_path_with_name(
                template_dir.join(weft_engine::lock::LOCK_FILE),
                weft_engine::lock::LOCK_FILE,
            )
            .context("adding weft.lock")?;
    }
    // Ignore rules travel with the template so recordings against a hub
    // copy filter the same junk.
    let ignore_file = weft_engine::weftignore::IGNORE_FILE;
    if template_dir.join(ignore_file).is_file() {
        builder
            .append_path_with_name(template_dir.join(ignore_file), ignore_file)
            .context("adding .weftignore")?;
    }
    for sub in ["patches", "presets"] {
        let dir = template_dir.join(sub);
        if dir.is_dir() {
            builder
                .append_dir_all(sub, &dir)
                .with_context(|| format!("adding {sub}/"))?;
        }
    }
    let tarball = builder.into_inner()?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tarball)?;
    Ok(gz.finish()?)
}

/// `weft hub publish`.
pub fn publish(
    registry: &str,
    token: &str,
    owner: &str,
    name: &str,
    version: &str,
    template_dir: &Utf8Path,
) -> Result<()> {
    let body = pack(template_dir)?;
    let url = format!("{registry}/api/v1/templates/{owner}/{name}/{version}");
    let response = ureq::put(&url)
        .header("authorization", &format!("Bearer {token}"))
        .header("content-type", "application/gzip")
        .send(&body[..]);
    match response {
        Ok(_) => {
            eprintln!("published {owner}/{name}@{version} ({} bytes)", body.len());
            Ok(())
        }
        Err(ureq::Error::StatusCode(code)) => {
            bail!("registry rejected the publish (HTTP {code}); run `weft check` locally and check the version number")
        }
        Err(e) => bail!("publishing to {url}: {e}"),
    }
}

/// `weft hub search`.
pub fn search(registry: &str, query: &str) -> Result<()> {
    let bytes = get(&format!("{registry}/api/v1/search?q={query}"))?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    let results = doc["results"].as_array().cloned().unwrap_or_default();
    if results.is_empty() {
        eprintln!("no templates matched `{query}`");
        return Ok(());
    }
    for row in results {
        println!(
            "{owner}/{name}@{latest}  {desc}",
            owner = row["owner"].as_str().unwrap_or(""),
            name = row["name"].as_str().unwrap_or(""),
            latest = row["latest"].as_str().unwrap_or(""),
            desc = row["description"]
                .as_str()
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or(""),
        );
    }
    Ok(())
}

/// `weft hub info`.
pub fn info(registry: &str, spec: &str) -> Result<()> {
    let (owner, name) = spec.split_once('/').context("expected owner/name")?;
    let bytes = get(&format!("{registry}/api/v1/index/{owner}/{name}"))?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    println!("{owner}/{name}");
    for v in doc["versions"].as_array().cloned().unwrap_or_default() {
        println!(
            "  {version:12} {published}  {yanked}",
            version = v["version"].as_str().unwrap_or(""),
            published = v["published_at"].as_str().unwrap_or(""),
            yanked = if v["yanked"].as_bool().unwrap_or(false) {
                "(yanked)"
            } else {
                ""
            },
        );
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A hub-aware [`IncludeResolver`]: path includes pass through; `hub:`
/// includes resolve their version requirement through each template's own
/// `weft.lock` (loaded lazily per root), download+verify into the cache,
/// and record the resolution back into that root's lock (unless frozen).
pub struct HubResolver {
    registry: Option<String>,
    frozen: bool,
    /// root → (lock, changed). Loaded on first touch, flushed at the end.
    locks: std::collections::BTreeMap<Utf8PathBuf, (weft_engine::lock::Lock, bool)>,
}

impl HubResolver {
    pub fn new(registry: Option<String>, frozen: bool) -> Self {
        Self {
            registry,
            frozen,
            locks: Default::default(),
        }
    }

    fn registry(&self) -> Result<&str> {
        self.registry
            .as_deref()
            .context("this template composes a hub template; set WEFT_HUB_URL or pass --registry")
    }

    /// Forget a template's lock (so `--upgrade` re-resolves its direct
    /// includes to the newest satisfying versions).
    pub fn clear_lock(&mut self, root: &Utf8Path) -> Result<()> {
        let _ = std::fs::remove_file(root.join(weft_engine::lock::LOCK_FILE));
        self.locks
            .insert(root.to_owned(), (weft_engine::lock::Lock::default(), false));
        Ok(())
    }

    /// Write back every lock that changed. Call after loading is done.
    pub fn flush(&self) -> Result<()> {
        for (root, (lock, changed)) in &self.locks {
            if *changed {
                lock.save(root)?;
                eprintln!("updated {}/weft.lock", root);
            }
        }
        Ok(())
    }
}

impl weft_engine::template::IncludeResolver for HubResolver {
    fn resolve(
        &mut self,
        parent_root: &Utf8Path,
        decl: &weft_engine::manifest::IncludeDecl,
    ) -> Result<Utf8PathBuf> {
        let Some(refstr) = decl.hub_ref() else {
            return Ok(parent_root.join(&decl.template));
        };
        let req_str = decl.version.as_deref().with_context(|| {
            format!(
                "include `{}`: hub template `{refstr}` needs a `version`",
                decl.name
            )
        })?;
        let req = semver::VersionReq::parse(req_str).with_context(|| {
            format!(
                "include `{}`: `{req_str}` is not a semver requirement",
                decl.name
            )
        })?;
        let hub = match parse_ref(refstr) {
            Some(r) => r?,
            None => bail!("include `{}`: `{refstr}` is not a hub ref", decl.name),
        };

        // Load this root's lock lazily.
        let (lock, _) = self.locks.entry(parent_root.to_owned()).or_insert_with(|| {
            (
                weft_engine::lock::Lock::load(parent_root).unwrap_or_default(),
                false,
            )
        });

        // A matching, satisfying lock entry → use the pinned version.
        if let Some(locked) = lock.entry(refstr) {
            if let Ok(v) = semver::Version::parse(&locked.version) {
                if req.matches(&v) {
                    let version = locked.version.clone();
                    let sha = locked.sha256.clone();
                    let registry = self.registry()?.to_owned();
                    return ensure_cached(&registry, &hub, &version, &sha);
                }
            }
        }

        // Otherwise resolve fresh (unless frozen, which is for CI).
        if self.frozen {
            bail!(
                "include `{}` ({refstr} {req_str}) is not in weft.lock (or the lock is stale); \
                 run `weft lock` — refusing to resolve under --frozen",
                decl.name
            );
        }
        let registry = self.registry()?.to_owned();
        let (version, sha) = resolve_req(&registry, &hub.owner, &hub.name, &req)?;
        let dir = ensure_cached(&registry, &hub, &version, &sha)?;

        let (lock, changed) = self.locks.get_mut(parent_root).expect("inserted above");
        if lock.upsert(weft_engine::lock::Locked {
            r#ref: refstr.to_owned(),
            req: req_str.to_owned(),
            version,
            sha256: sha,
        }) {
            *changed = true;
        }
        Ok(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_parsing() {
        assert!(parse_ref("local/path").is_none());
        let r = parse_ref("hub:acme/go-service").unwrap().unwrap();
        assert_eq!(
            r,
            HubRef {
                owner: "acme".into(),
                name: "go-service".into(),
                version: None
            }
        );
        let r = parse_ref("hub:acme/go-service@1.2.0").unwrap().unwrap();
        assert_eq!(r.version.as_deref(), Some("1.2.0"));
        assert!(parse_ref("hub:acme").unwrap().is_err());
        assert!(parse_ref("hub:Acme/x").unwrap().is_err());
        assert!(parse_ref("hub:acme/x@banana").unwrap().is_err());
    }
}
