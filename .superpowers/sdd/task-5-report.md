# Task 5 Report: Incremental Git Codebase Ingestion

## What Was Implemented

1. **Persistent Git Clone Directory Routing**: Modified `clone_and_index_repo` in `src/crawler/github.rs` to clone codebases into a persistent `~/.searchxyz/repos/` subdirectory structured as `{owner}_{repo_name}_{branch_sanitized}` instead of a random temp directory. This keeps the codebase cloned locally between crawls.
2. **Incremental Indexing Logic**:
   - Checks if the persistent directory `repo_dir` exists and is a valid Git repository (has `.git` subdirectory).
   - If it does not exist, performs a clean clone (`git clone --depth 1`) as before.
   - If it exists, resolves the active branch name using `git rev-parse --abbrev-ref HEAD`, fetches the latest commit via `git fetch --depth 1 origin <active_branch>`, gets the previous commit hash with `git rev-parse HEAD`, performs a hard reset onto the remote HEAD via `git reset --hard origin/<active_branch>`, and parses the diff list of files using `git diff --name-status <old_commit> <new_commit>`.
   - Maps diff statuses:
     - `D` (Deleted) & `M` (Modified): Deletes the file's index documents from the Tantivy search index using `index.delete_document(&file_url)`, and removes related entries and links from the Knowledge Graph using `graph.prune_node(&file_url)`.
     - `M` (Modified) & `A` (Added): Reads the file content, formats as a Markdown code block, chunks/indexes it via `index.add_document`, and parses new technology/topic associations into the KnowledgeGraph.
3. **Response Summary Enrichment**: Returns a status report summary indicating whether it was an incremental sync or clean clone, listing the number of modified/added files, number of deleted files, and lists details.

## What Was Tested and Test Results

Added `test_incremental_git_sync` to `tests` module in `src/crawler/github.rs`.
- Creates a local dummy git repository (acting as the remote origin) with initial files (`a.rs`, `b.rs`, `README.md`).
- Clones and indexes it cleanly.
- Modifies `a.rs`, deletes `b.rs`, and adds `c.rs` in the dummy origin repository, committing the changes.
- Syncs the cloned repository.
- Reloads the index reader and asserts:
  - Deleted file `b.rs` is completely gone from the search index.
  - Modified file `a.rs` and added file `c.rs` are properly indexed and updated.
  - Chunked documents (like `README.md`) are properly retained.

All 38 unit and integration tests are compiling and passing cleanly.

## TDD Evidence

### RED
Command run:
```bash
OPENSSL_VENDORED=1 cargo test crawler::github::tests::test_incremental_git_sync
```
Failing output:
```
test crawler::github::tests::test_incremental_git_sync ... FAILED

failures:

---- crawler::github::tests::test_incremental_git_sync stdout ----

thread 'crawler::github::tests::test_incremental_git_sync' (444828) panicked at src/crawler/github.rs:426:9:
assertion `left == right` failed: Expected 3 indexed files initially, got: [SourceEntry { url: "https://github.com/dummy-owner/dummy-repo/blob/main/README.md#chunk-0", title: "README.md (dummy-repo)", source: "github", indexed_at: "2026-06-21T17:45:17+00:00" }, SourceEntry { url: "https://github.com/dummy-owner/dummy-repo/blob/main/README.md#chunk-1", title: "README.md (dummy-repo)", source: "github", indexed_at: "2026-06-21T17:45:17+00:00" }]
  left: 2
 right: 3
```
*Note: The failure occurred because the incremental logic was not implemented, and the reader had not been reloaded.*

### GREEN
Command run:
```bash
OPENSSL_VENDORED=1 cargo test crawler::github::tests::test_incremental_git_sync
```
Passing output:
```
running 1 test
test crawler::github::tests::test_incremental_git_sync ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 37 filtered out; finished in 1.64s
```

## Files Changed

- [src/crawler/github.rs](file:///home/aswin/programming/vscode/myProjects/ai_agent_tools/searchxyz/src/crawler/github.rs)
- [Cargo.toml](file:///home/aswin/programming/vscode/myProjects/ai_agent_tools/searchxyz/Cargo.toml)
- [CHANGELOG.md](file:///home/aswin/programming/vscode/myProjects/ai_agent_tools/searchxyz/CHANGELOG.md)

## Self-Review Findings

- Cloned repository persistence is fully correct.
- Document and chunk updates and deletions have been safely synchronized across Tantivy and the KnowledgeGraph adjacency structures.
- All code styles conform to semantic parsing patterns.

## Issues or Concerns

None.
