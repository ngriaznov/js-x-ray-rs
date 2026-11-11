//! Upstream: `src/probes/warningAggregator.ts`

use indexmap::IndexMap;

use crate::estree::SourceLocation;
use crate::source_file::SourceFile;
use crate::utils::{SourceArrayLocation, to_array_location};
use crate::warnings::{GenerateWarningOptions, WarningLocation, generate_warning};

/// Probe context for probes reporting one warning per analysed file rather
/// than one warning per node: value → every location it was seen at.
pub type AggregatedLocations = IndexMap<String, Vec<SourceArrayLocation>>;

pub fn collect_location(
    locations: &mut AggregatedLocations,
    value: &str,
    location: Option<SourceLocation>,
) {
    let array_location = to_array_location(location);
    match locations.get_mut(value) {
        Some(collected) => collected.push(array_location),
        None => {
            locations.insert(value.to_owned(), vec![array_location]);
        }
    }
}

/// Emit a single warning gathering every collected value and location.
pub fn push_aggregated_warning(
    locations: &AggregatedLocations,
    source_file: &mut SourceFile,
    kind: &str,
    format_value: Option<&dyn Fn(&str) -> String>,
) {
    if locations.is_empty() {
        return;
    }

    let value = locations
        .keys()
        .map(|value| format_value.map_or_else(|| value.clone(), |format| format(value)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut warning = generate_warning(
        kind,
        GenerateWarningOptions {
            value: Some(value),
            ..Default::default()
        },
    );
    warning.location = WarningLocation::Multiple(locations.values().flatten().copied().collect());
    source_file.warnings.push(warning);
}
