//! Diagnostics and statistics for the deobfuscation process.
//!
//! After deobfuscation completes, a `TransformDiagnostics` report is available
//! containing per-transformer statistics and overall iteration counts.

/// Aggregate diagnostics for a complete deobfuscation run.
#[derive(Debug, Clone)]
pub struct TransformDiagnostics {
    /// Total number of main-phase iterations executed.
    pub total_main_iterations: usize,

    /// Total number of finalize-phase iterations executed.
    pub total_finalize_iterations: usize,

    /// Per-transformer statistics, in registration order.
    pub transformer_statistics: Vec<TransformerStatistics>,
}

impl TransformDiagnostics {
    /// Create a new diagnostics container for the given transformer names.
    pub fn new(transformer_names: &[&str]) -> Self {
        Self {
            total_main_iterations: 0,
            total_finalize_iterations: 0,
            transformer_statistics: transformer_names
                .iter()
                .map(|name| TransformerStatistics {
                    name: name.to_string(),
                    modifications: 0,
                    nodes_visited: 0,
                })
                .collect(),
        }
    }

    /// Record that a transformer visited a node.
    pub(crate) fn record_visit(&mut self, transformer_index: usize) {
        if let Some(stats) = self.transformer_statistics.get_mut(transformer_index) {
            stats.nodes_visited += 1;
        }
    }

    /// Record that a transformer modified a node.
    pub(crate) fn record_modification(&mut self, transformer_index: usize) {
        if let Some(stats) = self.transformer_statistics.get_mut(transformer_index) {
            stats.modifications += 1;
        }
    }
}

/// Statistics for a single transformer across the entire deobfuscation run.
#[derive(Debug, Clone)]
pub struct TransformerStatistics {
    /// The transformer's name (from `Transformer::name()`).
    pub name: String,

    /// How many times this transformer reported a modification.
    pub modifications: usize,

    /// How many nodes were dispatched to this transformer.
    pub nodes_visited: usize,
}

impl std::fmt::Display for TransformDiagnostics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            formatter,
            "Iterations: {} main, {} finalize",
            self.total_main_iterations, self.total_finalize_iterations
        )?;
        for stats in &self.transformer_statistics {
            writeln!(
                formatter,
                "  {}: {} modifications, {} nodes visited",
                stats.name, stats.modifications, stats.nodes_visited
            )?;
        }
        Ok(())
    }
}
