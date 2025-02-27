use crate::retriever::npm_metadata::NpmMetadata;
use futures::{
    future::{self, BoxFuture},
    Future, FutureExt, TryFutureExt,
};
use licensebat_core::{Comment, Dependency, RetrievedDependency};
use reqwest::Client;
use serde_json::Value;
use tracing::instrument;

/// Trait used by the [`Npm`] struct to retrieve dependencies.
pub trait Retriever: Send + Sync + std::fmt::Debug {
    /// Future that resolves to a [`RetrievedDependency`].
    /// It cannot fail.
    type Response: Future<Output = RetrievedDependency> + Send;
    /// Validates dependency's information from the original source.
    fn get_dependency(&self, dep_name: &str, dep_version: &str) -> Self::Response;
}

#[derive(Debug, Clone)]
pub struct Npm {
    client: Client,
}

impl Default for Npm {
    /// Creates a new [`Retriever`].
    /// If you want to reuse a [`reqwest::Client`] pool consider using the [`with_client`] method.
    fn default() -> Self {
        Self::new(Client::new())
    }
}

impl Npm {
    /// Creates a new [`Retriever`] using the given [`reqwest::Client`].
    #[must_use]
    pub const fn new(client: Client) -> Self {
        Self { client }
    }
}

impl Retriever for Npm {
    type Response = BoxFuture<'static, RetrievedDependency>;

    /// Gets a dependency from NPM.
    /// This method attacks the npm api.
    #[instrument(skip(self), level = "debug")]
    fn get_dependency(&self, dep_name: &str, dep_version: &str) -> Self::Response {
        let url = format!("https://registry.npmjs.org/{}", dep_name);

        let dependency = Dependency {
            name: dep_name.to_string(),
            version: dep_version.to_string(),
        };
        let dep_clone = dependency.clone();
        let dependency_version = dep_version.to_string();

        self.client
            .get(&url)
            .send()
            .and_then(reqwest::Response::json)
            .map_ok(|metadata: Value| {
                // get general license
                let license = metadata["license"].clone();
                // get info from specific version
                let version = metadata["versions"][dependency_version].clone();
                serde_json::from_value::<NpmMetadata>(version)
                    .ok()
                    .and_then(|mut md| {
                        if md.license.is_none() {
                            // use generic if no license is found in the version
                            md.license = match license {
                                Value::String(lic) => Some(lic),
                                Value::Object(lic) => lic
                                    .get("type")
                                    .and_then(serde_json::Value::as_str)
                                    .map(std::borrow::ToOwned::to_owned),
                                _ => None,
                            }
                        }
                        md.get_licenses()
                    })
            })
            .map_ok(move |licenses: Option<Vec<String>>| {
                build_retrieved_dependency(&dep_clone, licenses, None)
            })
            .or_else(move |e| future::ok(build_retrieved_dependency(&dependency, None, Some(e))))
            .map(std::result::Result::<RetrievedDependency, std::convert::Infallible>::unwrap)
            .boxed()
    }
}

fn build_retrieved_dependency(
    dependency: &Dependency,
    licenses: Option<Vec<String>>,
    error: Option<reqwest::Error>,
) -> RetrievedDependency {
    let url = format!(
        "https://www.npmjs.com/package/{}/v/{}",
        dependency.name, dependency.version
    );

    let has_licenses = licenses.is_some();

    RetrievedDependency {
        name: dependency.name.clone(),
        version: dependency.version.clone(),
        url: Some(url),
        dependency_type: "npm".to_owned(),
        validated: false,
        is_valid: has_licenses && error.is_none(),
        is_ignored: false,
        error: if let Some(err) = error {
            Some(err.to_string())
        } else if has_licenses {
            None
        } else {
            Some("No License".to_owned())
        },
        licenses: if has_licenses {
            licenses
        } else {
            Some(vec!["NO-LICENSE".to_string()])
        },
        comment: if has_licenses {
            None
        } else {
            Some(Comment::removable("Consider **ignoring** this specific dependency. You can also accept the **NO-LICENSE** key to avoid these issues."))
        },
    }
}
