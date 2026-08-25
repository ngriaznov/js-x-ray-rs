//! Upstream: `src/VirtualVariableIdentifier.ts`

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::estree::SourceLocation;

struct State {
    counter: u64,
    id_to_locations: HashMap<String, Option<SourceLocation>>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| {
    Mutex::new(State {
        counter: 0,
        id_to_locations: HashMap::new(),
    })
});

pub struct VirtualVariableIdentifier;

impl VirtualVariableIdentifier {
    pub fn generate(name: &str, location: Option<SourceLocation>) -> String {
        let mut state = STATE.lock().expect("VirtualVariableIdentifier state poisoned");
        let virtual_id = format!("__virtual_{name}_{}__", state.counter);
        state.counter += 1;
        state.id_to_locations.insert(virtual_id.clone(), location);
        virtual_id
    }

    pub fn get_location(virtual_id: &str) -> Option<SourceLocation> {
        STATE
            .lock()
            .expect("VirtualVariableIdentifier state poisoned")
            .id_to_locations
            .get(virtual_id)
            .copied()
            .flatten()
    }

    pub fn reset() {
        let mut state = STATE.lock().expect("VirtualVariableIdentifier state poisoned");
        state.counter = 0;
        state.id_to_locations.clear();
    }
}
