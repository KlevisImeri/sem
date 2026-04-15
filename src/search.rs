use crate::db::StoredChunk;
use std::fs;

#[derive(Debug)]
pub struct SearchResult {
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub content: String,
    pub score: f32,
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

pub fn search(query_embedding: &[f32], chunks: &[StoredChunk], top_n: usize) -> Vec<SearchResult> {
    let mut scored: Vec<SearchResult> = chunks
        .iter()
        .map(|chunk| {
            let score = cosine_similarity(query_embedding, &chunk.embedding);
            SearchResult {
                file_path: chunk.file_path.clone(),
                line_start: chunk.line_start,
                line_end: chunk.line_end,
                content: chunk.content.clone(),
                score,
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_n);
    scored
}

pub fn format_results(results: &[SearchResult], context_lines: usize) -> String {
    let mut output = String::new();

    for (i, result) in results.iter().enumerate() {
        if i > 0 {
            output.push_str("\n---\n\n");
        }

        output.push_str(&format!(
            "{}:{} (score: {:.4})\n",
            result.file_path, result.line_start, result.score
        ));

        if context_lines > 0 {
            if let Ok(file_content) = fs::read_to_string(&result.file_path) {
                let file_lines: Vec<&str> = file_content.lines().collect();
                let ctx_start = result.line_start.saturating_sub(context_lines);
                let ctx_end = (result.line_end + context_lines - 1).min(file_lines.len());

                for line_num in ctx_start..=ctx_end {
                    let line_idx = line_num.saturating_sub(1);
                    if line_idx < file_lines.len() {
                        let marker = if line_num >= result.line_start && line_num < result.line_end {
                            ">"
                        } else {
                            " "
                        };
                        output.push_str(&format!("{}{:>4}: {}\n", marker, line_num, file_lines[line_idx]));
                    }
                }
                output.push('\n');
            }
        }

        output.push_str(&result.content);
        output.push('\n');
    }

    output
}
