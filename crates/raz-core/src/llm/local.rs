//! In-process GGUF inference via candle (`feature = "local-llm"`). Loads a quantized GGUF and runs
//! a Mistral/Llama-architecture **instruct** model on CPU — no external server. Needs the model's
//! `tokenizer.json` alongside the gguf (candle's quantized loader doesn't rebuild it from gguf
//! metadata).

use std::path::Path;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use tokenizers::Tokenizer;

use crate::error::{RazError, Result};

fn err(msg: impl std::fmt::Display) -> RazError {
    RazError::Other(format!("local LLM: {msg}"))
}

/// Generate a completion for `system`+`user` from the GGUF at `gguf_path`, using the Mistral/Llama
/// `[INST] … [/INST]` instruct template. Greedy-ish (low temperature) with a light repeat penalty.
pub fn generate(
    gguf_path: &Path,
    tokenizer_path: &Path,
    system: &str,
    user: &str,
    max_tokens: usize,
) -> Result<String> {
    let device = Device::Cpu;

    let mut file = std::fs::File::open(gguf_path).map_err(err)?;
    let content = gguf_file::Content::read(&mut file).map_err(err)?;
    let mut model = ModelWeights::from_gguf(content, &mut file, &device).map_err(err)?;

    let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(err)?;
    let eos = tokenizer.token_to_id("</s>").unwrap_or(2);

    let prompt = format!("[INST] {system}\n\n{user} [/INST]");
    let prompt_tokens = tokenizer
        .encode(prompt, true)
        .map_err(err)?
        .get_ids()
        .to_vec();

    let mut logits_processor = LogitsProcessor::new(42, Some(0.2), Some(0.9));

    // Prime the model with the full prompt, sample the first response token.
    let input = Tensor::new(prompt_tokens.as_slice(), &device)
        .map_err(err)?
        .unsqueeze(0)
        .map_err(err)?;
    let logits = model
        .forward(&input, 0)
        .map_err(err)?
        .squeeze(0)
        .map_err(err)?;
    let mut next = logits_processor.sample(&logits).map_err(err)?;

    let mut generated = vec![next];
    for index in 0..max_tokens {
        if next == eos {
            break;
        }
        let input = Tensor::new(&[next], &device)
            .map_err(err)?
            .unsqueeze(0)
            .map_err(err)?;
        let logits = model
            .forward(&input, prompt_tokens.len() + index)
            .map_err(err)?
            .squeeze(0)
            .map_err(err)?;
        let start = generated.len().saturating_sub(64);
        let logits =
            candle_transformers::utils::apply_repeat_penalty(&logits, 1.1, &generated[start..])
                .map_err(err)?;
        next = logits_processor.sample(&logits).map_err(err)?;
        generated.push(next);
    }
    if generated.last() == Some(&eos) {
        generated.pop();
    }
    tokenizer.decode(&generated, true).map_err(err)
}
