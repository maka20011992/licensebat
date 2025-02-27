use crate::Cli;
use futures::StreamExt;
use licensebat_core::{licrc::LicRc, FileCollector, RetrievedDependency};
use std::sync::Arc;

const LICENSE_CACHE: &[u8] = std::include_bytes!("../license-cache.bin.zstd");

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error("Error reading dependency file: {0}")]
    DependencyFile(#[from] std::io::Error),
}

/// Check the dependencies of a project.
/// This is the main entry point of the CLI.
/// # Errors
///
/// Errors can be caused by anything.
pub async fn run(cli: Cli) -> anyhow::Result<Vec<RetrievedDependency>> {
    tracing::info!(
        dependency_file = %cli.dependency_file,
        "Licensebat running! Using {}", cli.dependency_file
    );

    // 0. spdx store & http client
    let store = Arc::new(askalono::Store::from_cache(LICENSE_CACHE).ok());
    let client = reqwest::Client::new();

    // 1 .get information from .licrc file
    tracing::debug!("Reading .licrc file");
    let licrc = LicRc::from_relative_path(cli.licrc_file)?;

    // 2. get content of the dependency file
    tracing::debug!("Getting dependency file content");
    let dep_file_content = get_dep_file_content(&cli.dependency_file).await?;

    // 3. create collectors
    tracing::debug!("Building collectors");
    let npm_retriever = licensebat_js::retriever::Npm::new(client.clone());
    let npm_collector = licensebat_js::collector::Npm::new(npm_retriever.clone());
    let yarn_collector = licensebat_js::collector::Yarn::new(npm_retriever);
    let rust_collector = licensebat_rust::collector::Rust::with_crates_io_retriever(client.clone());
    let dart_collector =
        licensebat_dart::collector::Dart::with_hosted_retriever(client.clone(), store.clone());

    let file_collectors: Vec<Box<dyn FileCollector>> = vec![
        Box::new(npm_collector),
        Box::new(yarn_collector),
        Box::new(rust_collector),
        Box::new(dart_collector),
    ];

    // 4. get dependency stream
    let mut stream = file_collectors
        .iter()
        .find(|c| cli.dependency_file.contains(&c.get_dependency_filename()))
        .and_then(|c| c.get_dependencies(&dep_file_content).ok())
        .expect("No collector found for dependency file");

    // 5. validate the dependencies according to the .licrc config
    tracing::debug!("Validating dependencies");
    let mut validated_deps = vec![];
    while let Some(mut dependency) = stream.next().await {
        // do the validation here
        licrc.validate(&mut dependency);
        validated_deps.push(dependency);
    }

    tracing::info!("Done!");
    Ok(validated_deps)
}

async fn get_dep_file_content(dependency_file: &str) -> Result<String, CheckError> {
    async {
        let dep_file_path = std::env::current_dir()?.join(dependency_file);
        let dep_file_content = tokio::fs::read_to_string(dep_file_path).await?;
        Ok(dep_file_content)
    }
    .await
    .map_err(CheckError::DependencyFile)
}
