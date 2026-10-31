//! Built-in PaddlePaddle recognition for the Full Parser.
//!
//! Windows ships only llama.cpp's CPU build, and Smart App Control refuses its
//! unsigned DLLs, so new Windows installs read pages with PaddlePaddle itself.
//! The choice is recorded in the parser manifest when the runtime is installed
//! and never changes during extraction. Installs made with managed llama.cpp
//! keep using it.

use super::*;

const MANAGED_LLAMA_BACKEND: &str = "managed llama.cpp";
const BUILT_IN_BACKEND: &str = "built-in PaddlePaddle";
const NATIVE_MODEL_DIR: &str = "PaddleOCR-VL-1.6-0.9B";
const NATIVE_MODEL_WEIGHTS: &str = "model.safetensors";
pub(super) const NATIVE_MODEL_REVISION: &str = "c5630abae1d940eafe0697512a0325494b02ab42";

pub(super) struct NativeModelFile {
    pub(super) name: &'static str,
    pub(super) bytes: u64,
    pub(super) sha256: &'static str,
}

// PaddleX loads these directly, so the repository's Python files are excluded.
pub(super) const NATIVE_MODEL_FILES: &[NativeModelFile] = &[
    NativeModelFile {
        name: "LICENSE",
        bytes: 11_376,
        sha256: "b8c4d7deccd236af023af1c88c4d4e8f0fc2f41914e0fb23a3ec9678fb5a8456",
    },
    NativeModelFile {
        name: "added_tokens.json",
        bytes: 25_381,
        sha256: "f59f889088e0fe21c523e7cf121bb6dca3b0bb148cb7159fbb4572c74dfc5644",
    },
    NativeModelFile {
        name: "chat_template.jinja",
        bytes: 1_474,
        sha256: "2f27812dab7f333e471884e0c803d807f11953d5453140dfb1aaba234f872bc8",
    },
    NativeModelFile {
        name: "config.json",
        bytes: 2_059,
        sha256: "ce7f4565f8b1db78532ad5d1b9ebe55c2139d49bd4cb04778b580a08a598f171",
    },
    NativeModelFile {
        name: "generation_config.json",
        bytes: 133,
        sha256: "a6701d78ab3b4d972307cdec3b69d4c13f46e0d5140514f50ab7d84259324b94",
    },
    NativeModelFile {
        name: "inference.yml",
        bytes: 43,
        sha256: "1587aece2d6442366efce34161d5f7d5f67f09ebfbf043168e5fade892c2780e",
    },
    NativeModelFile {
        name: NATIVE_MODEL_WEIGHTS,
        bytes: 1_917_255_968,
        sha256: "85a479d506a11e724e7285d395c551be69f41dbc16b6342d3cacfb189aed71db",
    },
    NativeModelFile {
        name: "preprocessor_config.json",
        bytes: 641,
        sha256: "111872ab1e8bb7fd040ac5087bfced7ab8f011f02139b088cba294964c3b1d0e",
    },
    NativeModelFile {
        name: "processor_config.json",
        bytes: 137,
        sha256: "1568858960a9760c54431dae693a6152e601ff55cdf6d2eab97a4a99958faea0",
    },
    NativeModelFile {
        name: "special_tokens_map.json",
        bytes: 1_151,
        sha256: "d3a125c03103deb2acaf7730791bdbbf196f620e5a2213b664511ff9b4b25bab",
    },
    NativeModelFile {
        name: "tokenizer.json",
        bytes: 11_189_060,
        sha256: "c8a215a59183d0d0781adc33bacd3ce6162716f7fd568fb30234a74d69803a7d",
    },
    NativeModelFile {
        name: "tokenizer.model",
        bytes: 1_614_363,
        sha256: "34ef7db83df785924fb83d7b887b6e822a031c56e15cff40aaf9b982988180df",
    },
    NativeModelFile {
        name: "tokenizer_config.json",
        bytes: 186_947,
        sha256: "1f979337347cc0cb72a6282d8a23ed183539aa81a87a906f022aee2bab83c7c5",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecognitionBackend {
    ManagedLlama,
    BuiltIn,
}

impl RecognitionBackend {
    pub(super) fn manifest_value(self) -> &'static str {
        match self {
            Self::ManagedLlama => MANAGED_LLAMA_BACKEND,
            Self::BuiltIn => BUILT_IN_BACKEND,
        }
    }

    pub(super) fn from_manifest(manifest: &serde_json::Value) -> Option<Self> {
        match manifest
            .get("recognition_backend")
            .and_then(serde_json::Value::as_str)
        {
            Some(MANAGED_LLAMA_BACKEND) => Some(Self::ManagedLlama),
            Some(BUILT_IN_BACKEND) => Some(Self::BuiltIn),
            _ => None,
        }
    }
}

pub(super) fn recognition_backend_for(os: &str, arch: &str) -> RecognitionBackend {
    if os == "windows" && arch == "x86_64" {
        RecognitionBackend::BuiltIn
    } else {
        RecognitionBackend::ManagedLlama
    }
}

/// The backend a new install uses on this computer.
pub(super) fn new_install_recognition_backend() -> RecognitionBackend {
    recognition_backend_for(std::env::consts::OS, std::env::consts::ARCH)
}

pub(super) fn native_model_dir(version_root: &Path) -> PathBuf {
    version_root.join("models").join(NATIVE_MODEL_DIR)
}

pub(super) fn native_model_committed(version_root: &Path) -> bool {
    let model = native_model_dir(version_root);
    NATIVE_MODEL_FILES
        .iter()
        .all(|file| bounded_regular_file(&model.join(file.name), file.bytes))
}

fn native_model_url(filename: &str) -> String {
    format!(
        "https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6/resolve/{NATIVE_MODEL_REVISION}/{filename}?download=true"
    )
}

/// Download every pinned recognition file into the parser's `models` scope, so
/// the install integrity inventory binds them with the rest of the runtime.
pub(super) async fn install_native_recognition_model(
    app: &crate::emit::EventBus,
    version_root: &Path,
) -> Result<PathBuf, String> {
    let model = native_model_dir(version_root);
    std::fs::create_dir_all(&model)
        .map_err(|error| format!("Failed to create recognition model directory: {error}"))?;
    for file in NATIVE_MODEL_FILES {
        download_verified(
            app,
            &[native_model_url(file.name)],
            &model.join(file.name),
            file.sha256,
            file.bytes,
            "PaddleOCR-VL recognition model",
        )
        .await?;
    }
    Ok(model)
}
