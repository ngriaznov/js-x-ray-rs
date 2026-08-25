//! Upstream: `src/pipelines/` (index.ts, Runner.class.ts, inline.ts,
//! deobfuscate.ts).

mod deobfuscate;
mod inline;

pub use deobfuscate::Deobfuscate;
pub use inline::Inline;

use serde_json::Value;

/// Upstream: `interface Pipeline`.
pub trait Pipeline {
    fn name(&self) -> &'static str;

    /// Takes the Program body and returns the (possibly transformed) body.
    fn walk(&mut self, body: Vec<Value>) -> Vec<Value>;
}

/// Upstream: `PipelineRunner`.
pub struct PipelineRunner {
    pipelines: Vec<Box<dyn Pipeline>>,
}

impl PipelineRunner {
    pub fn new(pipelines: Vec<Box<dyn Pipeline>>) -> Self {
        Self {
            pipelines: remove_duplicated_pipelines(pipelines),
        }
    }

    pub fn reduce(&mut self, initial_body: Vec<Value>) -> Vec<Value> {
        self.pipelines
            .iter_mut()
            .fold(initial_body, |body, pipeline| pipeline.walk(body))
    }
}

fn remove_duplicated_pipelines(pipelines: Vec<Box<dyn Pipeline>>) -> Vec<Box<dyn Pipeline>> {
    let mut seen = indexmap::IndexSet::new();
    pipelines
        .into_iter()
        .filter(|pipeline| seen.insert(pipeline.name().to_owned()))
        .collect()
}
