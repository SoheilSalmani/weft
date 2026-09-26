//! Where a template comes from: a local path, a Weft Hub ref
//! (`hub:owner/name[@version]`), or a git repository.
//!
//! Git refs follow `<repo>[//<subdir>][@<rev>]`:
//!
//! - `<repo>` is `gh:owner/repo` (GitHub shorthand), any URL git understands
//!   (`https://…`, `ssh://…`, `file://…`), or the scp form `git@host:path`.
//! - `//<subdir>` names the template inside a repository of templates.
//! - `@<rev>` is a tag, branch, or commit to track; absent means the remote's
//!   default branch.
//!
//! The engine only parses and classifies. Fetching and checking out is the
//! CLI's job — it hands the engine a local directory, so the engine never
//! touches the network (see [`crate::template::IncludeResolver`]).

use anyhow::{bail, Result};
use camino::Utf8PathBuf;

/// The kind of a template source spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Path,
    Hub,
    Git,
}

/// Classify a spec without fully parsing it.
pub fn kind(spec: &str) -> Kind {
    if spec.starts_with("hub:") {
        Kind::Hub
    } else if spec.starts_with("gh:") || split_url(spec).is_some() || split_scp(spec).is_some() {
        Kind::Git
    } else {
        Kind::Path
    }
}

/// A parsed git source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRef {
    /// The repository as the user wrote it (`gh:owner/repo` or a URL), with
    /// any `.git` suffix stripped from the shorthand. Displayed and stored.
    pub repo: String,
    /// The URL to clone (`gh:` expands to `https://github.com/owner/repo.git`).
    pub url: String,
    /// The template's directory inside the repository; `None` = the root.
    pub subdir: Option<Utf8PathBuf>,
    /// Tag, branch, or commit to track; `None` = the remote's default branch.
    pub rev: Option<String>,
}

impl GitRef {
    pub fn parse(spec: &str) -> Result<Self> {
        let spec = spec.trim();
        // Split the part git needs verbatim (scheme + host, or `user@host:`)
        // from the path, then pick `//subdir` and `@rev` out of the path so a
        // `@` in the user part is never mistaken for a revision.
        let (prefix, path, shorthand) = if let Some(rest) = spec.strip_prefix("gh:") {
            ("https://github.com/".to_owned(), rest, true)
        } else if let Some((prefix, path)) = split_url(spec) {
            (prefix.to_owned(), path, false)
        } else if let Some((prefix, path)) = split_scp(spec) {
            (prefix.to_owned(), path, false)
        } else {
            bail!("`{spec}` is not a git source (expected `gh:owner/repo`, a git URL, or `user@host:path`)");
        };

        let (repo_path, subdir, rev) = match path.split_once("//") {
            Some((repo_path, rest)) => {
                let (subdir, rev) = split_rev(rest);
                if subdir.is_empty() {
                    bail!("`{spec}`: empty subdirectory after `//`");
                }
                (repo_path, Some(subdir), rev)
            }
            None => {
                let (repo_path, rev) = split_rev(path);
                (repo_path, None, rev)
            }
        };
        let repo_path = repo_path.trim_end_matches('/');
        if repo_path.is_empty() {
            bail!("`{spec}`: missing repository path");
        }

        let (repo, url) = if shorthand {
            let bare = repo_path.strip_suffix(".git").unwrap_or(repo_path);
            let Some((owner, name)) = bare.split_once('/') else {
                bail!("`{spec}`: GitHub shorthand is `gh:owner/repo[//subdir][@rev]`");
            };
            if !valid_segment(owner) || !valid_segment(name) {
                bail!("`{spec}`: GitHub shorthand is `gh:owner/repo[//subdir][@rev]`");
            }
            (
                format!("gh:{owner}/{name}"),
                format!("https://github.com/{owner}/{name}.git"),
            )
        } else {
            let url = format!("{prefix}{repo_path}");
            (url.clone(), url)
        };

        let subdir = match subdir {
            None => None,
            Some(s) => {
                let s = s.trim_matches('/');
                if s.is_empty()
                    || s.split('/')
                        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
                {
                    bail!("`{spec}`: subdirectory must be a relative path inside the repository");
                }
                Some(Utf8PathBuf::from(s))
            }
        };
        let rev = match rev {
            None => None,
            Some(r) => {
                if r.is_empty()
                    || r.starts_with('-')
                    || r.chars().any(|c| c.is_whitespace() || c.is_control())
                {
                    bail!("`{spec}`: invalid revision `{r}`");
                }
                Some(r.to_owned())
            }
        };
        Ok(GitRef {
            repo,
            url,
            subdir,
            rev,
        })
    }

    /// The same source tracking a different revision.
    pub fn with_rev(&self, rev: Option<String>) -> Self {
        GitRef {
            rev,
            ..self.clone()
        }
    }

    /// `repo[//subdir]` without the revision — the lockfile key.
    pub fn repo_ref(&self) -> String {
        match &self.subdir {
            Some(s) => format!("{}//{s}", self.repo),
            None => self.repo.clone(),
        }
    }

    /// A stable identity for caching: the same repository written as
    /// `gh:owner/repo`, `https://github.com/owner/repo`, or with `.git`
    /// shares one clone.
    pub fn canonical_url(&self) -> String {
        let url = self.url.trim_end_matches('/');
        let url = url.strip_suffix(".git").unwrap_or(url);
        if url.contains("github.com") {
            url.to_ascii_lowercase()
        } else {
            url.to_owned()
        }
    }
}

impl std::fmt::Display for GitRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.repo_ref())?;
        if let Some(rev) = &self.rev {
            write!(f, "@{rev}")?;
        }
        Ok(())
    }
}

/// Is `s` a full commit hash (what makes a pinned checkout usable offline)?
pub fn is_full_commit(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// `scheme://authority/` + path.
fn split_url(spec: &str) -> Option<(&str, &str)> {
    let idx = spec.find("://")?;
    let scheme = &spec[..idx];
    let valid_scheme = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid_scheme {
        return None;
    }
    let after = &spec[idx + 3..];
    // The authority runs to the first `/`; `file://` has an empty one.
    let slash = after.find('/')?;
    let end = idx + 3 + slash + 1;
    Some((&spec[..end], &spec[end..]))
}

/// `user@host:` + path (the scp-like form git accepts).
fn split_scp(spec: &str) -> Option<(&str, &str)> {
    let colon = spec.find(':')?;
    let head = &spec[..colon];
    if head.contains('/') || !head.contains('@') || head.contains("://") {
        return None;
    }
    let ok = head
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '-' | '_'));
    if !ok {
        return None;
    }
    Some((&spec[..=colon], &spec[colon + 1..]))
}

fn split_rev(s: &str) -> (&str, Option<&str>) {
    match s.rsplit_once('@') {
        Some((head, rev)) => (head, Some(rev)),
        None => (s, None),
    }
}

fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_specs() {
        assert_eq!(kind("hub:acme/hello"), Kind::Hub);
        assert_eq!(kind("gh:acme/hello"), Kind::Git);
        assert_eq!(kind("https://github.com/acme/hello.git"), Kind::Git);
        assert_eq!(kind("git@github.com:acme/hello.git"), Kind::Git);
        assert_eq!(kind("file:///tmp/repo"), Kind::Git);
        assert_eq!(kind("../hello"), Kind::Path);
        assert_eq!(kind("/abs/hello"), Kind::Path);
        assert_eq!(kind("hello"), Kind::Path);
        assert_eq!(kind("C:/templates"), Kind::Path);
    }

    #[test]
    fn parses_github_shorthand() {
        let r = GitRef::parse("gh:soheil/templates.git//base@v1.2.0").unwrap();
        assert_eq!(r.repo, "gh:soheil/templates");
        assert_eq!(r.url, "https://github.com/soheil/templates.git");
        assert_eq!(r.subdir.as_deref().map(|p| p.as_str()), Some("base"));
        assert_eq!(r.rev.as_deref(), Some("v1.2.0"));
        assert_eq!(r.to_string(), "gh:soheil/templates//base@v1.2.0");
        assert_eq!(r.repo_ref(), "gh:soheil/templates//base");

        let plain = GitRef::parse("gh:soheil/react-template").unwrap();
        assert_eq!(plain.subdir, None);
        assert_eq!(plain.rev, None);
        assert_eq!(plain.to_string(), "gh:soheil/react-template");

        assert!(GitRef::parse("gh:soheil").is_err());
        assert!(GitRef::parse("gh:soheil/a/b").is_err());
    }

    #[test]
    fn parses_urls_and_scp() {
        let r = GitRef::parse("https://github.com/soheil/templates.git//apps/base@main").unwrap();
        assert_eq!(r.url, "https://github.com/soheil/templates.git");
        assert_eq!(r.subdir.as_deref().map(|p| p.as_str()), Some("apps/base"));
        assert_eq!(r.rev.as_deref(), Some("main"));
        assert_eq!(
            r.to_string(),
            "https://github.com/soheil/templates.git//apps/base@main"
        );

        // `@` in the user part is not a revision; a branch may contain `/`.
        let scp = GitRef::parse("git@github.com:soheil/templates.git@feature/x").unwrap();
        assert_eq!(scp.url, "git@github.com:soheil/templates.git");
        assert_eq!(scp.rev.as_deref(), Some("feature/x"));

        let file = GitRef::parse("file:///tmp/repo.git//templates/hello").unwrap();
        assert_eq!(file.url, "file:///tmp/repo.git");
        assert_eq!(
            file.subdir.as_deref().map(|p| p.as_str()),
            Some("templates/hello")
        );

        // Credentials in the authority are left alone.
        let cred = GitRef::parse("https://user:tok@example.com/x/y.git@v2").unwrap();
        assert_eq!(cred.url, "https://user:tok@example.com/x/y.git");
        assert_eq!(cred.rev.as_deref(), Some("v2"));
    }

    #[test]
    fn rejects_bad_subdirs_and_revs() {
        assert!(GitRef::parse("gh:a/b//../x").is_err());
        assert!(GitRef::parse("gh:a/b//").is_err());
        assert!(GitRef::parse("gh:a/b@-x").is_err());
        assert!(GitRef::parse("gh:a/b@").is_err());
        assert!(GitRef::parse("gh:a/b@v 1").is_err());
    }

    #[test]
    fn canonical_url_unifies_spellings() {
        let a = GitRef::parse("gh:Soheil/Templates").unwrap();
        let b = GitRef::parse("https://github.com/soheil/templates.git").unwrap();
        let c = GitRef::parse("https://github.com/soheil/templates").unwrap();
        assert_eq!(a.canonical_url(), b.canonical_url());
        assert_eq!(b.canonical_url(), c.canonical_url());
    }

    #[test]
    fn full_commit_detection() {
        assert!(is_full_commit(&"a".repeat(40)));
        assert!(!is_full_commit("abc123"));
        assert!(!is_full_commit("v1.0.0"));
    }
}
