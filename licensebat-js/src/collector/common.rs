use futures::FutureExt;
use licensebat_core::{collector::RetrievedDependencyStream, Dependency, Retriever};
use std::sync::Arc;
// use tracing::instrument;

pub const NPM: &str = "npm";

// #[instrument(skip(deps, retriever))]
pub fn retrieve_from_npm<'a, I, R>(deps: I, retriever: &Arc<R>) -> RetrievedDependencyStream<'a>
where
    I: Iterator<Item = Dependency>,
    R: Retriever + 'a,
{
    deps.into_iter()
        .map(|dep| {
            retriever
                .get_dependency(&dep.name, &dep.version)
                .map(std::result::Result::unwrap) // TODO: this will never be not ok! so if'ts ok. consider removing the need of using this as a result.
                .boxed()
        })
        .collect()
}
