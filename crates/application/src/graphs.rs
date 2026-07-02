use capsulet_core::GraphDefinition;

use crate::ports::GraphRepository;

pub struct GraphService<'a, R> {
    repository: &'a R,
}

impl<'a, R> GraphService<'a, R>
where
    R: GraphRepository + Sync,
{
    #[must_use]
    pub const fn new(repository: &'a R) -> Self {
        Self { repository }
    }

    /// Stores a validated graph definition.
    ///
    /// # Errors
    ///
    /// Returns the repository error when persistence fails.
    pub async fn create_graph(&self, graph: &GraphDefinition) -> Result<(), R::Error> {
        self.repository.save_graph(graph).await
    }
}
