//! LLM backends for the advisor's `--summary`. Two paths behind one facade:
//! * [`endpoint`] — always built: POST to an OpenAI-compatible server (llama.cpp / mistral.rs /
//!   vLLM / Ollama). Used by the TUI and as the CLI default.
//! * [`local`] — `#[cfg(feature = "local-llm")]`: load and run a GGUF in-process via candle, no
//!   server. Compiled into the `raz` CLI only when built with `--features local-llm`.

pub mod endpoint;
pub use endpoint::complete;

#[cfg(feature = "local-llm")]
pub mod local;
