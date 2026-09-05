use crate::error::LfResult;
use crate::pipeline::PipelineMode;
use crate::runtime;
use std::path::Path;

pub trait LanguageModel: Send + Sync {
    fn generate(
        &self,
        prompt: &str,
        mode: PipelineMode,
        model_path: Option<&Path>,
    ) -> LfResult<String>;
}

pub struct NativeLlm;

impl LanguageModel for NativeLlm {
    fn generate(
        &self,
        prompt: &str,
        mode: PipelineMode,
        model_path: Option<&Path>,
    ) -> LfResult<String> {
        match mode {
            PipelineMode::Raw | PipelineMode::Normal => {
                Ok(crate::pipeline::format_without_remote_llm(mode, prompt))
            }
            PipelineMode::Professional | PipelineMode::Code => {
                let path = model_path
                    .ok_or_else(|| crate::error::LfError::ModelMissing("llm".into()))?
                    .to_string_lossy();
                runtime::native_generate(&path, prompt)
            }
        }
    }
}

pub struct ScriptedLlm;

impl LanguageModel for ScriptedLlm {
    fn generate(
        &self,
        prompt: &str,
        mode: PipelineMode,
        _model_path: Option<&Path>,
    ) -> LfResult<String> {
        Ok(crate::pipeline::format_without_remote_llm(mode, prompt))
    }
}

/// Generated code is never executed. Callers must only insert text.
pub fn assert_non_execution_policy() -> bool {
    true
}
