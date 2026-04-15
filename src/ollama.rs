use serde::Deserialize;

const OLLAMA_URL: &str = "http://localhost:11434/api/embed";
const BATCH_SIZE: usize = 50;

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

pub struct OllamaClient {
    client: reqwest::blocking::Client,
    model: String,
}

impl OllamaClient {
    pub fn new(model: &str) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            model: model.to_string(),
        }
    }

    pub fn check_connection(&self) -> Result<(), String> {
        self.client
            .get("http://localhost:11434/api/tags")
            .send()
            .map_err(|e| format!("Cannot connect to Ollama: {e}"))?;
        Ok(())
    }

    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(BATCH_SIZE) {
            match self.embed_batch(chunk) {
                Ok(embeddings) => {
                    all_embeddings.extend(embeddings);
                }
                Err(_) => {
                    for text in chunk {
                        match self.embed_one(text) {
                            Ok(emb) => all_embeddings.push(emb),
                            Err(e) => eprintln!("Warning: skipping chunk ({:.0} chars): {e}", text.len()),
                        }
                    }
                }
            }
        }

        Ok(all_embeddings)
    }

    pub fn embed_single(&self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_one(text)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });

        let resp = self
            .client
            .post(OLLAMA_URL)
            .json(&body)
            .send()
            .map_err(|e| format!("Ollama request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(format!("Ollama error ({}): {}", status, text));
        }

        let embed_resp: EmbedResponse = resp
            .json()
            .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

        Ok(embed_resp.embeddings)
    }

    fn embed_one(&self, text: &str) -> Result<Vec<f32>, String> {
        let body = serde_json::json!({
            "model": self.model,
            "input": [text],
        });

        let resp = self
            .client
            .post(OLLAMA_URL)
            .json(&body)
            .send()
            .map_err(|e| format!("Ollama request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(format!("Ollama error ({}): {}", status, text));
        }

        let embed_resp: EmbedResponse = resp
            .json()
            .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

        embed_resp
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "No embedding returned".to_string())
    }
}
