//! Dependency resolution (spec §17.2).
//!
//! Semver, one version per package per environment, interface dependencies
//! satisfied by any provider. A conflict is reported with the two dependents
//! named, so the store can offer the versions that would satisfy both rather
//! than leaving the user to guess which package to blame.

use crate::manifest::PackageKind;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

/// One `[dependencies]` entry, as the resolver sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub id: String,
    /// Version requirement; `None` for an interface dependency.
    pub req: Option<String>,
    pub optional: bool,
    pub interface: bool,
}

/// A package the resolver may pick: installed, or offered by the catalog.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub version: String,
    pub kind: PackageKind,
    pub provides: Vec<String>,
    pub deps: Vec<Dependency>,
    pub path: PathBuf,
    pub installed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolution {
    /// Packages to install, dependencies before dependents. Already-installed
    /// packages that satisfy a requirement are not listed.
    pub install: Vec<(String, String)>,
    /// `(interface, provider id)` for every interface dependency met.
    pub bindings: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Missing {
        id: String,
        req: String,
        wanted_by: String,
    },
    Conflict {
        id: String,
        first: (String, String),
        second: (String, String),
    },
    NoProvider {
        interface: String,
        wanted_by: String,
    },
    Unknown(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Missing { id, req, wanted_by } => write!(
                f,
                "`{wanted_by}` needs `{id}` {req}, and no installed or offered package satisfies it"
            ),
            ResolveError::Conflict { id, first, second } => write!(
                f,
                "`{id}` cannot satisfy both `{}` (wants {}) and `{}` (wants {}); one version per package per environment",
                first.0, first.1, second.0, second.1
            ),
            ResolveError::NoProvider {
                interface,
                wanted_by,
            } => write!(
                f,
                "`{wanted_by}` needs a provider of `{interface}`, and nothing installed or offered provides it"
            ),
            ResolveError::Unknown(id) => write!(f, "`{id}` is not among the candidates"),
        }
    }
}

/// `^1.2`, `~1.2.3`, `=1.0.0`, `*`, or a bare `1.2` (treated as caret).
pub fn valid_requirement(req: &str) -> bool {
    let r = req.trim();
    if r == "*" {
        return true;
    }
    let body = r.trim_start_matches(['^', '~', '=']);
    !body.is_empty()
        && body
            .split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

pub fn version_satisfies(req: &str, version: &str) -> bool {
    crate::manifest::api_range_accepts(req, version)
}

fn parse_version(v: &str) -> (u32, u32, u32) {
    let mut it = v.trim().split('.');
    let a = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let b = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let c = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    (a, b, c)
}

/// Resolve everything `root` needs from `candidates`.
pub fn resolve(root: &str, candidates: &[Candidate]) -> Result<Resolution, ResolveError> {
    let root_c = candidates
        .iter()
        .find(|c| c.id == root)
        .ok_or_else(|| ResolveError::Unknown(root.to_string()))?;

    // id -> (chosen version, who first asked, with what requirement)
    let mut chosen: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    let mut bindings: Vec<(String, String)> = Vec::new();
    let mut queue: VecDeque<(String, String)> = VecDeque::new(); // (dependent, dependent version)
    queue.push_back((root_c.id.clone(), root_c.version.clone()));

    while let Some((dependent, dependent_version)) = queue.pop_front() {
        let Some(dc) = candidates
            .iter()
            .find(|c| c.id == dependent && c.version == dependent_version)
        else {
            continue;
        };
        for dep in &dc.deps {
            if dep.interface {
                // Any provider will do; prefer what is already installed.
                let provider = candidates
                    .iter()
                    .filter(|c| c.provides.contains(&dep.id))
                    .max_by_key(|c| (c.installed, parse_version(&c.version)));
                match provider {
                    Some(p) => {
                        if !bindings.iter().any(|(i, _)| *i == dep.id) {
                            bindings.push((dep.id.clone(), p.id.clone()));
                            if !p.installed && !chosen.contains_key(&p.id) {
                                chosen.insert(
                                    p.id.clone(),
                                    (
                                        p.version.clone(),
                                        dependent.clone(),
                                        format!("provides {}", dep.id),
                                    ),
                                );
                                order.push((p.id.clone(), p.version.clone()));
                                queue.push_back((p.id.clone(), p.version.clone()));
                            }
                        }
                    }
                    None if dep.optional => {}
                    None => {
                        return Err(ResolveError::NoProvider {
                            interface: dep.id.clone(),
                            wanted_by: dependent.clone(),
                        });
                    }
                }
                continue;
            }

            let req = dep.req.clone().unwrap_or_else(|| "*".into());
            if let Some((have, by, by_req)) = chosen.get(&dep.id) {
                if !version_satisfies(&req, have) {
                    return Err(ResolveError::Conflict {
                        id: dep.id.clone(),
                        first: (by.clone(), by_req.clone()),
                        second: (dependent.clone(), req),
                    });
                }
                continue;
            }

            // An installed version that satisfies wins; otherwise the highest
            // offered version that does.
            let pick = candidates
                .iter()
                .filter(|c| c.id == dep.id && version_satisfies(&req, &c.version))
                .max_by_key(|c| (c.installed, parse_version(&c.version)));
            match pick {
                Some(c) => {
                    // Something already installed at an incompatible version is
                    // a conflict too: one version per package per environment.
                    if let Some(inst) = candidates.iter().find(|x| {
                        x.id == dep.id && x.installed && !version_satisfies(&req, &x.version)
                    }) {
                        return Err(ResolveError::Conflict {
                            id: dep.id.clone(),
                            first: ("installed".into(), format!("={}", inst.version)),
                            second: (dependent.clone(), req),
                        });
                    }
                    chosen.insert(
                        dep.id.clone(),
                        (c.version.clone(), dependent.clone(), req.clone()),
                    );
                    if !c.installed {
                        order.push((c.id.clone(), c.version.clone()));
                    }
                    queue.push_back((c.id.clone(), c.version.clone()));
                }
                None if dep.optional => {}
                None => {
                    return Err(ResolveError::Missing {
                        id: dep.id.clone(),
                        req,
                        wanted_by: dependent.clone(),
                    });
                }
            }
        }
    }

    // Dependencies before dependents: what was discovered last is deepest.
    order.reverse();
    Ok(Resolution {
        install: order,
        bindings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, version: &str, deps: &[(&str, &str)], installed: bool) -> Candidate {
        Candidate {
            id: id.into(),
            version: version.into(),
            kind: PackageKind::Harness,
            provides: Vec::new(),
            deps: deps
                .iter()
                .map(|(d, r)| Dependency {
                    id: (*d).into(),
                    req: Some((*r).into()),
                    optional: false,
                    interface: false,
                })
                .collect(),
            path: PathBuf::from(id),
            installed,
        }
    }

    #[test]
    fn requirements_are_recognised() {
        for ok in ["^1.2", "~1.2.3", "=1.0.0", "*", "1.2", "2"] {
            assert!(valid_requirement(ok), "{ok}");
        }
        for bad in ["", "latest", "^x", "1..2", ">=1"] {
            assert!(!valid_requirement(bad), "{bad}");
        }
    }

    #[test]
    fn a_library_a_harness_needs_is_installed_first() {
        let cands = vec![
            cand("io.x.app", "1.0.0", &[("io.x.geo", "^1.2")], false),
            cand("io.x.geo", "1.4.0", &[], false),
            cand("io.x.geo", "1.1.0", &[], false),
        ];
        let r = resolve("io.x.app", &cands).unwrap();
        assert_eq!(
            r.install,
            vec![("io.x.geo".to_string(), "1.4.0".to_string())],
            "the highest satisfying version, and nothing for the root itself"
        );
    }

    #[test]
    fn an_installed_version_that_satisfies_is_reused_not_reinstalled() {
        let cands = vec![
            cand("io.x.app", "1.0.0", &[("io.x.geo", "^1.2")], false),
            cand("io.x.geo", "1.3.0", &[], true),
            cand("io.x.geo", "1.9.0", &[], false),
        ];
        let r = resolve("io.x.app", &cands).unwrap();
        assert!(
            r.install.is_empty(),
            "1.3.0 is installed and satisfies ^1.2"
        );
    }

    #[test]
    fn two_dependents_wanting_incompatible_majors_is_a_conflict_that_names_both() {
        let cands = vec![
            cand(
                "io.x.app",
                "1.0.0",
                &[("io.x.a", "^1"), ("io.x.b", "^1")],
                false,
            ),
            cand("io.x.a", "1.0.0", &[("io.x.geo", "^1.0")], false),
            cand("io.x.b", "1.0.0", &[("io.x.geo", "^2.0")], false),
            cand("io.x.geo", "1.5.0", &[], false),
            cand("io.x.geo", "2.1.0", &[], false),
        ];
        match resolve("io.x.app", &cands) {
            Err(ResolveError::Conflict { id, first, second }) => {
                assert_eq!(id, "io.x.geo");
                assert_eq!(first.0, "io.x.a");
                assert_eq!(second.0, "io.x.b");
                let msg = ResolveError::Conflict { id, first, second }.to_string();
                assert!(msg.contains("io.x.a") && msg.contains("io.x.b"), "{msg}");
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn an_installed_incompatible_version_is_also_a_conflict() {
        let cands = vec![
            cand("io.x.app", "1.0.0", &[("io.x.geo", "^2.0")], false),
            cand("io.x.geo", "1.5.0", &[], true),
            cand("io.x.geo", "2.0.0", &[], false),
        ];
        assert!(matches!(
            resolve("io.x.app", &cands),
            Err(ResolveError::Conflict { .. })
        ));
    }

    #[test]
    fn a_missing_dependency_says_who_wanted_it() {
        let cands = vec![cand("io.x.app", "1.0.0", &[("io.x.nope", "^1")], false)];
        match resolve("io.x.app", &cands) {
            Err(ResolveError::Missing { id, wanted_by, .. }) => {
                assert_eq!(id, "io.x.nope");
                assert_eq!(wanted_by, "io.x.app");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn interface_dependencies_bind_to_any_provider_installed_first() {
        let mut app = cand("io.x.cfd", "1.0.0", &[], false);
        app.deps.push(Dependency {
            id: "localspace.geometry.v1".into(),
            req: None,
            optional: false,
            interface: true,
        });
        let mut sketch = cand("io.x.sketch", "3.0.0", &[], false);
        sketch.provides = vec!["localspace.geometry.v1".into()];
        let mut cad = cand("io.x.cad", "1.0.0", &[], true);
        cad.provides = vec!["localspace.geometry.v1".into()];

        let r = resolve("io.x.cfd", &[app.clone(), sketch.clone(), cad.clone()]).unwrap();
        assert_eq!(
            r.bindings,
            vec![("localspace.geometry.v1".to_string(), "io.x.cad".to_string())]
        );
        assert!(
            r.install.is_empty(),
            "the installed provider needs nothing installed"
        );

        // With no installed provider, the offered one is installed.
        let r = resolve("io.x.cfd", &[app.clone(), sketch]).unwrap();
        assert_eq!(
            r.install,
            vec![("io.x.sketch".to_string(), "3.0.0".to_string())]
        );

        // With no provider at all, a clear refusal.
        assert!(matches!(
            resolve("io.x.cfd", &[app]),
            Err(ResolveError::NoProvider { .. })
        ));
    }

    #[test]
    fn optional_dependencies_are_skipped_when_absent() {
        let mut app = cand("io.x.app", "1.0.0", &[], false);
        app.deps.push(Dependency {
            id: "io.x.viewer".into(),
            req: Some("^2".into()),
            optional: true,
            interface: false,
        });
        assert!(resolve("io.x.app", &[app]).unwrap().install.is_empty());
    }
}
