//! Upstream: `test/CollectableSet.spec.ts`, `test/ShadyLink.spec.ts`.

use js_x_ray::collectable_set::{
    CollectableEntry, CollectableLocation, CollectableSetData, CollectableSetRegistry,
    DefaultCollectableSet,
};
use js_x_ray::estree::{Position, SourceLocation};
use js_x_ray::shady_link::{IsUrlSafeOptions, ShadyLink, ShadyLinkResult};
use serde_json::Map;

fn spec_metadata() -> Map<String, serde_json::Value> {
    serde_json::json!({ "spec": "react@19.0.1" })
        .as_object()
        .expect("object literal")
        .clone()
}

mod default_collectable_set {
    use super::*;

    mod values {
        use super::*;

        #[test]
        fn should_get_all_the_values() {
            let mut set = DefaultCollectableSet::new("url");
            set.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );
            set.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[5, 5], [7, 8]],
                Some(spec_metadata()),
            );
            set.add(
                "https://example.com",
                Some("other.js".to_owned()),
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );
            set.add(
                "https://other.com",
                None,
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );

            assert_eq!(
                set.values().collect::<Vec<_>>(),
                vec!["https://example.com", "https://other.com"]
            );
        }
    }

    mod to_json {
        use super::*;

        #[test]
        fn should_return_a_serializable_snapshot_with_type_and_entries() {
            let mut set = DefaultCollectableSet::new("url");
            set.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );
            set.add("https://example.com", None, [[1, 0], [1, 10]], None);

            let data = set.to_json();

            assert_eq!(data.r#type, "url");
            assert_eq!(
                data.entries,
                vec![CollectableEntry {
                    value: "https://example.com".to_owned(),
                    locations: vec![
                        CollectableLocation {
                            file: Some("str.js".to_owned()),
                            location: vec![[[0, 0], [0, 0]]],
                            metadata: Some(spec_metadata()),
                        },
                        CollectableLocation {
                            file: None,
                            location: vec![[[1, 0], [1, 10]]],
                            metadata: None,
                        },
                    ],
                }]
            );
        }

        #[test]
        fn should_produce_output_compatible_with_json_stringify_json_parse_round_trip() {
            let mut set = DefaultCollectableSet::new("dependency");
            set.add(
                "lodash",
                Some("index.js".to_owned()),
                [[3, 0], [3, 20]],
                None,
            );

            let json = serde_json::to_string(&set.to_json()).expect("serialize");
            let parsed: CollectableSetData = serde_json::from_str(&json).expect("deserialize");

            assert_eq!(parsed, set.to_json());
        }
    }

    mod from_json {
        use super::*;

        #[test]
        fn should_reconstruct_a_default_collectable_set_equal_to_the_original() {
            let mut original = DefaultCollectableSet::new("url");
            original.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );
            original.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[5, 5], [7, 8]],
                Some(spec_metadata()),
            );
            original.add(
                "https://example.com",
                Some("other.js".to_owned()),
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );
            original.add(
                "https://other.com",
                None,
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );

            let restored = DefaultCollectableSet::from_json(&original.to_json());

            // Adapted: upstream compares `Array.from(restored)` against
            // `Array.from(original)` (the `Symbol.iterator` snapshot); the
            // Rust equivalent snapshot is `to_json().entries`.
            assert_eq!(restored.to_json().entries, original.to_json().entries);
            assert_eq!(restored.r#type, original.r#type);
        }

        #[test]
        fn should_survive_a_json_stringify_json_parse_round_trip() {
            let mut original = DefaultCollectableSet::new("hostname");
            original.add("example.com", None, [[0, 0], [0, 5]], None);

            let json = serde_json::to_string(&original.to_json()).expect("serialize");
            let data: CollectableSetData = serde_json::from_str(&json).expect("deserialize");
            let restored = DefaultCollectableSet::from_json(&data);

            assert_eq!(restored.to_json().entries, original.to_json().entries);
        }
    }

    mod merge_data {
        use super::*;

        #[test]
        fn should_return_the_same_set_instance_that_was_passed_in() {
            let mut set = DefaultCollectableSet::new("url");
            let data = set.to_json();

            // Adapted: Rust's `merge_data` takes `&mut self` and mutates in
            // place instead of upstream's static method that returns the
            // passed-in set for chaining, so there is no separate return
            // value whose identity to check. This instead confirms merging
            // an empty snapshot into itself is a true no-op.
            set.merge_data(&data);

            assert_eq!(set.to_json(), data);
        }

        #[test]
        fn should_populate_an_empty_set_from_collectable_set_data() {
            let mut set = DefaultCollectableSet::new("url");
            let data = CollectableSetData {
                r#type: "url".to_owned(),
                entries: vec![CollectableEntry {
                    value: "https://example.com".to_owned(),
                    locations: vec![CollectableLocation {
                        file: Some("str.js".to_owned()),
                        location: vec![[[0, 0], [0, 10]]],
                        metadata: Some(spec_metadata()),
                    }],
                }],
            };

            set.merge_data(&data);

            assert_eq!(set.to_json().entries, data.entries);
        }

        #[test]
        fn should_accumulate_into_a_set_that_already_has_entries() {
            let mut set = DefaultCollectableSet::new("hostname");
            set.add(
                "example.com",
                Some("a.js".to_owned()),
                [[0, 0], [0, 11]],
                None,
            );

            set.merge_data(&CollectableSetData {
                r#type: "hostname".to_owned(),
                entries: vec![CollectableEntry {
                    value: "other.com".to_owned(),
                    locations: vec![CollectableLocation {
                        file: Some("b.js".to_owned()),
                        location: vec![[[1, 0], [1, 9]]],
                        metadata: None,
                    }],
                }],
            });

            assert_eq!(
                set.values().collect::<Vec<_>>(),
                vec!["example.com", "other.com"]
            );
        }

        #[test]
        fn should_expand_multiple_locations_within_a_single_location_entry() {
            let mut set = DefaultCollectableSet::new("url");

            set.merge_data(&CollectableSetData {
                r#type: "url".to_owned(),
                entries: vec![CollectableEntry {
                    value: "https://example.com".to_owned(),
                    locations: vec![CollectableLocation {
                        file: Some("str.js".to_owned()),
                        location: vec![[[0, 0], [0, 10]], [[5, 0], [5, 10]]],
                        metadata: None,
                    }],
                }],
            });

            assert_eq!(
                set.to_json().entries,
                vec![CollectableEntry {
                    value: "https://example.com".to_owned(),
                    locations: vec![
                        CollectableLocation {
                            file: Some("str.js".to_owned()),
                            location: vec![[[0, 0], [0, 10]]],
                            metadata: None,
                        },
                        CollectableLocation {
                            file: Some("str.js".to_owned()),
                            location: vec![[[5, 0], [5, 10]]],
                            metadata: None,
                        },
                    ],
                }]
            );
        }

        #[test]
        fn should_handle_entries_without_metadata() {
            let mut set = DefaultCollectableSet::new("dependency");
            let data = CollectableSetData {
                r#type: "dependency".to_owned(),
                entries: vec![CollectableEntry {
                    value: "lodash".to_owned(),
                    locations: vec![CollectableLocation {
                        file: None,
                        location: vec![[[3, 0], [3, 20]]],
                        metadata: None,
                    }],
                }],
            };

            set.merge_data(&data);

            assert_eq!(set.to_json().entries, data.entries);
        }

        #[test]
        fn should_handle_an_empty_entries_array_without_error() {
            let mut set = DefaultCollectableSet::new("ip");

            set.merge_data(&CollectableSetData {
                r#type: "ip".to_owned(),
                entries: vec![],
            });

            assert!(set.to_json().entries.is_empty());
        }

        #[test]
        fn should_be_compatible_with_to_json_output_round_trip_via_merge_data() {
            let mut original = DefaultCollectableSet::new("url");
            original.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[0, 0], [0, 10]],
                Some(spec_metadata()),
            );
            original.add("https://other.com", None, [[2, 0], [2, 18]], None);

            let mut target = DefaultCollectableSet::new("url");
            target.merge_data(&original.to_json());

            assert_eq!(target.to_json().entries, original.to_json().entries);
        }
    }

    mod add {
        use super::*;

        #[test]
        fn should_get_the_type_of_the_given_collectable_set() {
            let set = DefaultCollectableSet::new("url");
            assert_eq!(set.r#type, "url");
        }

        #[test]
        fn should_be_able_to_add_a_value() {
            let mut set = DefaultCollectableSet::new("url");
            set.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );
            set.add(
                "https://example.com",
                Some("str.js".to_owned()),
                [[5, 5], [7, 8]],
                Some(spec_metadata()),
            );
            set.add(
                "https://example.com",
                Some("other.js".to_owned()),
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );
            set.add(
                "https://other.com",
                None,
                [[0, 0], [0, 0]],
                Some(spec_metadata()),
            );

            assert_eq!(
                set.to_json().entries,
                vec![
                    CollectableEntry {
                        value: "https://example.com".to_owned(),
                        locations: vec![
                            CollectableLocation {
                                file: Some("str.js".to_owned()),
                                location: vec![[[0, 0], [0, 0]]],
                                metadata: Some(spec_metadata()),
                            },
                            CollectableLocation {
                                file: Some("str.js".to_owned()),
                                location: vec![[[5, 5], [7, 8]]],
                                metadata: Some(spec_metadata()),
                            },
                            CollectableLocation {
                                file: Some("other.js".to_owned()),
                                location: vec![[[0, 0], [0, 0]]],
                                metadata: Some(spec_metadata()),
                            },
                        ],
                    },
                    CollectableEntry {
                        value: "https://other.com".to_owned(),
                        locations: vec![CollectableLocation {
                            file: None,
                            location: vec![[[0, 0], [0, 0]]],
                            metadata: Some(spec_metadata()),
                        }],
                    },
                ]
            );
        }
    }
}

const SAFE: ShadyLinkResult = ShadyLinkResult {
    safe: true,
    is_local_address: false,
};
const UNSAFE: ShadyLinkResult = ShadyLinkResult {
    safe: false,
    is_local_address: false,
};
const LOCAL: ShadyLinkResult = ShadyLinkResult {
    safe: false,
    is_local_address: true,
};

fn fresh_registry() -> CollectableSetRegistry {
    CollectableSetRegistry::new(vec![
        DefaultCollectableSet::new("url"),
        DefaultCollectableSet::new("hostname"),
        DefaultCollectableSet::new("ip"),
    ])
}

fn is_safe(input: &str) -> ShadyLinkResult {
    let mut registry = fresh_registry();
    ShadyLink::is_url_safe(
        input,
        IsUrlSafeOptions {
            collectable_set_registry: &mut registry,
            file: None,
            location: None,
            metadata: None,
        },
    )
}

mod shady_link_is_url_safe {
    use super::*;

    mod when_input_is_not_a_valid_url {
        use super::*;

        #[test]
        fn should_return_true_for_an_invalid_url() {
            assert_eq!(is_safe("not-a-url"), SAFE);
        }

        #[test]
        fn should_return_true_for_an_empty_string() {
            assert_eq!(is_safe(""), SAFE);
        }

        #[test]
        fn should_return_true_for_a_valid_url_but_with_unknown_protocol() {
            assert_eq!(is_safe("unknown://example.com"), SAFE);
        }

        #[test]
        fn should_return_true_for_a_malformed_url() {
            assert_eq!(is_safe("http://"), SAFE);
        }
    }

    mod when_url_contains_an_ip_address {
        use super::*;

        mod private_ip_addresses {
            use super::*;

            #[test]
            fn should_return_false_for_localhost_ipv4() {
                assert_eq!(is_safe("https://127.0.0.1/path"), LOCAL);
            }

            #[test]
            fn should_return_false_for_private_ipv4_10_x_x_x() {
                assert_eq!(is_safe("https://10.0.0.1/path"), LOCAL);
            }

            #[test]
            fn should_return_false_for_private_ipv4_192_168_x_x() {
                assert_eq!(is_safe("https://192.168.1.1/path"), LOCAL);
            }

            #[test]
            fn should_return_false_for_private_ipv4_172_16_x_x() {
                assert_eq!(is_safe("https://172.16.0.1/path"), LOCAL);
            }

            #[test]
            fn should_return_false_for_ipv6_loopback_address() {
                assert_eq!(is_safe("https://[::1]/path"), LOCAL);
            }

            #[test]
            fn should_return_false_for_ipv6_unspecified_address() {
                assert_eq!(is_safe("https://[::]/path"), LOCAL);
            }

            #[test]
            fn should_return_false_for_ipv4_mapped_ipv6_private_address() {
                assert_eq!(is_safe("https://[::ffff:127.0.0.1]/path"), LOCAL);
            }

            #[test]
            fn should_return_false_for_ipv4_mapped_ipv6_private_address_192_168_x_x() {
                assert_eq!(is_safe("https://[::ffff:192.168.1.1]/path"), LOCAL);
            }
        }

        mod public_ip_addresses {
            use super::*;

            #[test]
            fn should_return_false_for_public_ipv4_with_http() {
                assert_eq!(is_safe("http://8.8.8.8/path"), UNSAFE);
            }

            #[test]
            fn should_return_true_for_public_ipv4_with_https() {
                assert_eq!(is_safe("https://8.8.8.8/path"), SAFE);
            }

            #[test]
            fn should_return_false_for_public_ipv6_with_http() {
                assert_eq!(is_safe("http://[2001:4860:4860::8888]/path"), UNSAFE);
            }

            #[test]
            fn should_return_true_for_public_ipv6_with_https() {
                assert_eq!(is_safe("https://[2001:4860:4860::8888]/path"), SAFE);
            }
        }
    }

    mod when_url_scheme_is_not_https {
        use super::*;

        #[test]
        fn should_return_false_for_http_url() {
            assert_eq!(is_safe("http://example.com"), UNSAFE);
        }

        #[test]
        fn should_return_false_for_ftp_url() {
            assert_eq!(is_safe("ftp://example.com"), UNSAFE);
        }
    }

    mod when_url_matches_shady_link_patterns {
        use super::*;

        mod known_shady_domains {
            use super::*;

            #[test]
            fn should_return_false_for_bit_ly() {
                assert_eq!(is_safe("https://bit.ly/abc123"), UNSAFE);
            }

            #[test]
            fn should_return_false_for_ipinfo_io() {
                assert_eq!(is_safe("https://ipinfo.io/json"), UNSAFE);
            }

            #[test]
            fn should_return_false_for_httpbin_org() {
                assert_eq!(is_safe("https://httpbin.org/get"), UNSAFE);
            }

            #[test]
            fn should_return_false_for_api_ipify_org() {
                assert_eq!(is_safe("https://api.ipify.org"), UNSAFE);
            }
        }

        mod suspicious_tlds {
            use super::*;

            macro_rules! tld_is_unsafe {
                ($name:ident, $domain:literal) => {
                    #[test]
                    fn $name() {
                        assert_eq!(is_safe(concat!("https://malicious.", $domain)), UNSAFE);
                    }
                };
            }

            tld_is_unsafe!(should_return_false_for_link_tld, "link");
            tld_is_unsafe!(should_return_false_for_xyz_tld, "xyz");
            tld_is_unsafe!(should_return_false_for_tk_tld, "tk");
            tld_is_unsafe!(should_return_false_for_ml_tld, "ml");
            tld_is_unsafe!(should_return_false_for_ga_tld, "ga");
            tld_is_unsafe!(should_return_false_for_cf_tld, "cf");
            tld_is_unsafe!(should_return_false_for_gq_tld, "gq");
            tld_is_unsafe!(should_return_false_for_pw_tld, "pw");
            tld_is_unsafe!(should_return_false_for_top_tld, "top");
            tld_is_unsafe!(should_return_false_for_club_tld, "club");
            tld_is_unsafe!(should_return_false_for_mw_tld, "mw");
            tld_is_unsafe!(should_return_false_for_bd_tld, "bd");
            tld_is_unsafe!(should_return_false_for_ke_tld, "ke");
            tld_is_unsafe!(should_return_false_for_am_tld, "am");
            tld_is_unsafe!(should_return_false_for_sbs_tld, "sbs");
            tld_is_unsafe!(should_return_false_for_date_tld, "date");
            tld_is_unsafe!(should_return_false_for_quest_tld, "quest");
            tld_is_unsafe!(should_return_false_for_cd_tld, "cd");
            tld_is_unsafe!(should_return_false_for_bid_tld, "bid");
            tld_is_unsafe!(should_return_false_for_ws_tld, "ws");
            tld_is_unsafe!(should_return_false_for_icu_tld, "icu");
            tld_is_unsafe!(should_return_false_for_cam_tld, "cam");
            tld_is_unsafe!(should_return_false_for_uno_tld, "uno");
            tld_is_unsafe!(should_return_false_for_email_tld, "email");
            tld_is_unsafe!(should_return_false_for_stream_tld, "stream");
        }
    }

    mod when_url_is_safe {
        use super::*;

        #[test]
        fn should_return_true_for_a_standard_https_url() {
            assert_eq!(is_safe("https://example.com"), SAFE);
        }

        #[test]
        fn should_return_true_for_a_https_url_with_path() {
            assert_eq!(is_safe("https://example.com/path/to/resource"), SAFE);
        }

        #[test]
        fn should_return_true_for_a_https_url_with_query_params() {
            assert_eq!(is_safe("https://example.com?foo=bar"), SAFE);
        }

        #[test]
        fn should_return_true_for_npm_registry_url() {
            assert_eq!(is_safe("https://registry.npmjs.org/package"), SAFE);
        }

        #[test]
        fn should_return_true_for_github_url() {
            assert_eq!(is_safe("https://github.com/NodeSecure/js-x-ray"), SAFE);
        }

        #[test]
        fn should_return_true_for_com_tld() {
            assert_eq!(is_safe("https://safe-website.com"), SAFE);
        }

        #[test]
        fn should_return_true_for_org_tld() {
            assert_eq!(is_safe("https://safe-website.org"), SAFE);
        }

        #[test]
        fn should_return_true_for_io_tld_not_in_shady_list() {
            assert_eq!(is_safe("https://safe-website.io"), SAFE);
        }
    }

    mod data_collecting {
        use super::*;

        #[test]
        fn should_not_collect_anything_when_the_url_is_a_real_url() {
            let mut registry = fresh_registry();
            ShadyLink::is_url_safe(
                "not-a-url",
                IsUrlSafeOptions {
                    collectable_set_registry: &mut registry,
                    file: None,
                    location: None,
                    metadata: None,
                },
            );

            assert!(
                registry
                    .get("url")
                    .expect("url set")
                    .to_json()
                    .entries
                    .is_empty()
            );
            assert!(
                registry
                    .get("hostname")
                    .expect("hostname set")
                    .to_json()
                    .entries
                    .is_empty()
            );
            assert!(
                registry
                    .get("ip")
                    .expect("ip set")
                    .to_json()
                    .entries
                    .is_empty()
            );
        }

        #[test]
        fn should_collect_the_url_and_the_hostname() {
            let mut registry = fresh_registry();
            let metadata = spec_metadata();
            ShadyLink::is_url_safe(
                "https://example.com",
                IsUrlSafeOptions {
                    collectable_set_registry: &mut registry,
                    file: None,
                    location: None,
                    metadata: Some(&metadata),
                },
            );

            assert_eq!(
                registry.get("url").expect("url set").to_json().entries,
                vec![CollectableEntry {
                    value: "https://example.com/".to_owned(),
                    locations: vec![CollectableLocation {
                        file: None,
                        location: vec![[[0, 0], [0, 0]]],
                        metadata: Some(spec_metadata()),
                    }],
                }]
            );
            assert_eq!(
                registry
                    .get("hostname")
                    .expect("hostname set")
                    .to_json()
                    .entries,
                vec![CollectableEntry {
                    value: "example.com".to_owned(),
                    locations: vec![CollectableLocation {
                        file: None,
                        location: vec![[[0, 0], [0, 0]]],
                        metadata: Some(spec_metadata()),
                    }],
                }]
            );
            assert!(
                registry
                    .get("ip")
                    .expect("ip set")
                    .to_json()
                    .entries
                    .is_empty()
            );
        }

        #[test]
        fn should_collect_the_url_and_the_ip() {
            let mut registry = fresh_registry();
            let metadata = spec_metadata();
            let location = SourceLocation {
                start: Position { line: 1, column: 0 },
                end: Position { line: 1, column: 0 },
            };
            ShadyLink::is_url_safe(
                "https://127.0.0.1/path",
                IsUrlSafeOptions {
                    collectable_set_registry: &mut registry,
                    file: Some("file.js"),
                    location: Some(location),
                    metadata: Some(&metadata),
                },
            );

            assert_eq!(
                registry.get("url").expect("url set").to_json().entries,
                vec![CollectableEntry {
                    value: "https://127.0.0.1/path".to_owned(),
                    locations: vec![CollectableLocation {
                        file: Some("file.js".to_owned()),
                        location: vec![[[1, 0], [1, 0]]],
                        metadata: Some(spec_metadata()),
                    }],
                }]
            );
            assert!(
                registry
                    .get("hostname")
                    .expect("hostname set")
                    .to_json()
                    .entries
                    .is_empty()
            );
            assert_eq!(
                registry.get("ip").expect("ip set").to_json().entries,
                vec![CollectableEntry {
                    value: "127.0.0.1".to_owned(),
                    locations: vec![CollectableLocation {
                        file: Some("file.js".to_owned()),
                        location: vec![[[1, 0], [1, 0]]],
                        metadata: Some(spec_metadata()),
                    }],
                }]
            );
        }
    }
}

mod shady_link_is_valid_ip_address {
    use super::*;

    #[test]
    fn should_be_a_valid_ip_address() {
        assert!(ShadyLink::is_valid_ip_address("127.0.0.1"));
    }

    #[test]
    fn should_not_be_a_valid_address() {
        assert!(!ShadyLink::is_valid_ip_address("127.0.0.1.1"));
        assert!(!ShadyLink::is_valid_ip_address("::"));
    }

    /// https://github.com/NodeSecure/js-x-ray/issues/474
    #[test]
    fn should_not_interpret_a_plain_integer_as_an_ip_address() {
        assert!(!ShadyLink::is_valid_ip_address("1"));
        assert!(!ShadyLink::is_valid_ip_address("12130706433"));
    }
}
