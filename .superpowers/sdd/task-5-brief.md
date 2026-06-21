### Task 5: Incremental Git Codebase Ingestion

**Files:**
- Modify: `src/crawler/github.rs`
- Test: Add unit/integration tests in `src/crawler/github.rs` to verify incremental repository indexing, matching git diffs, and deleting/re-indexing only modified/deleted files.

**Interfaces:**
- Consumes: `clone_and_index_repo`.
- Produces: Efficient incremental Git syncing and index updates.

- [ ] **Step 1: Write the failing tests**
  Add `test_incremental_git_sync` to `tests` module in `src/crawler/github.rs`:
  - Set up a dummy local git repository (acting as the remote origin) with standard files.
  - Clone and index it cleanly.
  - Modify some files, add new files, and delete some files in the dummy origin repository, committing the changes.
  - Run the update indexing on the local clone path.
  - Verify that only the changed files are updated in the search index and knowledge graph (the deleted file is gone, the modified file has the new content, and the added file is indexed).

- [ ] **Step 2: Run test to verify it fails**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: FAIL

- [ ] **Step 3: Write minimal implementation**
  1. Resolve target repository directory name in `clone_and_index_repo`:
     ```rust
     let resolved_branch = branch.or(parsed_branch.as_deref());
     let branch_sanitized = resolved_branch.unwrap_or("default").replace('/', "_").replace('\\', "_");
     let repo_dir_name = format!("{}_{}_{}", owner, repo_name, branch_sanitized);
     let repo_dir = dirs::home_dir()
         .unwrap_or_else(|| std::env::temp_dir())
         .join(".searchxyz")
         .join("repos")
         .join(repo_dir_name);
     ```
  2. Implement checking if `repo_dir` exists.
     - If it does NOT exist: clone normally to `repo_dir` using `git clone --depth 1` (as before, but into `repo_dir` instead of `temp_dir`), and index all files recursively.
     - If it DOES exist:
       - Run `git rev-parse --abbrev-ref HEAD` to find the active branch.
       - Run `git rev-parse HEAD` to get `old_commit`.
       - Run `git fetch --depth 1 origin <branch>`.
       - Run `git reset --hard origin/<branch>`.
       - Run `git rev-parse HEAD` to get `new_commit`.
       - Run `git diff --name-status <old_commit> <new_commit>` to retrieve changed files list.
       - Parse the output (each line has `Status\tPath` e.g. `M\tsrc/lib.rs`).
       - For status `D` (Deleted) or `M` (Modified): delete their previous chunk paths from Tantivy using `index.delete_document(&file_url)`.
       - For status `M` (Modified) or `A` (Added): read files, format to markdown code blocks, chunk content, and index new chunks using `index.add_document`. Also update knowledge graph heuristics.
  3. Ensure clean up of temporary files is not needed if we keep the clones inside the persistent `~/.searchxyz/repos/` directory.

- [ ] **Step 4: Run test to verify it passes**
  Run: `OPENSSL_VENDORED=1 cargo test`
  Expected: PASS

- [ ] **Step 5: Commit**
  Run:
  ```bash
  git add src/crawler/github.rs CHANGELOG.md
  git commit -m "feat: support incremental git code repository indexing based on git pull delta diffs"
  ```
