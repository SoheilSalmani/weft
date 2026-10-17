//! Weft Hub client: `hub:owner/name[@version]` refs resolve through a
//! sha256-verified local cache (`~/.weft/hub/…`); immutability makes cache
//! hits skip the network entirely. Publishing tars the template directory
//! and PUTs it. The registry URL comes from `--registry` or `WEFT_HUB_URL`.
//! Include resolution (lockfile + cache) lives in [`crate::source`].

use std::io::{Read, Write};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};
use weft_engine::report;

use crate::ui::{self, palette, Out};

/// A parsed `hub:owner/name[@version]` ref.
#[derive(Debug, Clone, PartialEq)]
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

fn call(url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    ureq::get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("GET {url}: {e}"))
}

/// GET a registry document, under a spinner saying what it is for.
fn get(url: &str, label: String) -> Result<Vec<u8>> {
    let _task = ui::Task::spinner(label);
    let mut response = call(url)?;
    let mut bytes = Vec::new();
    response.body_mut().as_reader().read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// GET a template archive, counting its bytes against `Content-Length`.
fn download(url: &str, label: String) -> Result<Vec<u8>> {
    /// Bytes read between bar updates.
    const CHUNK: u64 = 64 * 1024;
    /// Most a declared length may preallocate; the header is the server's word.
    const MAX_PREALLOC: u64 = 64 * 1024 * 1024;
    let mut response = {
        let _task = ui::Task::spinner(label.clone());
        call(url)?
    };
    let total = response.body().content_length();
    let task = ui::Task::bytes(label, total);
    let capacity = total.map_or(0, |n| n.min(MAX_PREALLOC));
    let mut bytes = Vec::with_capacity(usize::try_from(capacity).unwrap_or(0));
    let mut reader = response.body_mut().as_reader();
    loop {
        let n = reader.by_ref().take(CHUNK).read_to_end(&mut bytes)?;
        if n == 0 {
            break;
        }
        task.inc(n as u64);
    }
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
    let bytes = get(&url, format!("resolving hub:{owner}/{name}"))
        .with_context(|| format!("resolving hub:{owner}/{name}"))?;
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
    let bytes = get(&url, format!("resolving hub:{}/{}", r.owner, r.name))
        .with_context(|| format!("resolving {r}"))?;
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
    let bytes = download(
        &url,
        format!("downloading hub:{}/{}@{version}", r.owner, r.name),
    )
    .with_context(|| format!("downloading {r}@{version}"))?;
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
    report::done(format_args!(
        "downloaded hub:{}/{}@{version} ({} bytes, sha256 verified)",
        r.owner,
        r.name,
        bytes.len()
    ));
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
    let response = {
        let _task = ui::Task::spinner(format!("publishing {owner}/{name}@{version}"));
        ureq::put(&url)
            .header("authorization", &format!("Bearer {token}"))
            .header("content-type", "application/gzip")
            .send(&body[..])
    };
    match response {
        Ok(_) => {
            report::done(format_args!(
                "published {owner}/{name}@{version} ({} bytes)",
                body.len()
            ));
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
    let bytes = get(
        &format!("{registry}/api/v1/search?q={query}"),
        format!("searching hub for {query}"),
    )?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    let results = doc["results"].as_array().cloned().unwrap_or_default();
    if results.is_empty() {
        report::info(format_args!("no templates matched `{query}`"));
        return Ok(());
    }
    let field = |row: &serde_json::Value, key: &str| row[key].as_str().unwrap_or("").to_owned();
    let rows = results.iter().map(|row| {
        let desc = field(row, "description");
        let summary = desc.lines().next().unwrap_or("").to_owned();
        (
            format!("{}/{}", field(row, "owner"), field(row, "name")),
            field(row, "latest"),
            summary,
        )
    });
    if ui::Table::fits() {
        let mut table = ui::Table::new(&["TEMPLATE", "LATEST", "DESCRIPTION"]);
        for (template, latest, desc) in rows {
            table.row([
                ui::cell(template).accent(),
                ui::cell(latest),
                ui::cell(desc),
            ]);
        }
        table.print();
    } else {
        for (template, latest, desc) in rows {
            ui::outln!("{template}@{latest}  {desc}");
        }
    }
    Ok(())
}

/// `weft hub info`.
pub fn info(registry: &str, spec: &str) -> Result<()> {
    let (owner, name) = spec.split_once('/').context("expected owner/name")?;
    let bytes = get(
        &format!("{registry}/api/v1/index/{owner}/{name}"),
        format!("resolving hub:{owner}/{name}"),
    )?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    let versions = doc["versions"].as_array().cloned().unwrap_or_default();
    let rows = versions.iter().map(|v| {
        (
            v["version"].as_str().unwrap_or(""),
            v["published_at"].as_str().unwrap_or(""),
            v["yanked"].as_bool().unwrap_or(false),
        )
    });
    if ui::Table::fits() {
        let heading = format!("{owner}/{name}");
        ui::outln!("{}", ui::paint(Out::Stdout, palette::bold(), heading));
        let any_yanked = rows.clone().any(|(_, _, yanked)| yanked);
        let headers: &[&str] = if any_yanked {
            &["VERSION", "PUBLISHED", "STATUS"]
        } else {
            &["VERSION", "PUBLISHED"]
        };
        let mut table = ui::Table::new(headers);
        for (version, published, yanked) in rows {
            let mut cells = vec![ui::cell(version), ui::cell(published).dim()];
            if any_yanked {
                cells.push(ui::cell(if yanked { "yanked" } else { "" }).warning());
            }
            table.row(cells);
        }
        table.print();
    } else {
        ui::outln!("{owner}/{name}");
        for (version, published, yanked) in rows {
            let yanked = if yanked { "(yanked)" } else { "" };
            ui::outln!("  {version:12} {published}  {yanked}");
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
