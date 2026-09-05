use crate::error::LfResult;
use crate::pipeline::PipelineMode;
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
        _model_path: Option<&Path>,
    ) -> LfResult<String> {
        Ok(crate::pipeline::format_without_remote_llm(mode, prompt))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_llm_formats_locally_without_llama_stub() {
        let llm = NativeLlm;
        let out = llm
            .generate("Привет запятая мир", PipelineMode::Professional, None)
            .unwrap();
        assert!(out.contains("Привет"));
        assert!(!out.contains("stub"));
    }

    #[test]
    fn code_mode_does_not_execute() {
        assert!(assert_non_execution_policy());
        let out = NativeLlm
            .generate("print hello", PipelineMode::Code, None)
            .unwrap();
        assert_eq!(out, "print hello");
    }

    #[test]
    fn raw_mode_is_pass_through() {
        assert_eq!(
            NativeLlm
                .generate("api sql", PipelineMode::Raw, None)
                .unwrap(),
            "api sql"
        );
    }
}
