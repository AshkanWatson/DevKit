//! SourceResolver: dispatches a resolution request to the committed provider
//! registered for that package ID (ADR 0009). Adding a new provider does not
//! touch the download engine or the pipeline.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{ArtifactSource, ResolveRequest, ResolvedArtifact, SourceError, SourceHttp};

/// Registry of providers by package ID. Built once from committed definitions;
/// callers cannot register arbitrary providers from catalog data.
pub struct SourceResolver {
    /// Stable provider list (owned) plus a package-id → index lookup.
    providers: Vec<Arc<dyn ArtifactSource>>,
    by_id: HashMap<String, usize>,
}

impl SourceResolver {
    /// Builds a resolver from a list of providers. Duplicate package IDs are
    /// rejected so resolution can never be ambiguous.
    pub fn new(providers: Vec<Box<dyn ArtifactSource>>) -> Result<Self, SourceError> {
        let providers: Vec<Arc<dyn ArtifactSource>> =
            providers.into_iter().map(Arc::from).collect();
        let mut by_id = HashMap::new();
        for (index, provider) in providers.iter().enumerate() {
            for id in provider.package_ids() {
                if by_id.insert((*id).to_owned(), index).is_some() {
                    return Err(SourceError::InvalidMetadata(format!(
                        "duplicate provider registration for {id}"
                    )));
                }
            }
        }
        Ok(Self { providers, by_id })
    }

    /// Resolves `request` via the registered provider, using `http` for
    /// official metadata.
    pub async fn resolve(
        &self,
        request: &ResolveRequest,
        http: &dyn SourceHttp,
    ) -> Result<ResolvedArtifact, SourceError> {
        let index = self
            .by_id
            .get(&request.package_id)
            .ok_or_else(|| SourceError::NoProvider(request.package_id.clone()))?;
        self.providers[*index].resolve(request, http).await
    }

    /// True when a provider is registered for `package_id`.
    pub fn has_provider(&self, package_id: &str) -> bool {
        self.by_id.contains_key(package_id)
    }
}
