//! Template sources on the CLI side: parse what the user typed (a path, a
//! `hub:` ref, or a git source), fetch remote ones into the local caches,
//! and resolve `[[include]]`s the same way through each template's
//! `weft.lock`. The engine only ever sees local directories.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_engine::lock::{Lock, Locked};
use weft_engine::manifest::IncludeDecl;
use weft_engine::source::{is_full_commit, GitRef, Kind};
use weft_engine::state::{ProjectTemplate, State, StoredSource};

use crate::{git, hub};

pub enum Source {
    Path(Utf8PathBuf),
    Hub(hub::HubRef),
    Git(GitRef),
}

impl Source {
    pub fn parse(spec: &str) -> Result<Self> {
        Ok(match weft_engine::source::kind(spec) {
            Kind::Hub => Source::Hub(hub::parse_ref(spec).expect("hub: prefix")?),
            Kind::Git => Source::Git(GitRef::parse(spec)?),
            Kind::Path => Source::Path(Utf8PathBuf::from(spec)),
        })
    }

    pub fn is_path(&self) -> bool {
        matches!(self, Source::Path(_))
    }
}

/// Fetch a source into a directory the engine can load. `offline` forbids
/// network access for git sources (hub refs are offline once pinned +
/// cached regardless).
pub fn fetch(source: &Source, offline: bool) -> Result<ProjectTemplate> {
    match source {
        Source::Path(path) => Ok(ProjectTemplate {
            dir: path.clone(),
            stored: None,
        }),
        Source::Hub(r) => {
            let registry = hub::registry_url(None)?;
            let (dir, resolved) = hub::fetch(&registry, r)?;
            Ok(ProjectTemplate {
                dir,
                stored: Some(StoredSource {
                    template: resolved,
                    commit: None,
                }),
            })
        }
        Source::Git(r) => fetch_git(r, offline),
    }
}

fn fetch_git(r: &GitRef, offline: bool) -> Result<ProjectTemplate> {
    // A full commit that is already exported needs neither network nor
    // mirror — the pinned-and-cached case.
    if let Some(rev) = &r.rev {
        if is_full_commit(rev) {
            let dir = git::checkout_dir(r, rev)?;
            if dir.is_dir() {
                return git_template(r, rev, &dir);
            }
        }
    }
    if !offline {
        git::fetch(r)?;
    }
    let sha = git::resolve_rev(r, r.rev.as_deref())?;
    let export = git::export(r, &sha, offline)?;
    git_template(r, &sha, &export)
}

fn git_template(r: &GitRef, sha: &str, export: &Utf8Path) -> Result<ProjectTemplate> {
    let dir = git::template_dir(r, export)?;
    Ok(ProjectTemplate {
        dir,
        stored: Some(StoredSource {
            template: r.to_string(),
            commit: Some(sha.to_owned()),
        }),
    })
}

/// Where a scaffolded project's template is: remote sources are re-fetched
/// (moved to `to` when given — a git rev or a hub version); local paths
/// pass through.
pub fn locate_project(state: &State, to: Option<&str>, offline: bool) -> Result<ProjectTemplate> {
    let spec = &state.state.template;
    match (Source::parse(spec)?, to) {
        (Source::Path(dir), None) => Ok(ProjectTemplate { dir, stored: None }),
        (Source::Path(_), Some(_)) => bail!(
            "--to only applies to projects scaffolded from a remote template; this one \
             uses the local path `{spec}` (pass --template DIR to switch it)"
        ),
        (Source::Hub(r), to) => {
            let target = hub::HubRef {
                version: to.map(str::to_owned).or(r.version.clone()),
                ..r.clone()
            };
            let located = fetch(&Source::Hub(target), offline)?;
            if to.is_none() {
                hub_newer_note(&r);
            }
            Ok(located)
        }
        (Source::Git(r), to) => {
            let target = match to {
                Some(rev) => r.with_rev(Some(rev.to_owned())),
                None => r,
            };
            fetch(&Source::Git(target), offline)
        }
    }
}

/// Mention a newer registry version when the project pins an older one.
fn hub_newer_note(r: &hub::HubRef) {
    let Some(pinned) = &r.version else {
        return;
    };
    let Ok(registry) = hub::registry_url(None) else {
        return;
    };
    let unpinned = hub::HubRef {
        version: None,
        ..r.clone()
    };
    if let Ok((latest, _)) = hub::resolve(&registry, &unpinned) {
        if *pinned != latest {
            eprintln!(
                "note: hub:{}/{} has {latest} on the registry (project pins {pinned}); \
                 `weft update --to {latest}` moves to it",
                r.owner, r.name
            );
        }
    }
}

/// Is `root` inside one of the local caches? Cached templates are
/// immutable, so their lock is never written — it must ship with them.
fn under_cache(root: &Utf8Path) -> bool {
    let root = root.canonicalize_utf8().unwrap_or_else(|_| root.to_owned());
    [hub::cache_root(), git::cache_root()]
        .into_iter()
        .flatten()
        .any(|cache| {
            let cache = cache.canonicalize_utf8().unwrap_or(cache);
            root.starts_with(&cache)
        })
}

/// The CLI's [`IncludeResolver`](weft_engine::template::IncludeResolver):
/// path includes pass through; `hub:` and git includes resolve through
/// each template's own `weft.lock` (loaded lazily per root), fetch into the
/// caches, and record the resolution back into that root's lock — unless
/// frozen, offline, or the root is itself a cached (immutable) template.
pub struct RemoteResolver {
    registry: Option<String>,
    frozen: bool,
    offline: bool,
    /// root → (lock, changed). Loaded on first touch, flushed at the end.
    locks: BTreeMap<Utf8PathBuf, (Lock, bool)>,
}

impl RemoteResolver {
    pub fn new(registry: Option<String>, frozen: bool) -> Self {
        Self {
            registry,
            frozen,
            offline: false,
            locks: Default::default(),
        }
    }

    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
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
        self.locks.insert(root.to_owned(), (Lock::default(), false));
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

    /// The resolver every CLI command uses unless it has its own flags
    /// (`--frozen`/`--offline`/`--registry`): default registry, not frozen.
    pub fn standard() -> Self {
        Self::new(hub::registry_url(None).ok(), false)
    }

    fn lock_for(&mut self, root: &Utf8Path) -> &mut (Lock, bool) {
        self.locks
            .entry(root.to_owned())
            .or_insert_with(|| (Lock::load(root).unwrap_or_default(), false))
    }

    /// Why a fresh resolution is not allowed right now, if it isn't.
    fn pinned_only(&self, parent_root: &Utf8Path, decl: &IncludeDecl, req: &str) -> Option<String> {
        let what = format!("{} (`{}` {req})", decl.label(), decl.template);
        if self.frozen {
            Some(format!(
                "{what} is not in weft.lock (or the lock is stale); run `weft lock` — \
                 refusing to resolve under --frozen"
            ))
        } else if self.offline {
            Some(format!(
                "{what} is not in weft.lock (or the lock is stale) and this run is offline; \
                 run `weft lock` (or drop --offline) to resolve it"
            ))
        } else if under_cache(parent_root) {
            Some(format!(
                "{what} is not pinned in `{parent_root}/weft.lock`; this template was fetched \
                 from a remote source, so its author must run `weft lock` and commit weft.lock"
            ))
        } else {
            None
        }
    }

    fn resolve_hub(&mut self, parent_root: &Utf8Path, decl: &IncludeDecl) -> Result<Utf8PathBuf> {
        let refstr = decl.template.as_str();
        let req_str = decl.version.as_deref().with_context(|| {
            format!(
                "{}: hub template `{refstr}` needs a `version`",
                decl.label()
            )
        })?;
        let req = semver::VersionReq::parse(req_str).with_context(|| {
            format!("{}: `{req_str}` is not a semver requirement", decl.label())
        })?;
        let hub = match hub::parse_ref(refstr) {
            Some(r) => r?,
            None => bail!("{}: `{refstr}` is not a hub ref", decl.label()),
        };

        // A matching, satisfying lock entry → use the pinned version.
        let (lock, _) = self.lock_for(parent_root);
        if let Some(locked) = lock.entry(refstr) {
            if let (Some(version), Some(sha)) = (&locked.version, &locked.sha256) {
                if semver::Version::parse(version).is_ok_and(|v| req.matches(&v)) {
                    let (version, sha) = (version.clone(), sha.clone());
                    let registry = self.registry()?.to_owned();
                    return hub::ensure_cached(&registry, &hub, &version, &sha);
                }
            }
        }

        if let Some(why) = self.pinned_only(parent_root, decl, req_str) {
            bail!("{why}");
        }
        let registry = self.registry()?.to_owned();
        let (version, sha) = hub::resolve_req(&registry, &hub.owner, &hub.name, &req)?;
        let dir = hub::ensure_cached(&registry, &hub, &version, &sha)?;

        let (lock, changed) = self.lock_for(parent_root);
        if lock.upsert(Locked::hub(
            refstr.to_owned(),
            req_str.to_owned(),
            version,
            sha,
        )) {
            *changed = true;
        }
        Ok(dir)
    }

    fn resolve_git(&mut self, parent_root: &Utf8Path, decl: &IncludeDecl) -> Result<Utf8PathBuf> {
        let r = GitRef::parse(decl.template.as_str()).with_context(|| decl.label())?;
        if decl.version.is_some() {
            bail!(
                "{}: `version` only applies to `hub:` templates; pin a git template \
                 with `@rev` in the ref (`{}@v1.0.0`)",
                decl.label(),
                r.repo_ref()
            );
        }
        let req = r.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
        let key = r.repo_ref();

        // A lock entry for the same tracked rev → use its commit.
        let (lock, _) = self.lock_for(parent_root);
        if let Some(locked) = lock.entry(&key) {
            if locked.req == req {
                if let Some(commit) = locked.commit.clone() {
                    let export = git::export(&r, &commit, self.offline)?;
                    return git::template_dir(&r, &export);
                }
            }
        }

        if let Some(why) = self.pinned_only(parent_root, decl, &req) {
            bail!("{why}");
        }
        git::fetch(&r)?;
        let sha = git::resolve_rev(&r, r.rev.as_deref())?;
        let export = git::export(&r, &sha, false)?;
        let dir = git::template_dir(&r, &export)?;

        let (lock, changed) = self.lock_for(parent_root);
        if lock.upsert(Locked::git(key, req, sha)) {
            *changed = true;
        }
        Ok(dir)
    }
}

impl weft_engine::template::IncludeResolver for RemoteResolver {
    fn resolve(&mut self, parent_root: &Utf8Path, decl: &IncludeDecl) -> Result<Utf8PathBuf> {
        match decl.kind() {
            Kind::Path => Ok(parent_root.join(&decl.template)),
            Kind::Hub => self.resolve_hub(parent_root, decl),
            Kind::Git => self.resolve_git(parent_root, decl),
        }
    }
}

/// Load a template the way every CLI command must: path, `hub:`, and git
/// refs in `extends` and `[[include]]` resolve through the lock and caches
/// (the engine's `Template::load` only knows local paths). Lock updates are
/// written back.
pub fn load_template(dir: &Utf8Path) -> Result<weft_engine::template::Template> {
    let mut resolver = RemoteResolver::standard();
    let template = weft_engine::template::Template::load_with(dir, &mut resolver)?;
    resolver.flush()?;
    Ok(template)
}

/// Run an engine operation with the standard resolver (so remote `extends`
/// and includes resolve), writing back any lock it updated.
pub fn with_resolver<T>(
    f: impl FnOnce(&mut dyn weft_engine::template::IncludeResolver) -> Result<T>,
) -> Result<T> {
    let mut resolver = RemoteResolver::standard();
    let out = f(&mut resolver)?;
    resolver.flush()?;
    Ok(out)
}
