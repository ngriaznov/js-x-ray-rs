//! Upstream: `src/CollectableSet.ts` and `src/CollectableSetRegistry.ts`

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::utils::SourceArrayLocation;

/// "url" | "hostname" | "ip" | "email" | "dependency" | custom
pub type CollectableType = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectableLocation {
    pub file: Option<String>,
    pub location: Vec<SourceArrayLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectableEntry {
    pub value: String,
    pub locations: Vec<CollectableLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectableSetData {
    #[serde(rename = "type")]
    pub r#type: CollectableType,
    pub entries: Vec<CollectableEntry>,
}

#[derive(Debug, Clone)]
struct Entry {
    file: Option<String>,
    location: SourceArrayLocation,
    metadata: Option<Map<String, Value>>,
}

/// Upstream: `DefaultCollectableSet`.
#[derive(Debug, Clone)]
pub struct DefaultCollectableSet {
    entries: IndexMap<String, Vec<Entry>>,
    pub r#type: CollectableType,
}

impl DefaultCollectableSet {
    pub fn new(r#type: impl Into<CollectableType>) -> Self {
        Self {
            entries: IndexMap::new(),
            r#type: r#type.into(),
        }
    }

    pub fn add(
        &mut self,
        value: &str,
        file: Option<String>,
        location: SourceArrayLocation,
        metadata: Option<Map<String, Value>>,
    ) {
        let entry = Entry {
            file,
            location,
            metadata,
        };
        self.entries.entry(value.to_owned()).or_default().push(entry);
    }

    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn to_json(&self) -> CollectableSetData {
        CollectableSetData {
            r#type: self.r#type.clone(),
            entries: self
                .entries
                .iter()
                .map(|(value, entries)| CollectableEntry {
                    value: value.clone(),
                    locations: entries
                        .iter()
                        .map(|entry| CollectableLocation {
                            file: entry.file.clone(),
                            location: vec![entry.location],
                            metadata: entry.metadata.clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn from_json(data: &CollectableSetData) -> Self {
        let mut set = Self::new(data.r#type.clone());
        set.merge_data(data);
        set
    }

    pub fn merge_data(&mut self, data: &CollectableSetData) {
        for CollectableEntry { value, locations } in &data.entries {
            for loc_entry in locations {
                for loc in &loc_entry.location {
                    self.add(
                        value,
                        loc_entry.file.clone(),
                        *loc,
                        loc_entry.metadata.clone(),
                    );
                }
            }
        }
    }
}

/// Upstream: `CollectableSetRegistry`.
#[derive(Debug, Default, Clone)]
pub struct CollectableSetRegistry {
    collectable_sets: IndexMap<CollectableType, DefaultCollectableSet>,
}

impl CollectableSetRegistry {
    pub fn new(collectable_sets: Vec<DefaultCollectableSet>) -> Self {
        let mut map = IndexMap::new();
        for set in collectable_sets {
            map.insert(set.r#type.clone(), set);
        }
        Self {
            collectable_sets: map,
        }
    }

    pub fn add(
        &mut self,
        r#type: &str,
        value: &str,
        file: Option<String>,
        location: SourceArrayLocation,
        metadata: Option<Map<String, Value>>,
    ) {
        if let Some(set) = self.collectable_sets.get_mut(r#type) {
            set.add(value, file, location, metadata.or_else(|| Some(Map::new())));
        }
    }

    pub fn has(&self, r#type: &str) -> bool {
        self.collectable_sets.contains_key(r#type)
    }

    pub fn get(&self, r#type: &str) -> Option<&DefaultCollectableSet> {
        self.collectable_sets.get(r#type)
    }
}
