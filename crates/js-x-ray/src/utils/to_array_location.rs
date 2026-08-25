//! Upstream: `src/utils/toArrayLocation.ts`

use crate::estree::{SourceLocation, root_location};

/// `[[line, column], [line, column]]`
pub type SourceArrayLocation = [[u64; 2]; 2];

pub use crate::estree::root_location as root_location_fn;

/// Upstream `toArrayLocation`.
pub fn to_array_location(location: Option<SourceLocation>) -> SourceArrayLocation {
    let location = location.unwrap_or_else(root_location);
    [
        [location.start.line, location.start.column],
        [location.end.line, location.end.column],
    ]
}
