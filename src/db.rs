use rusqlite::Connection;
use std::path::Path;

pub struct Db {
    conn: Connection,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct StoredChunk {
    pub id: i64,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub content: String,
    pub embedding: Vec<f32>,
}

impl Db {
    pub fn open(db_path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY,
                file_path TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_file ON chunks(file_path);",
        )?;
        Ok(Self { conn })
    }

    pub fn clear(&self) -> Result<usize, rusqlite::Error> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
        self.conn.execute("DELETE FROM chunks", [])?;
        Ok(count as usize)
    }

    pub fn insert_chunk(
        &self,
        file_path: &str,
        line_start: usize,
        line_end: usize,
        content: &str,
        embedding: &[f32],
    ) -> Result<(), rusqlite::Error> {
        let blob = embedding_to_blob(embedding);
        self.conn.execute(
            "INSERT INTO chunks (file_path, line_start, line_end, content, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![file_path, line_start, line_end, content, blob],
        )?;
        Ok(())
    }

    pub fn load_all_chunks(&self) -> Result<Vec<StoredChunk>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_path, line_start, line_end, content, embedding FROM chunks",
        )?;
        let chunks = stmt
            .query_map([], |row| {
                let blob: Vec<u8> = row.get(5)?;
                Ok(StoredChunk {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    line_start: row.get(2)?,
                    line_end: row.get(3)?,
                    content: row.get(4)?,
                    embedding: blob_to_embedding(&blob),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(chunks)
    }

    pub fn chunk_count(&self) -> Result<usize, rusqlite::Error> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
        Ok(count as usize)
    }
}

fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        blob.extend_from_slice(&val.to_le_bytes());
    }
    blob
}

fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
