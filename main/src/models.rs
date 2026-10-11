//! The model catalog: what can be selected, where it comes from, and how big
//! it is.
//!
//! ## Adding models later
//!
//! [`catalog`] is the single place where models are declared. Adding another
//! local model is one more [`ModelDef`] entry. Remote API models are already
//! representable through [`ModelSource::Api`] so that later you can add an
//! OpenAI/Anthropic/Ollama entry without touching the downloader, the UI or
//! the settings file.

use std::path::{Path, PathBuf};

use crate::paths;

pub const HF_ENDPOINT: &str = "https://huggingface.co";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKind {
    Llm,
    Tts,
    Stt,
}

impl ModelKind {
    pub const fn label(self) -> &'static str {
        match self {
            ModelKind::Llm => "LLM",
            ModelKind::Tts => "TTS",
            ModelKind::Stt => "STT",
        }
    }
}

/// Where the weights come from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "`Api` is the extension point for hosted model providers")]
pub enum ModelSource {
    /// A Hugging Face repository. Files are fetched from
    /// `https://huggingface.co/<repo>/resolve/<revision>/<file>`.
    HuggingFace {
        repo: String,
        revision: String,
        files: Vec<String>,
    },
    /// A remote API. No files are downloaded; the GUI only needs an endpoint
    /// and a model id. Not used yet, but already wired into the data model so
    /// hosted providers can be added later without touching the downloader.
    Api {
        endpoint: String,
        model_id: String,
    },
}

impl ModelSource {
    /// Human-readable label for the Models tab.
    pub fn label(&self) -> String {
        match self {
            ModelSource::HuggingFace { repo, .. } => format!("Hugging Face · {repo}"),
            ModelSource::Api { endpoint, model_id } => format!("API · {endpoint} ({model_id})"),
        }
    }

    /// Files that need to be on disk.
    pub fn files(&self) -> &[String] {
        match self {
            ModelSource::HuggingFace { files, .. } => files,
            ModelSource::Api { .. } => &[],
        }
    }

    /// Is this model downloaded already?
    ///
    /// Sources without files (API models) are always complete: there is
    /// nothing to fetch.
    pub fn is_complete(&self, model_id: &str) -> bool {
        let files = self.files();
        files.is_empty() || files.iter().all(|f| exists(&paths::model_dir(model_id).join(f)))
    }

    /// Number of files still missing. API models are always 0.
    pub fn missing_count(&self, model_id: &str) -> usize {
        if matches!(self, ModelSource::Api { .. }) {
            return 0;
        }
        let dir = paths::model_dir(model_id);
        self.files().iter().filter(|f| !exists(&dir.join(f))).count()
    }

    /// Directory size of an already downloaded model.
    pub fn size_on_disk(&self, model_id: &str) -> u64 {
        dir_size(&paths::model_dir(model_id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDef {
    /// Stable id, also used as the folder name under `models/`.
    pub id: String,
    pub kind: ModelKind,
    /// Display name.
    pub name: String,
    /// Short description shown on the card.
    pub description: String,
    /// Human facing repository / documentation link.
    pub repo_url: String,
    /// Approximate download size in bytes, for display before downloading.
    pub approx_bytes: u64,
    pub source: ModelSource,
}

impl ModelDef {
    /// Primary file that decides whether this model counts as downloaded.
    #[allow(dead_code, reason = "part of the catalog API, used by future callers")]
    pub fn primary_file(&self) -> Option<&String> {
        self.source.files().first()
    }

    pub fn downloaded(&self) -> bool {
        self.source.is_complete(&self.id)
    }
}

/// Every model the GUI knows about.
///
/// Add entries here to extend the app. Nothing else needs to change.
pub fn catalog() -> Vec<ModelDef> {
    vec![
        ModelDef {
            id: "qwen3.5-2b-gguf".into(),
            kind: ModelKind::Llm,
            name: "Qwen3.5 2B (GGUF)".into(),
            description: "Local language model, 4-bit quantised.".into(),
            repo_url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF".into(),
            approx_bytes: 1_280_000_000,
            source: ModelSource::HuggingFace {
                repo: "unsloth/Qwen3.5-2B-GGUF".into(),
                revision: "main".into(),
                files: vec!["Qwen3.5-2B-Q4_K_M.gguf".into()],
            },
        },
        ModelDef {
            id: "kitten-tts-2".into(),
            kind: ModelKind::Tts,
            name: "Kitten TTS 2".into(),
            description: "Local text-to-speech with voice samples.".into(),
            repo_url: "https://huggingface.co/KittenML/kitten-tts-2".into(),
            approx_bytes: 573_000_000,
            source: ModelSource::HuggingFace {
                repo: "KittenML/kitten-tts-2".into(),
                revision: "main".into(),
                files: vec![
                    "cpp/default/model-tq2_1.gguf".into(),
                    "cpp/default/decoder.pt".into(),
                    "cpp/default/voices.json".into(),
                ],
            },
        },
        ModelDef {
            id: "whisper-large-v3-turbo".into(),
            kind: ModelKind::Stt,
            name: "Whisper large-v3 turbo".into(),
            description: "Local speech-to-text (openai/whisper-large-v3, turbo).".into(),
            repo_url: "https://huggingface.co/openai/whisper-large-v3-turbo".into(),
            approx_bytes: 1_621_000_000,
            source: ModelSource::HuggingFace {
                repo: "openai/whisper-large-v3-turbo".into(),
                revision: "main".into(),
                files: vec![
                    "model.safetensors".into(),
                    "config.json".into(),
                    "preprocessor_config.json".into(),
                    "generation_config.json".into(),
                    "tokenizer.json".into(),
                ],
            },
        },
    ]
}

/// Build a direct download URL for one file of a Hugging Face repo.
pub fn hf_url(repo: &str, revision: &str, file: &str) -> String {
    format!("{HF_ENDPOINT}/{repo}/resolve/{revision}/{file}")
}

fn exists(path: &Path) -> bool {
    std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

fn dir_size(dir: &Path) -> u64 {
    fn walk(dir: &Path, acc: &mut u64) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path: PathBuf = entry.path();
            if path.is_dir() {
                walk(&path, acc);
            } else if let Ok(meta) = entry.metadata() {
                *acc += meta.len();
            }
        }
    }
    let mut acc = 0;
    walk(dir, &mut acc);
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_the_three_expected_models() {
        let cat = catalog();
        assert_eq!(cat.len(), 3);
        assert!(cat.iter().any(|m| m.kind == ModelKind::Llm));
        assert!(cat.iter().any(|m| m.kind == ModelKind::Tts));
        assert!(cat.iter().any(|m| m.kind == ModelKind::Stt));
    }

    #[test]
    fn urls_are_well_formed() {
        assert_eq!(
            hf_url("unsloth/Qwen3.5-2B-GGUF", "main", "a.gguf"),
            "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/a.gguf"
        );
    }

    #[test]
    fn api_sources_need_no_files() {
        let src = ModelSource::Api {
            endpoint: "https://api.example.com/v1".into(),
            model_id: "some-model".into(),
        };
        assert!(src.files().is_empty());
        assert_eq!(src.missing_count("anything"), 0);
        assert!(src.is_complete("anything"));
    }
}