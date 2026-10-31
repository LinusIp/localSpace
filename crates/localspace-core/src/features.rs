//! Switched features (docs/decisions/2026-10-31-switched-features.md): a feature still being
//! built ships off, and is turned on per installation. A switch never turns
//! off a protection.
//!
//! What is on here comes, in order, from:
//! 1. the build's channel, read from `features.toml` when Core is compiled
//!    (`LOCALSPACE_CHANNEL`: `dev`, `preview` or `stable`; a build that names
//!    none is `stable`);
//! 2. what this installation has stored, once someone turned a switch;
//! 3. in a development build only, `LOCALSPACE_FEATURES`, for example
//!    `+themes,-translation`.
//!
//! An organisation's policy will hold some switches where they are
//! (`locked`); until it exists, nothing is locked.

use crate::store::Store;
use anyhow::{Context, Result, bail};
use localspace_proto as proto;
use proto::Feature;

/// The defaults of every channel, as checked in.
const FEATURES_TOML: &str = include_str!("../../../features.toml");

/// The channel this build was made for.
pub fn channel() -> &'static str {
    option_env!("LOCALSPACE_CHANNEL").unwrap_or("stable")
}

/// Every feature there is, in a fixed order.
pub const ALL: [Feature; 3] = [Feature::Themes, Feature::Translation, Feature::Lfm3];

fn named(name: &str) -> Option<Feature> {
    ALL.into_iter().find(|f| name_of(*f) == name)
}

/// A feature's name, as `features.toml` and the wire write it.
pub fn name_of(feature: Feature) -> &'static str {
    match feature {
        Feature::Themes => "themes",
        Feature::Translation => "translation",
        Feature::Lfm3 => "lfm3",
    }
}

/// The features `channel` turns on, from a `features.toml`.
pub fn defaults(toml_text: &str, channel: &str) -> Result<Vec<Feature>> {
    let table: toml::Table = toml::from_str(toml_text).context("features.toml")?;
    let channels = table
        .get("channels")
        .and_then(|c| c.as_table())
        .context("features.toml has no [channels]")?;
    let Some(list) = channels.get(channel) else {
        bail!("features.toml has no channel `{channel}`");
    };
    let names = list
        .as_array()
        .with_context(|| format!("channel `{channel}` is not a list"))?;
    let mut on = Vec::new();
    for name in names {
        let name = name.as_str().context("a feature's name is a string")?;
        on.push(named(name).with_context(|| format!("`{name}` is not a feature"))?);
    }
    on.sort();
    on.dedup();
    Ok(on)
}

/// `LOCALSPACE_FEATURES` applied to `on`: `+name` turns one on, `-name` off.
/// Names that aren't features are left out.
pub fn overridden(mut on: Vec<Feature>, spec: &str) -> Vec<Feature> {
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (turn_on, name) = match part.strip_prefix('-') {
            Some(name) => (false, name),
            None => (true, part.strip_prefix('+').unwrap_or(part)),
        };
        let Some(feature) = named(name) else { continue };
        on.retain(|f| *f != feature);
        if turn_on {
            on.push(feature);
        }
    }
    on.sort();
    on
}

/// What is on here now.
pub fn current(store: &Store) -> proto::Features {
    let mut on = match store.features() {
        Ok(Some(names)) => {
            let mut on: Vec<Feature> = names.iter().filter_map(|n| named(n)).collect();
            on.sort();
            on.dedup();
            on
        }
        Ok(None) => defaults(FEATURES_TOML, channel()).unwrap_or_else(|e| {
            tracing::warn!("features: {e:#}; every feature is off");
            Vec::new()
        }),
        Err(e) => {
            tracing::warn!(
                "features: the stored switches can't be read: {e:#}; every feature is off"
            );
            Vec::new()
        }
    };
    if cfg!(debug_assertions)
        && let Ok(spec) = std::env::var("LOCALSPACE_FEATURES")
    {
        on = overridden(on, &spec);
    }
    proto::Features {
        on,
        locked: Vec::new(),
    }
}

/// Turns one feature on or off for this installation, and keeps it.
pub fn set(store: &Store, feature: Feature, turn_on: bool) -> Result<proto::Features> {
    let now = current(store);
    if now.locked.contains(&feature) {
        bail!("`{}` is set by your organisation", name_of(feature));
    }
    let mut on = now.on;
    on.retain(|f| *f != feature);
    if turn_on {
        on.push(feature);
    }
    on.sort();
    let names: Vec<String> = on.iter().map(|f| name_of(*f).to_string()).collect();
    store.set_features(&names)?;
    Ok(current(store))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checked_in_file_names_every_channel_and_only_features() {
        for channel in ["dev", "preview", "stable"] {
            defaults(FEATURES_TOML, channel).unwrap();
        }
        assert!(defaults(FEATURES_TOML, "nightly").is_err());
        assert!(defaults("[channels]\nstable = [\"wings\"]", "stable").is_err());
    }

    /// Only the preview has features on, the new look and LFM3, and never
    /// translation (docs/decisions/2026-10-31-0.3.0-preview.1.md); the other
    /// channels are off, and a build that names no channel is `stable`.
    #[test]
    fn only_the_preview_turns_features_on() {
        assert_eq!(
            defaults(FEATURES_TOML, "preview").unwrap(),
            vec![Feature::Themes, Feature::Lfm3]
        );
        for channel in ["dev", "stable"] {
            assert_eq!(
                defaults(FEATURES_TOML, channel).unwrap(),
                Vec::<Feature>::new(),
                "{channel}"
            );
        }
    }

    #[test]
    fn a_development_override_turns_features_on_and_off() {
        assert_eq!(overridden(vec![], "+themes"), vec![Feature::Themes]);
        assert_eq!(
            overridden(vec![Feature::Themes, Feature::Translation], "-themes"),
            vec![Feature::Translation]
        );
        assert_eq!(
            overridden(vec![], "translation, wings,+themes"),
            vec![Feature::Themes, Feature::Translation]
        );
    }

    #[test]
    fn a_switch_turned_is_kept_and_read_back() {
        let store = Store::in_memory().unwrap();
        assert_eq!(current(&store).on, Vec::<Feature>::new());
        assert_eq!(
            set(&store, Feature::Themes, true).unwrap().on,
            vec![Feature::Themes]
        );
        assert_eq!(current(&store).on, vec![Feature::Themes]);
        assert_eq!(
            set(&store, Feature::Themes, false).unwrap().on,
            Vec::<Feature>::new()
        );
        assert_eq!(current(&store).on, Vec::<Feature>::new());
    }
}
