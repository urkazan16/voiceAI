use localflow_lib::catalog::ModelCatalog;

#[test]
fn ai_bench_profile_documents_gate() {
    let catalog = ModelCatalog::embedded().unwrap();
    let llm = catalog
        .models
        .iter()
        .find(|m| m.kind == "llm")
        .expect("llm catalog entry");
    assert_eq!(llm.format, "GGUF");
    assert!(!llm.sha256.is_empty());
    println!(
        "AI bench requires a verified local model at Application Support; catalog id={}",
        llm.model_id
    );
}
