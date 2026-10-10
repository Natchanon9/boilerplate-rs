//! `info env` and `info features` (runtime.md AC-3, AC-4).

use std::io::{self, Write};

use crate::config::Settings;
use crate::features::Feature;

/// Prints the effective configuration as `NAME=value` lines, with every secret redacted.
pub fn env(settings: &Settings, out: &mut impl Write) -> io::Result<()> {
    for (name, value) in settings.redacted_env() {
        writeln!(out, "{name}={value}")?;
    }
    Ok(())
}

/// Prints each optional feature as `name enabled|disabled`.
pub fn features(settings: &Settings, out: &mut impl Write) -> io::Result<()> {
    let features = settings.features();
    for feature in Feature::ALL {
        let state = if features.is_enabled(feature) {
            "enabled"
        } else {
            "disabled"
        };
        writeln!(out, "{:<8} {state}", feature.name())?;
    }
    Ok(())
}
