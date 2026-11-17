//! Provider-neutral FastEmbed model selection and inference.
mod provider;
mod selection;
pub use provider::FastEmbedBgeSmallQueryProvider;
pub use selection::FastEmbedModelSelection;
