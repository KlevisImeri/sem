mod chunker;
mod config;
mod db;
mod ollama;
mod search;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "sem", about = "Semantic code search for Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Index {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Search {
        query: String,
        #[arg(short, long, default_value_t = 5)]
        top: usize,
        #[arg(short = 'C', long, default_value_t = 0)]
        context: usize,
    },
    Reindex {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => cmd_init(),
        Commands::Index { path } => cmd_index(&path),
        Commands::Search { query, top, context } => cmd_search(&query, top, context),
        Commands::Reindex { path } => cmd_reindex(&path),
    }
}

fn sem_dir() -> PathBuf {
    std::env::current_dir().unwrap().join(".sem")
}

fn cmd_init() {
    let dir = sem_dir();
    std::fs::create_dir_all(&dir).expect("Failed to create .sem directory");

    let config = config::Config::default();
    config.save(&dir).expect("Failed to write config.toml");

    let db_path = dir.join("sem.db");
    let _db = db::Db::open(&db_path).expect("Failed to create database");

    println!("Initialized semantic search in {}", dir.display());
    println!("Model: {}", config.model);
    println!("Run `sem index <path>` to index your Rust code");
}

fn cmd_index(path: &Path) {
    let dir = sem_dir();
    if !dir.exists() {
        eprintln!("Not initialized. Run `sem init` first.");
        std::process::exit(1);
    }

    let config = config::Config::load(&dir);
    let db_path = dir.join("sem.db");
    let db = db::Db::open(&db_path).expect("Failed to open database");
    let client = ollama::OllamaClient::new(&config.model);

    println!("Checking Ollama connection...");
    if let Err(e) = client.check_connection() {
        eprintln!("{e}");
        std::process::exit(1);
    }

    println!("Model: {}", config.model);
    println!("Scanning for .rs files in {}...", path.display());

    let mut all_chunks: Vec<chunker::Chunk> = Vec::new();
    let mut file_count = 0;

    for entry in ignore::Walk::new(path) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match path.extension() {
            Some(ext) if ext == "rs" => {}
            _ => continue,
        }

        let file_path = path.to_string_lossy().to_string();
        let code = match std::fs::read(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Cannot read {}: {e}", file_path);
                continue;
            }
        };

        match chunker::chunk_file(&file_path, &code) {
            Ok(chunks) => {
                file_count += 1;
                for mut chunk in chunks {
                    chunk.file_path = file_path.clone();
                    all_chunks.push(chunk);
                }
            }
            Err(e) => {
                eprintln!("Warning: Cannot chunk {}: {e}", file_path);
            }
        }
    }

    if all_chunks.is_empty() {
        println!("No Rust files found.");
        return;
    }

    println!(
        "Found {} files, {} chunks. Embedding...",
        file_count,
        all_chunks.len()
    );

    let contents: Vec<String> = all_chunks.iter().map(|c| c.content.clone()).collect();

    let embeddings = client.embed(&contents).unwrap_or_else(|e| {
        eprintln!("Embedding failed: {e}");
        std::process::exit(1);
    });

    let skipped = all_chunks.len() - embeddings.len();
    if skipped > 0 {
        eprintln!("Warning: {skipped} chunk(s) skipped (too large for embedding model)");
        all_chunks.truncate(embeddings.len());
    }

    for (chunk, embedding) in all_chunks.iter().zip(embeddings.iter()) {
        db.insert_chunk(
            &chunk.file_path,
            chunk.line_start,
            chunk.line_end,
            &chunk.content,
            embedding,
        )
        .expect("Failed to insert chunk");
    }

    let total = db.chunk_count().unwrap_or(0);
    println!("Done. {} chunks indexed ({} total in DB).", embeddings.len(), total);
}

fn cmd_search(query: &str, top_n: usize, context_lines: usize) {
    let dir = sem_dir();
    if !dir.exists() {
        eprintln!("Not initialized. Run `sem init` first.");
        std::process::exit(1);
    }

    let config = config::Config::load(&dir);
    let db_path = dir.join("sem.db");
    let db = match db::Db::open(&db_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to open database: {e}");
            std::process::exit(1);
        }
    };

    let count = match db.chunk_count() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to count chunks: {e}");
            std::process::exit(1);
        }
    };

    if count == 0 {
        eprintln!("No chunks indexed. Run `sem index <path>` first.");
        std::process::exit(1);
    }

    let client = ollama::OllamaClient::new(&config.model);

    let query_embedding = match client.embed_single(query) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to embed query: {e}");
            std::process::exit(1);
        }
    };

    let chunks = match db.load_all_chunks() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load chunks: {e}");
            std::process::exit(1);
        }
    };

    let results = search::search(&query_embedding, &chunks, top_n);
    let output = search::format_results(&results, context_lines);
    print!("{output}");
}

fn cmd_reindex(path: &Path) {
    let dir = sem_dir();
    if !dir.exists() {
        eprintln!("Not initialized. Run `sem init` first.");
        std::process::exit(1);
    }

    let db_path = dir.join("sem.db");
    let db = db::Db::open(&db_path).expect("Failed to open database");

    let cleared = db.clear().expect("Failed to clear database");
    println!("Cleared {} existing chunks.", cleared);

    cmd_index(path);
}
