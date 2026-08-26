//! Upstream: `test/utils/*.spec.ts`

use serde_json::json;

use js_x_ray_rs::estree::{Position, SourceLocation};
use js_x_ray_rs::parser::{JsSourceParser, SourceParser};
use js_x_ray_rs::utils::hex::{SAFE_HEX_VALUES, encode_hex, is_hex, is_hex_str, is_safe};
use js_x_ray_rs::utils::{
    Base64Options, common_hexadecimal_prefix, common_string_prefix_str, common_string_suffix,
    get_sub_member_expression_segments, is_evil_identifier_path, is_minified_code,
    is_one_line_expression_export, is_string_base64, is_svg, is_svg_path, make_prefix_remover,
    string_char_diversity, string_suspicion_score, strip_node_prefix, to_array_location,
};

/// Small dependency-free PRNG (xorshift64) standing in for `randomBytes`:
/// the assertions below only need arbitrary hex-digit strings of a given
/// length, not cryptographic randomness.
fn random_hex_chars(len: usize, seed: &mut u64) -> String {
    const HEX_DIGITS: &[u8] = b"0123456789abcdef";
    (0..len)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            HEX_DIGITS[(*seed % 16) as usize] as char
        })
        .collect()
}

// --- getSubMemberExpressionSegments.spec.ts ---------------------------------

// Upstream drives a generator through an `IteratorMatcher`; the Rust port is
// eager, so the adaptation is a direct equality check on the collected steps.
#[test]
fn given_a_member_expression_then_it_should_return_each_segments_except_the_last_one() {
    let segments = get_sub_member_expression_segments("foo.bar.xd");
    assert_eq!(segments, vec!["foo".to_string(), "foo.bar".to_string()]);
}

// --- hex.spec.ts -------------------------------------------------------------

#[test]
fn is_hex_must_return_true_for_random_4_character_hexadecimal_values() {
    let mut seed = 0x1234_5678_9abc_def0;
    let hex_value = random_hex_chars(4, &mut seed);
    assert!(
        is_hex_str(&hex_value),
        "Hexadecimal value '{hex_value}' must return true"
    );
}

#[test]
fn is_hex_must_return_true_for_estree_literals_containing_random_4_character_hexadecimal_values() {
    let mut seed = 0x0fed_cba9_8765_4321;
    let hex_value = random_hex_chars(4, &mut seed);
    let literal = json!({ "type": "Literal", "value": hex_value });
    assert!(
        is_hex(&literal),
        "Hexadecimal value '{hex_value}' must return true"
    );
}

#[test]
fn is_hex_an_hexadecimal_value_must_be_at_least_4_chars_long() {
    let mut seed = 0x9999_1111_2222_3333;
    let hex_value = random_hex_chars(2, &mut seed);
    assert!(
        !is_hex_str(&hex_value),
        "Hexadecimal value '{hex_value}' must return false"
    );
}

#[test]
fn is_hex_should_return_false_for_non_string_estree_literal_values() {
    // Adapted: `is_hex` takes `&serde_json::Value`, so the JS "typeof number"
    // input becomes a JSON number, which still isn't a string value.
    assert!(
        !is_hex(&json!(100)),
        "100 is typeof number so it must always return false"
    );
}

#[test]
fn is_safe_must_return_true_for_a_value_with_length_lower_or_equal_five_characters() {
    assert!(is_safe(&json!("h2l5x")));
}

#[test]
fn is_safe_must_return_true_if_the_string_diversity_is_only_two_characters_or_lower() {
    assert!(is_safe(&json!("aaaaaaaaaaaaaabbbbbbbbbbbbb")));
}

#[test]
fn is_safe_must_always_return_true_if_argument_is_only_number_lower_or_upper_letters() {
    for hex_value in ["00000000", "aaaaaaaa", "AAAAAAAAA"] {
        assert!(is_safe(&json!(hex_value)));
    }
}

#[test]
fn is_safe_must_always_return_true_if_the_value_start_with_one_of_the_safe_values() {
    let mut seed = 0x1111_2222_3333_4444;
    for safe_value in SAFE_HEX_VALUES {
        let hex_value = format!("{safe_value}{}", random_hex_chars(4, &mut seed));
        assert!(is_safe(&json!(hex_value)));
    }
}

#[test]
fn is_safe_must_return_true_because_it_starts_with_a_safe_pattern_and_must_lowercase_the_string() {
    assert!(is_safe(&json!("ABCDEF1234567890")));
}

#[test]
fn is_safe_must_always_return_false_if_the_value_start_with_one_of_the_unsafe_values() {
    // `UNSAFE_HEX_VALUES` (hex of "require"/"length") is a private constant
    // upstream re-exports as `CONSTANTS.UNSAFE_HEXA_VALUES`; the Rust port
    // only exposes the pure `encode_hex` helper it's built from, so the
    // values are recomputed here instead.
    for unsafe_value in ["require", "length"].map(encode_hex) {
        assert!(!is_safe(&json!(unsafe_value)));
    }
}

// --- isEvilIdentifierPath.spec.ts -------------------------------------------

#[test]
fn given_a_random_prototype_method_name_then_it_should_return_false() {
    assert!(!is_evil_identifier_path("Function.prototype.foo"));
}

#[test]
fn given_a_list_of_evil_identifiers_it_should_always_return_true() {
    for identifier in [
        "Function.prototype.bind",
        "Function.prototype.call",
        "Function.prototype.apply",
    ] {
        assert!(is_evil_identifier_path(identifier));
    }
}

// --- isMinifiedCode.spec.ts --------------------------------------------------

#[test]
fn should_return_false_for_formatted_multi_line_code() {
    let formatted_code = "\n            function add(a, b) {\n                return a + b;\n            }\n        ";
    assert!(!is_minified_code(formatted_code));
}

#[test]
fn should_return_true_for_a_single_line_code() {
    let minified_code = "function add(a,b){return a+b;}";
    assert!(is_minified_code(minified_code));
}

#[test]
fn should_return_true_when_median_line_length_exceeds_200() {
    let long_string = "a".repeat(250);
    let code = format!("\n{long_string}\n{long_string}\n");
    assert!(is_minified_code(&code));
}

#[test]
fn should_ignore_comments_when_evaluating_minification() {
    let code = "\n    // this is a comment\n    function test() {\n    return 1;\n    }\n    ";
    assert!(!is_minified_code(code));
}

#[test]
fn should_handle_empty_code_as_minified() {
    assert!(is_minified_code(""));
}

#[test]
fn should_treat_comment_only_code_as_minified() {
    let code = "\n// comment\n/* block comment */\n";
    assert!(is_minified_code(code));
}

#[test]
fn should_not_be_affected_by_a_single_long_line() {
    let long_line = "a".repeat(250);
    let code = format!("\nshort line\n{long_line}\nshort line\n");
    assert!(!is_minified_code(&code));
}

// --- isOneLineExpressionExport.spec.ts ---------------------------------------

#[test]
fn is_one_line_should_return_false_for_empty_body() {
    assert!(!is_one_line_expression_export(&[]));
}

#[test]
fn is_one_line_should_return_false_for_multiple_statements() {
    let body = JsSourceParser
        .parse("require('a');\nrequire('b');\n")
        .unwrap();
    assert!(!is_one_line_expression_export(&body));
}

#[test]
fn is_one_line_should_return_true_for_single_require_call() {
    let body = JsSourceParser.parse("require('a');").unwrap();
    assert!(is_one_line_expression_export(&body));
}

#[test]
fn is_one_line_should_return_true_for_module_exports_assignment_to_require() {
    let body = JsSourceParser
        .parse("module.exports = require('a');")
        .unwrap();
    assert!(is_one_line_expression_export(&body));
}

#[test]
fn is_one_line_should_return_true_for_conditional_require_export() {
    let body = JsSourceParser
        .parse("module.exports = condition ? require('a') : require('b');")
        .unwrap();
    assert!(is_one_line_expression_export(&body));
}

#[test]
fn is_one_line_should_return_true_for_logical_require_export() {
    let body = JsSourceParser
        .parse("module.exports = condition && require('b');")
        .unwrap();
    assert!(is_one_line_expression_export(&body));
}

#[test]
fn is_one_line_should_return_false_for_non_require_expression() {
    let body = JsSourceParser.parse("module.exports = foo();").unwrap();
    assert!(!is_one_line_expression_export(&body));
}

#[test]
fn is_one_line_should_return_true_for_require_member_access() {
    let body = JsSourceParser
        .parse("module.exports = require(\"foo\").bar.baz;")
        .unwrap();
    assert!(is_one_line_expression_export(&body));
}

#[test]
fn is_one_line_should_return_false_for_non_require_member_access() {
    let body = JsSourceParser
        .parse("module.exports = something.require(\"foo\");")
        .unwrap();
    assert!(!is_one_line_expression_export(&body));
}

// --- isStringBase64.spec.ts ---------------------------------------------------

#[test]
fn is_string_base64_matches_upstream_assertions() {
    let png_string = "iVBORw0KGgoAAAANSUhEUgAABQAAAALQAQMAAAD1s08VAAAAA1BMVEX/AAAZ4gk3AAAAh0lEQVR42u3BMQEAAADCoPVPbQlPoAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB4GsTfAAGc95RKAAAAAElFTkSuQmCC";
    let png_string_with_mime = format!("data:image/png;base64,{png_string}");
    let jpg_string = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wCEACAhITMkM1EwMFFCLy8vQiccHBwcJyIXFxcXFyIRDAwMDAwMEQwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwBIjMzNCY0IhgYIhQODg4UFA4ODg4UEQwMDAwMEREMDAwMDAwRDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDP/AABEIAAYABgMBIgACEQEDEQH/xABVAAEBAAAAAAAAAAAAAAAAAAAAAxAAAQQCAwEAAAAAAAAAAAAAAgABAxQEIxIkMxMBAQAAAAAAAAAAAAAAAAAAAAARAQAAAAAAAAAAAAAAAAAAAAD/2gAMAwEAAhEDEQA/AIE7MwkbOUJDJWx+ZjXATitx2/h2bEWvX5Y0npQ7aIiD/9k=";
    let jpg_string_with_mime = format!("data:image/jpeg;base64,{jpg_string}");

    let no_opts = Base64Options::default();
    let allow_mime = Base64Options {
        allow_mime: Some(true),
        ..Default::default()
    };
    let mime_required = Base64Options {
        mime_required: Some(true),
        ..Default::default()
    };

    assert!(is_string_base64(png_string, no_opts));
    assert!(!is_string_base64(&png_string_with_mime, no_opts));
    assert!(is_string_base64(&png_string_with_mime, allow_mime));
    assert!(!is_string_base64(png_string, mime_required));
    assert!(is_string_base64(&png_string_with_mime, mime_required));
    assert!(is_string_base64(jpg_string, no_opts));
    assert!(!is_string_base64(&jpg_string_with_mime, no_opts));
    assert!(is_string_base64(&jpg_string_with_mime, allow_mime));

    let create_mime_string = |mime: &str| format!("data:{mime};base64,{png_string}");

    assert!(is_string_base64(
        &create_mime_string("application/vnd.apple.installer+xml"),
        allow_mime
    ));
    assert!(is_string_base64(
        &create_mime_string("image/svg+xml"),
        allow_mime
    ));
    assert!(is_string_base64(
        &create_mime_string("application/set-payment-initiation"),
        allow_mime
    ));
    assert!(is_string_base64(
        &create_mime_string("image/vnd.adobe.photoshop"),
        allow_mime
    ));

    assert!(!is_string_base64("1342234", no_opts));
    assert!(!is_string_base64("afQ$%rfew", no_opts));
    assert!(!is_string_base64("dfasdfr342", no_opts));
    assert!(!is_string_base64("uuLMhh", no_opts));
    assert!(is_string_base64(
        "uuLMhh",
        Base64Options {
            padding_required: Some(false),
            ..Default::default()
        }
    ));
    assert!(!is_string_base64(
        "uuLMhh",
        Base64Options {
            padding_required: Some(true),
            ..Default::default()
        }
    ));
    assert!(is_string_base64("uuLMhh==", no_opts));
    assert!(is_string_base64(
        "uuLMhh==",
        Base64Options {
            padding_required: Some(false),
            ..Default::default()
        }
    ));
    assert!(is_string_base64(
        "uuLMhh==",
        Base64Options {
            padding_required: Some(true),
            ..Default::default()
        }
    ));
    assert!(!is_string_base64(
        "data:image/png;base64,uuLMhh==",
        Base64Options {
            padding_required: Some(true),
            ..Default::default()
        }
    ));
    assert!(is_string_base64(
        "data:image/png;base64,uuLMhh==",
        Base64Options {
            padding_required: Some(true),
            allow_mime: Some(true),
            ..Default::default()
        }
    ));
    assert!(is_string_base64("", no_opts));
    assert!(!is_string_base64(
        "",
        Base64Options {
            allow_empty: Some(false),
            ..Default::default()
        }
    ));
}

// --- isSvg.spec.ts -------------------------------------------------------------

#[test]
fn is_svg_should_return_true_for_an_html_svg_element() {
    let svg_html = r##"<svg xmlns="http://www.w3.org/2000/svg"
          width="150" height="100" viewBox="0 0 3 2">

          <rect width="1" height="2" x="0" fill="#008d46" />
          <rect width="1" height="2" x="1" fill="#ffffff" />
          <rect width="1" height="2" x="2" fill="#d2232c" />
      </svg>"##;
    assert!(is_svg(&json!(svg_html)));
}

#[test]
fn is_svg_should_return_true_for_an_svg_path_string() {
    assert!(is_svg(&json!("M150 0 L75 200 L225 200 Z")));
}

#[test]
fn is_svg_should_return_false_for_invalid_xml_string() {
    assert!(!is_svg(&json!("</a>")));
}

#[test]
fn is_svg_path_should_return_true_for_a_valid_svg_path() {
    assert!(is_svg_path("M150 0 L75 200 L225 200 Z"));
}

#[test]
fn is_svg_path_should_return_false_for_an_svg_path_shorter_than_4_characters() {
    assert!(!is_svg_path("M150"));
}

#[test]
fn is_svg_path_should_return_false_for_a_non_svg_path_string() {
    assert!(!is_svg_path("hello world!"));
}

// Upstream also asserts `isSvgPath(10)` returns false; omitted since
// `is_svg_path` takes `&str`, and passing a non-string is a compile error.

// --- makePrefixRemover.spec.ts ------------------------------------------------

#[test]
fn make_prefix_remover_returns_the_original_string_when_no_dot_is_present() {
    let strip = make_prefix_remover(vec!["window".to_string()]);
    assert_eq!(strip("foo"), "foo");
}

#[test]
fn make_prefix_remover_returns_the_original_string_when_the_identifier_is_not_at_the_start() {
    let strip = make_prefix_remover(vec!["window".to_string()]);
    assert_eq!(strip("foo.window"), "foo.window");
}

#[test]
fn make_prefix_remover_removes_a_matching_prefix_at_the_start_of_the_expression() {
    let strip = make_prefix_remover(vec!["window".to_string(), "globalThis".to_string()]);
    assert_eq!(strip("window.bar"), "bar");
    assert_eq!(strip("globalThis.console"), "console");
}

#[test]
fn make_prefix_remover_returns_the_original_string_when_no_prefix_matches() {
    let strip = make_prefix_remover(vec!["window".to_string()]);
    assert_eq!(strip("document.title"), "document.title");
}

#[test]
fn make_prefix_remover_handles_nested_member_expressions() {
    let strip = make_prefix_remover(vec!["window".to_string()]);
    assert_eq!(strip("window.document.title"), "document.title");
}

#[test]
fn make_prefix_remover_accepts_any_iterable_of_prefixes() {
    // Adapted: the Rust API takes `Vec<String>` rather than an arbitrary
    // `Iterable<string>`, so a `HashSet` is collected into a `Vec` first.
    let prefixes: std::collections::HashSet<String> = ["window".to_string()].into_iter().collect();
    let strip = make_prefix_remover(prefixes.into_iter().collect());
    assert_eq!(strip("window.location"), "location");
}

#[test]
fn make_prefix_remover_uses_the_first_matching_prefix_based_on_input_order() {
    let strip1 = make_prefix_remover(vec!["window.document".to_string(), "window".to_string()]);
    assert_eq!(strip1("window.document.title"), "title");
    let strip2 = make_prefix_remover(vec!["window".to_string(), "window.document".to_string()]);
    assert_eq!(strip2("window.document.title"), "document.title");
}

// --- notNullOrUndefined.spec.ts -----------------------------------------------
//
// Omitted entirely: upstream's `notNullOrUndefined` is a null/undefined
// runtime guard with no ported counterpart — Rust's `Option<T>` makes the
// null/undefined distinction a compile-time property instead, so there is
// no function to port these assertions against.

// --- patterns.spec.ts ---------------------------------------------------------

#[test]
fn common_string_prefix_must_return_null_for_two_strings_that_have_no_common_prefix() {
    assert_eq!(common_string_prefix_str("boo", "foo"), None);
}

#[test]
fn common_string_prefix_should_return_the_common_prefix_for_strings_with_a_shared_prefix() {
    assert_eq!(
        common_string_prefix_str("bromance", "brother"),
        Some("bro".to_string())
    );
}

#[test]
fn common_string_suffix_must_return_the_common_suffix_for_the_two_strings_with_a_shared_suffix() {
    assert_eq!(common_string_suffix("boo", "foo"), Some("oo".to_string()));
}

#[test]
fn common_string_suffix_must_return_null_for_two_strings_with_no_common_suffix() {
    assert_eq!(common_string_suffix("bromance", "brother"), None);
}

// Upstream also asserts `commonHexadecimalPrefix(10)` throws a `TypeError`;
// omitted since `common_hexadecimal_prefix` takes `&[String]`, and passing
// a non-array is a compile error.

#[test]
fn common_hexadecimal_prefix_should_handle_only_hexadecimal_identifiers() {
    let data = [
        "_0x3c0c55",
        "_0x1185d5",
        "_0x160fc8",
        "_0x18a66f",
        "_0x18a835",
        "_0x1a8356",
        "_0x1adf3b",
        "_0x1e4510",
        "_0x1e9a2a",
        "_0x215558",
        "_0x2b0194",
        "_0x2fffe5",
        "_0x32c822",
        "_0x33bb79",
    ]
    .map(String::from);

    let result = common_hexadecimal_prefix(&data);

    assert_eq!(result.one_time_occurence, 0);
    assert_eq!(result.prefix.get("_0x"), Some(&data.len()));
}

#[test]
fn common_hexadecimal_prefix_should_add_one_non_hexadecimal_identifier() {
    let data = [
        "_0x3c0c55",
        "_0x1185d5",
        "_0x160fc8",
        "_0x18a66f",
        "_0x18a835",
        "_0x1a8356",
        "_0x1adf3b",
        "_0x1e4510",
        "_0x1e9a2a",
        "_0x215558",
        "_0x2b0194",
        "_0x2fffe5",
        "_0x32c822",
        "_0x33bb79",
        "foo",
    ]
    .map(String::from);

    let result = common_hexadecimal_prefix(&data);

    assert_eq!(result.one_time_occurence, 1);
    assert_eq!(result.prefix.get("_0x"), Some(&(data.len() - 1)));
}

// --- stringSuspicionScore.spec.ts ---------------------------------------------

#[test]
fn string_char_diversity_should_return_the_number_of_unique_characters_in_a_string() {
    assert_eq!(string_char_diversity("helloo!", &[]), 5);
}

#[test]
fn string_char_diversity_should_exclude_specified_characters_when_counting_unique_characters() {
    assert_eq!(string_char_diversity("- - -\n", &['\n']), 2);
}

#[test]
fn string_suspicion_score_should_return_0_for_strings_shorter_than_45_characters() {
    let mut seed = 0xabcd_ef01_2345_6789;
    for str_size in 1..45usize {
        let random_str = random_hex_chars(str_size, &mut seed);
        assert_eq!(string_suspicion_score(&random_str), 0);
    }
}

#[test]
fn string_suspicion_score_should_return_1_for_strings_between_45_and_200_with_no_spaces() {
    let mut seed = 0x1357_9bdf_2468_ace0;
    let random_str_with_no_spaces = random_hex_chars(50, &mut seed);
    assert_eq!(string_suspicion_score(&random_str_with_no_spaces), 1);
}

#[test]
fn string_suspicion_score_should_return_0_for_strings_between_45_and_200_with_a_space_in_first_45()
{
    let mut seed = 0x2468_ace0_1357_9bdf;
    let random_str_with_spaces = format!(
        "{} -_- {}",
        random_hex_chars(20, &mut seed),
        random_hex_chars(60, &mut seed)
    );
    assert_eq!(string_suspicion_score(&random_str_with_spaces), 0);
}

#[test]
fn string_suspicion_score_should_return_2_for_strings_longer_than_200_with_no_spaces() {
    let mut seed = 0x0f0f_0f0f_0f0f_0f0f;
    let random_str = random_hex_chars(400, &mut seed);
    assert_eq!(string_suspicion_score(&random_str), 2);
}

#[test]
fn string_suspicion_score_should_add_2_when_the_string_has_more_than_70_unique_characters() {
    let random_str = "૱꠸┯┰┱┲❗►◄Ăă0123456789ᶀᶁᶂᶃᶄᶆᶇᶈᶉᶊᶋᶌᶍᶎᶏᶐᶑᶒᶓᶔᶕᶖᶗᶘᶙᶚᶸᵯᵰᵴᵶᵹᵼᵽᵾᵿ⤢⤣⤤⤥⥆⥇™°×π±√ ";
    assert_eq!(string_suspicion_score(random_str), 3);
}

// --- stripNodePrefix.spec.ts ---------------------------------------------------

#[test]
fn strip_node_prefix_should_remove_node_prefix_from_module_name() {
    assert_eq!(strip_node_prefix("node:fs"), "fs");
    assert_eq!(strip_node_prefix("node:path"), "path");
}

#[test]
fn strip_node_prefix_should_return_the_value_unchanged_if_no_prefix_is_present() {
    assert_eq!(strip_node_prefix("fs"), "fs");
    assert_eq!(strip_node_prefix("http"), "http");
}

#[test]
fn strip_node_prefix_should_not_modify_similar_but_invalid_prefixes() {
    assert_eq!(strip_node_prefix("nod:fs"), "nod:fs");
}

#[test]
fn strip_node_prefix_should_only_remove_prefix_at_the_beginning() {
    assert_eq!(strip_node_prefix("my-node:fs"), "my-node:fs");
}

#[test]
fn strip_node_prefix_should_handle_empty_string() {
    assert_eq!(strip_node_prefix(""), "");
}

// Upstream also asserts numbers/null/undefined/objects pass through
// unchanged; omitted since `strip_node_prefix` takes `&str`, and the Rust
// type system already makes those inputs uncallable.

// --- toArrayLocation.spec.ts ---------------------------------------------------

#[test]
fn to_array_location_should_convert_a_valid_source_location_to_array_format() {
    let location = SourceLocation {
        start: Position { line: 1, column: 2 },
        end: Position { line: 3, column: 4 },
    };
    assert_eq!(to_array_location(Some(location)), [[1, 2], [3, 4]]);
}

#[test]
fn to_array_location_should_default_to_root_location_when_no_argument_is_provided() {
    assert_eq!(to_array_location(None), [[0, 0], [0, 0]]);
}

// Upstream also covers "end defaults to start when omitted", "line/column
// fall back to 0 when undefined", and "mixed defined/undefined fields":
// all three rely on a JS location object whose `start`/`end`/`line`/`column`
// are individually optional at runtime. The Rust `SourceLocation`/`Position`
// types make `start`, `end`, `line` and `column` all mandatory `u64` fields,
// so those partially-populated inputs have no representable value to pass
// to `to_array_location` — the per-field runtime fallback they exercise
// isn't part of the ported (strongly-typed) surface. Omitted.
