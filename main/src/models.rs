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
    /// Fallback download size in bytes, used until the real one has been
    /// fetched from Hugging Face. Never shown as exact - see
    /// [`crate::models::sizes::fetch`].
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
            approx_bytes: 1_280_000_000, // real: 1_281_302_528
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
            approx_bytes: 1_602_623_375, // real, measured: was 573 MB (-64%)
            source: ModelSource::HuggingFace {
                repo: "KittenML/kitten-tts-2".into(),
                revision: "main".into(),
                files: vec![
                    // Not `cpp/default/...`: the weights sit one level up. The
                    // old path 404'd, which is how this was found.
                    "cpp/model-tq2_1.gguf".into(),
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
            approx_bytes: 1_621_000_000, // real: 1_619_854_052
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

/// Real download sizes, fetched from the Hugging Face API.
///
/// `approx_bytes` in the catalog was a hand-written guess, and the guess was
/// wrong often enough to be misleading ("Size 1.19 GB" for a 1.22 GB file).
/// The tree endpoint reports the true size of every file, so the number on the
/// card can be exact instead of approximately fictional.
pub mod sizes {
    use super::{ModelDef, ModelSource, HF_ENDPOINT};
    use std::collections::HashMap;

    /// `model id -> total bytes`.
    pub type Sizes = HashMap<String, u64>;

    /// Fetch the real size of every Hugging Face model in `catalog`.
    ///
    /// Blocking, so this belongs on a worker thread. A repo that cannot be
    /// reached is simply left out of the map, and the UI keeps showing the
    /// catalog's fallback - a wrong-looking number is better than an error
    /// dialog on the Models page.
    pub fn fetch(
        catalog: &[ModelDef],
        token: Option<&str>,
        timeout: std::time::Duration,
    ) -> Result<Sizes, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("ProjectRem/", env!("PROJECTREM_VERSION")))
            .timeout(timeout)
            .build()
            .map_err(|e| e.to_string())?;

        let mut out = Sizes::new();
        let mut first_error: Option<String> = None;

        for model in catalog {
            let ModelSource::HuggingFace { repo, files, .. } = &model.source else {
                continue;
            };
            match repo_size(&client, repo, files, token) {
                Ok(total) => {
                    out.insert(model.id.clone(), total);
                }
                Err(err) => {
                    // Remember the first failure but keep going: one broken repo
                    // should not blank out the sizes of the others.
                    first_error.get_or_insert(err);
                }
            }
        }

        match (out.is_empty(), first_error) {
            (false, _) => Ok(out),
            (true, Some(err)) => Err(err),
            (true, None) => Ok(out),
        }
    }

    fn repo_size(
        client: &reqwest::blocking::Client,
        repo: &str,
        wanted: &[String],
        token: Option<&str>,
    ) -> Result<u64, String> {
        let url = format!("{HF_ENDPOINT}/api/models/{repo}/tree/main?recursive=true");
        let request = client.get(&url);
        let request = match token {
            Some(value) => request.bearer_auth(value),
            None => request,
        };
        let response = request.send().map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {} for {repo}", response.status()));
        }
        let body: serde_json::Value = response.json().map_err(|e| e.to_string())?;

        // The tree is a list of entries; only the files this model actually
        // downloads are counted, so an extra README in the repo does not inflate
        // the number.
        let mut total = 0u64;
        let mut found = 0usize;
        for entry in body.as_array().into_iter().flatten() {
            if entry.get("type").and_then(|t| t.as_str()) != Some("file") {
                continue;
            }
            let Some(path) = entry.get("path").and_then(|p| p.as_str()) else { continue };
            if !wanted.iter().any(|name| name == path) {
                continue;
            }
            if let Some(size) = entry.get("size").and_then(|s| s.as_u64()) {
                total += size;
                found += 1;
            }
        }

        (found > 0).then_some(total).ok_or_else(|| format!("no sizes for {repo}"))
    }

    /// Sum the `size` fields of a tree response.
    ///
    /// Split out so the parsing can be tested without a network round trip.
    #[cfg(test)]
    pub fn sum_tree(body: &serde_json::Value, wanted: &[String]) -> Option<u64> {
        let mut total = 0u64;
        let mut found = 0usize;
        for entry in body.as_array()? {
            if entry.get("type").and_then(|t| t.as_str()) != Some("file") {
                continue;
            }
            let path = entry.get("path")?.as_str()?;
            if !wanted.iter().any(|name| name == path) {
                continue;
            }
            total += entry.get("size")?.as_u64()?;
            found += 1;
        }
        (found > 0).then_some(total)
    }
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

    #[test]
    fn the_tree_parser_only_counts_the_files_we_download() {
        let body = serde_json::json!([
            {"type": "file", "path": "model.safetensors", "size": 100},
            {"type": "file", "path": "README.md", "size": 900},
            {"type": "directory", "path": "cpp"},
            {"type": "file", "path": "config.json", "size": 5}
        ]);
        let wanted = vec!["model.safetensors".to_string()];
        // README and config must not inflate the number.
        assert_eq!(sizes::sum_tree(&body, &wanted), Some(100));

        let two = vec!["model.safetensors".to_string(), "config.json".to_string()];
        assert_eq!(sizes::sum_tree(&body, &two), Some(105));

        // A file we do not have is a problem, not a zero.
        let missing = vec!["nope.gguf".to_string()];
        assert_eq!(sizes::sum_tree(&body, &missing), None);
    }

    /// Every file in the catalog must actually exist in its repository.
    ///
    /// A wrong path does not fail at build time - it fails much later as a 404
    /// in the middle of a download, with the user waiting. `KittenML/kitten-tts-2`
    /// had exactly that problem (`cpp/default/model-tq2_1.gguf` instead of
    /// `cpp/model-tq2_1.gguf`) until the size fetcher pointed it out.
    ///
    /// Network access, so it is `#[ignore]`d and run deliberately:
    ///     cargo test -- --ignored catalog_files_exist_upstream
    #[test]
    #[ignore = "needs network"]
    fn catalog_files_exist_upstream() {
        let client = reqwest::blocking::Client::builder()
            .user_agent("ProjectRem catalog check")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("client");

        for model in catalog() {
            let ModelSource::HuggingFace { repo, files, .. } = &model.source else { continue };
            let url = format!("{HF_ENDPOINT}/api/models/{repo}/tree/main?recursive=true");
            let body: serde_json::Value = client
                .get(&url)
                .send()
                .unwrap_or_else(|e| panic!("{repo}: {e}"))
                .json()
                .unwrap_or_else(|e| panic!("{repo}: bad JSON: {e}"));

            for file in files {
                let present = body
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|entry| {
                        entry.get("type").and_then(|t| t.as_str()) == Some("file")
                            && entry.get("path").and_then(|p| p.as_str()) == Some(file.as_str())
                    });
                assert!(present, "{model_id}: {repo} has no file {file}", model_id = model.id);
            }
        }
    }

    /// The size on the card must not be off by more than 10% once we know the
    /// real figure - that is what makes the hand-written `approx_bytes`
    /// fallback tolerable rather than misleading.
    #[test]
    #[ignore = "needs network"]
    fn the_fallback_sizes_are_close_to_reality() {
        let catalog = catalog();
        let sizes =
            sizes::fetch(&catalog, None, std::time::Duration::from_secs(30)).expect("fetch");
        for model in &catalog {
            let Some(real) = sizes.get(&model.id) else {
                eprintln!("skipping {}: no size returned", model.id);
                continue;
            };
            let guess = model.approx_bytes as f64;
            let ratio = *real as f64 / guess;
            assert!(
                (0.9..=1.1).contains(&ratio),
                "{}: catalog says {} but the real size is {} ({:+.0}%)",
                model.id,
                model.approx_bytes,
                real,
                (ratio - 1.0) * 100.0
            );
        }
    }
}