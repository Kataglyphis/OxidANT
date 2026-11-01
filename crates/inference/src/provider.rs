//! The error an explicitly requested ONNX Runtime execution provider returns when it cannot run.

/// The requested execution provider is missing from this ONNX Runtime or this architecture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUnavailable {
    /// ORT's provider name, e.g. `DnnlExecutionProvider`.
    pub provider: &'static str,
    pub reason: String,
}

impl std::fmt::Display for ProviderUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProviderUnavailable: {} {}", self.provider, self.reason)
    }
}

impl std::error::Error for ProviderUnavailable {}

#[cfg(test)]
mod tests {
    use super::ProviderUnavailable;

    #[test]
    fn the_message_names_the_error_and_the_provider() {
        let error = anyhow::Error::new(ProviderUnavailable {
            provider: "DnnlExecutionProvider",
            reason: "is absent".into(),
        })
        .context("ORT could not load model.onnx");
        assert!(
            format!("{error:#}").contains("ProviderUnavailable: DnnlExecutionProvider is absent")
        );
        assert_eq!(
            error
                .downcast_ref::<ProviderUnavailable>()
                .unwrap()
                .provider,
            "DnnlExecutionProvider"
        );
    }
}
