use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::llama::{Config, Llama, LlamaConfig};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;
use tokio::sync::OnceCell;

use crate::domain::StartupInfo;

const MODEL_ID: &str = "HuggingFaceTB/SmolLM2-135M-Instruct";
const HF_BASE_URL: &str = "https://huggingface.co";
const MAX_NEW_TOKENS: usize = 256;
const TEMPERATURE: f64 = 0.7;
const TOP_P: f64 = 0.9;
const REPEAT_PENALTY: f32 = 1.1;

static MODEL_INSTANCE: OnceCell<Arc<LocalLlm>> = OnceCell::const_new();

pub struct LocalLlm {
    model: Mutex<Llama>,
    tokenizer: Tokenizer,
    device: Device,
    config: Config,
}

impl LocalLlm {
    pub async fn get_or_init() -> Result<Arc<Self>, LocalLlmError> {
        MODEL_INSTANCE
            .get_or_try_init(|| async {
                tracing::info!("Initializing local LLM: {}", MODEL_ID);
                let llm = Self::new().await?;
                Ok(Arc::new(llm))
            })
            .await
            .cloned()
    }

    async fn new() -> Result<Self, LocalLlmError> {
        let device = Device::Cpu;
        let dtype = DType::F32;

        // Create cache directory
        let cache_dir = Self::cache_dir()?;
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .map_err(|e| LocalLlmError::Io(e.to_string()))?;

        tracing::info!("Downloading model from Hugging Face: {}", MODEL_ID);

        // Download files
        let config_path = Self::download_file(&cache_dir, "config.json").await?;
        let tokenizer_path = Self::download_file(&cache_dir, "tokenizer.json").await?;
        let weights_path = Self::download_file(&cache_dir, "model.safetensors").await?;

        tracing::info!("Loading model configuration...");
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| LocalLlmError::Io(e.to_string()))?;
        let llama_config: LlamaConfig = serde_json::from_str(&config_str)
            .map_err(|e| LocalLlmError::Config(e.to_string()))?;
        let config = llama_config.into_config(false); // false = no flash attention

        tracing::info!("Loading tokenizer...");
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| LocalLlmError::Tokenizer(e.to_string()))?;

        tracing::info!("Loading model weights (~135MB)...");
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], dtype, &device)
                .map_err(|e| LocalLlmError::Model(e.to_string()))?
        };

        let model = Llama::load(vb, &config)
            .map_err(|e| LocalLlmError::Model(e.to_string()))?;

        tracing::info!("Local LLM initialized successfully!");

        Ok(Self {
            model: Mutex::new(model),
            tokenizer,
            device,
            config,
        })
    }

    fn cache_dir() -> Result<PathBuf, LocalLlmError> {
        let home = std::env::var("HOME").map_err(|_| LocalLlmError::Io("HOME not set".to_string()))?;
        let model_name = MODEL_ID.replace("/", "--");
        Ok(PathBuf::from(home)
            .join(".cache")
            .join("roasting-startup")
            .join("models")
            .join(model_name))
    }

    async fn download_file(cache_dir: &PathBuf, filename: &str) -> Result<PathBuf, LocalLlmError> {
        let file_path = cache_dir.join(filename);

        // Check if file already exists
        if file_path.exists() {
            tracing::info!("Using cached {}", filename);
            return Ok(file_path);
        }

        let url = format!(
            "{}/{}/resolve/main/{}",
            HF_BASE_URL, MODEL_ID, filename
        );

        tracing::info!("Downloading {}...", filename);

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("User-Agent", "roasting-startup/1.0")
            .send()
            .await
            .map_err(|e| LocalLlmError::HfHub(format!("Failed to download {}: {}", filename, e)))?;

        if !response.status().is_success() {
            return Err(LocalLlmError::HfHub(format!(
                "Failed to download {}: HTTP {}",
                filename,
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| LocalLlmError::HfHub(format!("Failed to read {}: {}", filename, e)))?;

        tokio::fs::write(&file_path, &bytes)
            .await
            .map_err(|e| LocalLlmError::Io(format!("Failed to write {}: {}", filename, e)))?;

        tracing::info!("Downloaded {} ({} bytes)", filename, bytes.len());
        Ok(file_path)
    }

    pub fn generate_roast(&self, startup_info: &StartupInfo) -> Result<String, LocalLlmError> {
        let prompt = self.build_chat_prompt(startup_info);
        self.generate(&prompt)
    }

    fn build_chat_prompt(&self, startup_info: &StartupInfo) -> String {
        let title = startup_info.title.as_deref().unwrap_or("Unknown");
        let description = startup_info
            .description
            .as_deref()
            .unwrap_or("No description");
        let headings = if startup_info.headings.is_empty() {
            "None".to_string()
        } else {
            startup_info.headings.join(", ")
        };
        let content = &startup_info.content_summary;

        // SmolLM2 uses simple chat format
        format!(
            r#"<|im_start|>system
You are a brutal but funny roasting comedian. Your job is to roast startups in Indonesian language.
<|im_end|>
<|im_start|>user
Roast this startup brutally but hilariously in Indonesian slang (bahasa gaul):

URL: {}
Name: {}
Description: {}
Headings: {}
Content: {}

Requirements:
- Use Indonesian slang (bahasa gaul Jakarta)
- Be savage but funny
- 2-3 short paragraphs
- End with a dramatic failure prediction
<|im_end|>
<|im_start|>assistant
"#,
            startup_info.url, title, description, headings, content
        )
    }

    fn generate(&self, prompt: &str) -> Result<String, LocalLlmError> {
        let tokens = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| LocalLlmError::Tokenizer(e.to_string()))?;

        let input_ids = tokens.get_ids();
        let prompt_len = input_ids.len();

        let mut logits_processor = LogitsProcessor::new(
            rand::random(),
            Some(TEMPERATURE),
            Some(TOP_P),
        );

        let eos_token_id = self
            .tokenizer
            .token_to_id("<|im_end|>")
            .or_else(|| self.tokenizer.token_to_id("<|endoftext|>"))
            .or_else(|| self.tokenizer.token_to_id("</s>"))
            .unwrap_or(2); // Common EOS token

        tracing::info!("Generating response ({} input tokens)...", prompt_len);

        let model = self.model.lock().map_err(|e| LocalLlmError::Model(e.to_string()))?;

        // Create fresh cache for each generation
        let mut cache = candle_transformers::models::llama::Cache::new(
            true,
            DType::F32,
            &self.config,
            &self.device,
        ).map_err(|e| LocalLlmError::Model(e.to_string()))?;

        let mut generated_tokens: Vec<u32> = Vec::new();
        let mut current_tokens = input_ids.to_vec();

        for i in 0..MAX_NEW_TOKENS {
            let input = Tensor::new(&current_tokens[..], &self.device)
                .map_err(|e| LocalLlmError::Model(format!("Tensor creation error: {}", e)))?
                .unsqueeze(0)
                .map_err(|e| LocalLlmError::Model(format!("Unsqueeze error: {}", e)))?;

            let index_pos = if i == 0 { 0 } else { prompt_len + i - 1 };
            let logits = model
                .forward(&input, index_pos, &mut cache)
                .map_err(|e| LocalLlmError::Model(format!("Forward pass error at token {}: {}", i, e)))?;

            // Llama returns logits for last token only: [batch, vocab_size]
            let logits = logits
                .squeeze(0)
                .map_err(|e| LocalLlmError::Model(format!("Squeeze error: {}", e)))?;

            // Apply repeat penalty
            let all_tokens: Vec<u32> = input_ids.iter().copied().chain(generated_tokens.iter().copied()).collect();
            let logits = self.apply_repeat_penalty(&logits, &all_tokens)?;

            // Sample next token
            let next_token = logits_processor
                .sample(&logits)
                .map_err(|e| LocalLlmError::Model(format!("Sample error: {}", e)))?;

            if next_token == eos_token_id {
                tracing::info!("EOS token reached after {} tokens", i + 1);
                break;
            }

            generated_tokens.push(next_token);
            current_tokens = vec![next_token]; // Only feed new token with KV cache
        }

        drop(model);

        let response = self
            .tokenizer
            .decode(&generated_tokens, true)
            .map_err(|e| LocalLlmError::Tokenizer(e.to_string()))?;

        Ok(response.trim().to_string())
    }

    fn apply_repeat_penalty(
        &self,
        logits: &Tensor,
        tokens: &[u32],
    ) -> Result<Tensor, LocalLlmError> {
        let mut logits_vec: Vec<f32> = logits
            .to_vec1()
            .map_err(|e| LocalLlmError::Model(e.to_string()))?;

        for &token in tokens.iter().rev().take(64) {
            if let Some(logit) = logits_vec.get_mut(token as usize) {
                if *logit > 0.0 {
                    *logit /= REPEAT_PENALTY;
                } else {
                    *logit *= REPEAT_PENALTY;
                }
            }
        }

        Tensor::new(logits_vec, &self.device).map_err(|e| LocalLlmError::Model(e.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LocalLlmError {
    #[error("Hugging Face Hub error: {0}")]
    HfHub(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Tokenizer error: {0}")]
    Tokenizer(String),

    #[error("Model error: {0}")]
    Model(String),
}
