//! Upstream: `src/utils/patterns.ts`

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde_json::Value;

use crate::estree::to_value;

/// Upstream `commonStringPrefix`.
pub fn common_string_prefix(left_any: &Value, right_any: &Value) -> Option<String> {
    let left_str = to_value(left_any);
    let right_str = to_value(right_any);
    common_string_prefix_str(&left_str, &right_str)
}

pub fn common_string_prefix_str(left_str: &str, right_str: &str) -> Option<String> {
    let prefix: String = left_str
        .chars()
        .zip(right_str.chars())
        .take_while(|(l, r)| l == r)
        .map(|(l, _)| l)
        .collect();
    if prefix.is_empty() { None } else { Some(prefix) }
}

fn reverse_string(string: &str) -> String {
    string.chars().rev().collect()
}

/// Upstream `commonStringSuffix`.
pub fn common_string_suffix(left_str: &str, right_str: &str) -> Option<String> {
    common_string_prefix_str(&reverse_string(left_str), &reverse_string(right_str))
        .map(|prefix| reverse_string(&prefix))
}

pub struct CommonHexadecimalPrefixResult {
    pub one_time_occurence: usize,
    pub prefix: BTreeMap<String, usize>,
}

/// Upstream `commonHexadecimalPrefix`.
pub fn common_hexadecimal_prefix(identifiers_array: &[String]) -> CommonHexadecimalPrefixResult {
    let mut sorted: Vec<&String> = identifiers_array.iter().collect();
    sorted.sort();

    // Insertion order matters for the pairwise prefix matching; use IndexMap
    // to mirror the JS Map semantics.
    let mut prefix: IndexMap<String, usize> = IndexMap::new();

    'main_loop: for value in sorted {
        let keys: Vec<String> = prefix.keys().cloned().collect();
        for cp in keys {
            let count = prefix[&cp];
            let Some(common_str) = common_string_prefix_str(value, &cp) else {
                continue;
            };

            if common_str == cp || common_str.starts_with(&cp) {
                prefix.insert(cp, count + 1);
            } else if cp.starts_with(&common_str) {
                prefix.shift_remove(&cp);
                prefix.insert(common_str, count + 1);
            }
            continue 'main_loop;
        }
        prefix.insert(value.clone(), 1);
    }

    let mut one_time_occurence = 0usize;
    prefix.retain(|_, value| {
        if *value == 1 {
            one_time_occurence += 1;
            false
        } else {
            true
        }
    });

    CommonHexadecimalPrefixResult {
        one_time_occurence,
        prefix: prefix.into_iter().collect(),
    }
}
