//! Upstream: `src/utils/` — one Rust module per upstream file.

pub mod hex;
mod get_sub_member_expression_segments;
mod is_evil_identifier;
mod is_minified_code;
mod is_one_line_expression_export;
mod is_string_base64;
mod is_svg;
mod make_prefix_remover;
mod patterns;
mod string_suspicion_score;
mod strip_node_prefix;
mod to_array_location;

pub use get_sub_member_expression_segments::*;
pub use is_evil_identifier::*;
pub use is_minified_code::*;
pub use is_one_line_expression_export::*;
pub use is_string_base64::*;
pub use is_svg::*;
pub use make_prefix_remover::*;
pub use patterns::*;
pub use string_suspicion_score::*;
pub use strip_node_prefix::*;
pub use to_array_location::*;

pub mod safe_regex;
