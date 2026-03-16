use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::runtime::bootstrap::{bootstrap, build_embedding_provider};

#[derive(Subcommand)]
pub enum MemoryCommands {
    #[command(about = "Show memory index statistics")]
    Stats,
    #[command(about = "Show memory trace audit log for an agent")]
    Audit {
        #[arg(help = "Agent ID to audit")]
        agent_id: String,
        #[arg(long, short = 'n', default_value = "20", help = "Number of entries")]
        limit: usize,
    },
    #[command(about = "Rebuild search index from memory files")]
    RebuildIndex,
}

pub async fn run(cmd: MemoryCommands, root: &Path) -> Result<()> {
    let (_bus, memory, _gateway, config, _schedule_manager, _wait_manager, _approval_registry) =
        bootstrap(root, None).await?;

    match cmd {
        MemoryCommands::Stats => {
            let db = memory.db();
            let conn = db.lock().map_err(|_| anyhow::anyhow!("lock failed"))?;

            let chunk_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
            let file_count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
            let cache_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM embedding_cache", [], |r| r.get(0))?;
            let trace_count: i64 =
                conn.query_row("SELECT COUNT(*) FROM memory_trace", [], |r| r.get(0))?;

            let total_access: i64 = conn.query_row(
                "SELECT COALESCE(SUM(access_count), 0) FROM chunks",
                [],
                |r| r.get(0),
            )?;

            let hot_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM chunks WHERE access_count > 0",
                [],
                |r| r.get(0),
            )?;

            println!("Memory Index Statistics:");
            println!("  Chunks indexed:    {chunk_count}");
            println!("  Files tracked:     {file_count}");
            println!("  Embedding cache:   {cache_count}");
            println!("  Trace entries:     {trace_count}");
            println!("  Total accesses:    {total_access}");
            println!("  Hot chunks (>0):   {hot_count}");

            // Show per-source breakdown
            let mut stmt = conn
                .prepare("SELECT source, COUNT(*) FROM chunks GROUP BY source ORDER BY source")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            println!("\n  By source:");
            for row in rows {
                let (source, count) = row?;
                println!("    {source}: {count}");
            }

            Ok(())
        }
        MemoryCommands::Audit { agent_id, limit } => {
            let db = memory.db();
            let conn = db.lock().map_err(|_| anyhow::anyhow!("lock failed"))?;

            let mut stmt = conn.prepare(
                "SELECT timestamp, operation, details, duration_ms FROM memory_trace WHERE agent_id = ?1 ORDER BY timestamp DESC LIMIT ?2"
            )?;
            let rows = stmt.query_map(rusqlite::params![agent_id, limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })?;

            let mut count = 0;
            for row in rows {
                let (timestamp, operation, details, duration_ms) = row?;
                let duration = duration_ms
                    .map(|ms| format!(" ({ms}ms)"))
                    .unwrap_or_default();
                println!("[{timestamp}] {operation}{duration}");
                println!("  {details}");
                println!();
                count += 1;
            }

            if count == 0 {
                println!("No trace entries found for agent '{agent_id}'.");
            } else {
                println!("Showing {count} entries (newest first).");
            }

            Ok(())
        }
        MemoryCommands::RebuildIndex => {
            let workspace_dir = root.to_path_buf();
            let file_store = clawhive_memory::file_store::MemoryFileStore::new(&workspace_dir);
            let session_reader = clawhive_memory::session::SessionReader::new(&workspace_dir);
            let search_index = clawhive_memory::search_index::SearchIndex::new(memory.db());

            let embedding_provider = build_embedding_provider(&config).await;
            println!("Rebuilding search index...");
            let count = search_index
                .index_all(&file_store, &session_reader, embedding_provider.as_ref())
                .await?;
            println!("Done. Indexed {count} chunks.");

            Ok(())
        }
    }
}
