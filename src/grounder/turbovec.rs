//! The real turbovec engine behind the Grounder trait: fastembed embeds code
//! chunks and the query; turbovec (2-4 bit quantized SIMD search) finds the
//! nearest chunks. Native Rust crates, no cgo, no shim - the payoff of the port.

use std::path::Path;
use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use turbovec::IdMapIndex;

use super::{Grounder, Ref};

const EMBED_DIM: usize = 384; // BGESmallENV15 is 384-dimensional (a multiple of 8)
const CHUNK_LINES: usize = 40;

/// Turbovec grounds semantically: it embeds the codebase into a quantized vector
/// index at construction and returns the chunks nearest a query.
pub struct Turbovec {
    model: TextEmbedding,
    index: Mutex<IdMapIndex>,
    chunks: Mutex<Vec<Ref>>,
}

impl Turbovec {
    /// Build the index over `root`, downloading the embedding model on first use.
    pub fn new(root: &str) -> Result<Self, String> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(false),
        )
        .map_err(|e| format!("turbovec: load model: {e}"))?;
        let index =
            IdMapIndex::new(EMBED_DIM, 4).map_err(|e| format!("turbovec: new index: {e}"))?;
        let tv = Turbovec {
            model,
            index: Mutex::new(index),
            chunks: Mutex::new(Vec::new()),
        };
        let (texts, refs) = chunk_tree(Path::new(root), root);
        tv.add_chunks(texts, refs)?;
        Ok(tv)
    }

    fn add_chunks(&self, texts: Vec<String>, refs: Vec<Ref>) -> Result<(), String> {
        if texts.is_empty() {
            return Ok(());
        }
        let embeddings = self
            .model
            .embed(texts, None)
            .map_err(|e| format!("turbovec: embed: {e}"))?;
        let mut chunks = self.chunks.lock().unwrap();
        let start_id = chunks.len() as u64;
        let mut flat = Vec::with_capacity(embeddings.len() * EMBED_DIM);
        let mut ids = Vec::with_capacity(embeddings.len());
        for (i, emb) in embeddings.iter().enumerate() {
            flat.extend_from_slice(emb);
            ids.push(start_id + i as u64);
        }
        chunks.extend(refs);
        drop(chunks); // release before taking the index lock
        self.index
            .lock()
            .unwrap()
            .add_with_ids(&flat, &ids)
            .map_err(|e| format!("turbovec: add: {e}"))?;
        Ok(())
    }

    fn embed_query(&self, query: &str) -> Option<Vec<f32>> {
        self.model.embed(vec![query], None).ok()?.into_iter().next()
    }
}

impl Grounder for Turbovec {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        if query.is_empty() || k == 0 {
            return Vec::new();
        }
        let qv = match self.embed_query(query) {
            Some(v) => v,
            None => return Vec::new(),
        };
        let (_scores, ids) = self.index.lock().unwrap().search(&qv, k);
        let chunks = self.chunks.lock().unwrap();
        ids.iter()
            .filter_map(|id| chunks.get(*id as usize).cloned())
            .collect()
    }

    fn reindex(&self, src_dir: &str, files: &[String]) {
        let mut texts = Vec::new();
        let mut refs = Vec::new();
        for f in files {
            chunk_file(&Path::new(src_dir).join(f), src_dir, &mut texts, &mut refs);
        }
        let _ = self.add_chunks(texts, refs);
    }
}

fn chunk_tree(root_path: &Path, root: &str) -> (Vec<String>, Vec<Ref>) {
    let mut texts = Vec::new();
    let mut refs = Vec::new();
    walk(root_path, root, &mut texts, &mut refs);
    (texts, refs)
}

fn walk(dir: &Path, root: &str, texts: &mut Vec<String>, refs: &mut Vec<Ref>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !matches!(
                name.as_ref(),
                ".git" | ".rigger" | "vendor" | "target" | "node_modules"
            ) {
                walk(&path, root, texts, refs);
            }
        } else {
            chunk_file(&path, root, texts, refs);
        }
    }
}

fn chunk_file(path: &Path, root: &str, texts: &mut Vec<String>, refs: &mut Vec<Ref>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let lines: Vec<&str> = content.lines().collect();
    let mut start = 0;
    while start < lines.len() {
        let end = (start + CHUNK_LINES).min(lines.len());
        let slice = &lines[start..end];
        let chunk = slice.join("\n");
        if !chunk.trim().is_empty() {
            texts.push(chunk);
            refs.push(Ref {
                file: rel.clone(),
                line: (start + 1) as u32,
                text: first_non_blank(slice),
            });
        }
        start += CHUNK_LINES;
    }
}

fn first_non_blank(lines: &[&str]) -> String {
    lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Downloads the embedding model on first run; gated behind the turbovec feature.
    #[test]
    fn grounds_semantically() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("render.rs"),
            "fn draw_sprite(sprite: &Sprite, x: f32, y: f32) {\n    // upload to the gpu\n}\n",
        )
        .unwrap();
        let tv = Turbovec::new(dir.path().to_str().unwrap()).unwrap();
        let refs = tv.ground("how is damage dealt to an enemy", 1);
        assert_eq!(
            refs.first().map(|r| r.file.as_str()),
            Some("combat.rs"),
            "semantic search should rank the damage code above the rendering code"
        );
    }
}
