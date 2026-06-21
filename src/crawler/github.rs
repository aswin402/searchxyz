use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::sync::Mutex;
use rand::Rng;

use crate::error::SearchXyzError;
use crate::index::SearchIndex;
use crate::graph::KnowledgeGraph;
use crate::extractor::ExtractedContent;

const DEFAULT_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "go", "c", "cpp", "h", "java", "swift", "kt", "sh", "pl", "rb", "php",
    "md", "txt", "rst", "adoc", "toml", "json", "yaml", "yml", "ini", "conf"
];

const DEFAULT_EXCLUDE_PATHS: &[&str] = &[
    ".git", ".github", "target", "node_modules", "dist", "build", "vendor", "bin", "obj"
];

const MAX_FILE_SIZE_BYTES: u64 = 256 * 1024; // 256 KB

/// Parse a GitHub URL to extract Owner, Repo, and optional Branch
pub fn parse_github_url(url: &str) -> Option<(String, String, Option<String>)> {
    let parsed = url::Url::parse(url).ok()?;
    
    // Check hostname
    let host = parsed.host_str()?;
    if host != "github.com" && host != "www.github.com" {
        return None;
    }
    
    let mut segments = parsed.path_segments()?;
    let owner = segments.next()?.to_string();
    let mut repo = segments.next()?.to_string();
    
    if repo.ends_with(".git") {
        repo = repo[..repo.len() - 4].to_string();
    }
    
    // Check if there is a branch specified, e.g. /tree/branch-name or /blob/branch-name
    let next_segment = segments.next();
    let branch = if next_segment == Some("tree") || next_segment == Some("blob") {
        segments.next().map(|b| b.to_string())
    } else {
        None
    };
    
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    
    Some((owner, repo, branch))
}

/// Recursively walk the directory, filtering files by extension and path exclusion
fn visit_dirs(
    dir: &Path,
    base_dir: &Path,
    include_exts: &[String],
    exclude_paths: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            // Check if path should be excluded
            if let Ok(rel_path) = path.strip_prefix(base_dir) {
                if exclude_paths.iter().any(|ex| {
                    rel_path.components().any(|c| c.as_os_str() == ex.as_str())
                }) {
                    continue;
                }
            }

            if path.is_dir() {
                visit_dirs(&path, base_dir, include_exts, exclude_paths, files)?;
            } else if path.is_file() {
                // Check file size first
                if let Ok(metadata) = fs::metadata(&path) {
                    if metadata.len() > MAX_FILE_SIZE_BYTES {
                        continue;
                    }
                }
                
                // Filter by extension
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if include_exts.iter().any(|ie| ie.to_lowercase() == ext_lower) {
                        files.push(path);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Clone a repository using git and index its contents
pub async fn clone_and_index_repo(
    index: &SearchIndex,
    graph: &Mutex<KnowledgeGraph>,
    url: &str,
    branch: Option<&str>,
    include_exts: Option<&[String]>,
    exclude_paths: Option<&[String]>,
) -> Result<String, SearchXyzError> {
    // 1. Parse GitHub URL
    let (owner, repo_name, parsed_branch) = parse_github_url(url).ok_or_else(|| {
        SearchXyzError::CrawlFailed {
            url: url.to_string(),
            reason: "Invalid GitHub repository URL".to_string(),
        }
    })?;
    
    let resolved_branch = branch.or(parsed_branch.as_deref());
    let repo_base_url = format!("https://github.com/{}/{}", owner, repo_name);
    
    // 2. Prepare temp directory
    let rand_val: u32 = rand::rng().random();
    let temp_dir = std::env::temp_dir().join(format!("searchxyz-git-{}-{}", repo_name, rand_val));
    
    // Ensure no name collisions
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
    }
    
    // 3. Clone Repository
    let mut cmd = Command::new("git");
    cmd.arg("clone")
       .arg("--depth")
       .arg("1");
    
    if let Some(b) = resolved_branch {
        // Validate branch string to prevent command injection
        if !b.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/') {
            return Err(SearchXyzError::CrawlFailed {
                url: url.to_string(),
                reason: format!("Invalid/unsafe branch name: {}", b),
            });
        }
        cmd.arg("--branch").arg(b);
    }
    
    cmd.arg(&repo_base_url)
       .arg(&temp_dir);
    
    tracing::info!(repo = %repo_base_url, branch = ?resolved_branch, dest = ?temp_dir, "Cloning repository");
    let output = cmd.output().map_err(|e| SearchXyzError::CrawlFailed {
        url: url.to_string(),
        reason: format!("Failed to spawn git command: {}", e),
    })?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SearchXyzError::CrawlFailed {
            url: url.to_string(),
            reason: format!("git clone failed: {}", stderr.trim()),
        });
    }
    
    // 4. Traverse files
    let include_exts_vec: Vec<String> = match include_exts {
        Some(exts) => exts.iter().map(|s| s.to_string()).collect(),
        None => DEFAULT_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
    };
    
    let exclude_paths_vec: Vec<String> = match exclude_paths {
        Some(paths) => paths.iter().map(|s| s.to_string()).collect(),
        None => DEFAULT_EXCLUDE_PATHS.iter().map(|s| s.to_string()).collect(),
    };
    
    let mut file_paths = Vec::new();
    if let Err(e) = visit_dirs(&temp_dir, &temp_dir, &include_exts_vec, &exclude_paths_vec, &mut file_paths) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(SearchXyzError::CrawlFailed {
            url: url.to_string(),
            reason: format!("Directory traversal failed: {}", e),
        });
    }
    
    // 5. Index each file and compile summary
    let branch_for_url = resolved_branch.unwrap_or("main");
    let mut indexed_count = 0;
    let mut readme_content = String::new();
    let mut file_list_summary = String::new();
    
    for path in &file_paths {
        let rel_path = match path.strip_prefix(&temp_dir) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let rel_path_str = rel_path.to_string_lossy().to_string();
        
        let file_content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => continue, // Skip binary files or unreadable files silently
        };
        
        let file_url = format!("{}/blob/{}/{}", repo_base_url, branch_for_url, rel_path_str);
        let file_title = format!("{} ({})", rel_path_str, repo_name);
        
        // Format content as Markdown
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("txt");
        let formatted_markdown = format!(
            "### File: `{}`\n\n```{}\n{}\n```",
            rel_path_str, ext, file_content
        );
        
        let extracted = ExtractedContent {
            url: file_url.clone(),
            title: file_title.clone(),
            description: format!("Source code file from {} repository", repo_name),
            content_markdown: formatted_markdown,
            links: Vec::new(),
        };
        
        // Add to search index
        if let Err(e) = index.add_document(&extracted, "github").await {
            tracing::warn!(url = %file_url, error = %e, "Failed to index GitHub file (non-fatal)");
        }
        
        // Extract Knowledge Graph heuristics
        {
            let mut g = graph.lock().await;
            g.extract_heuristics(&file_url, &file_title, &file_content);
        }
        
        // Capture README content for primary summary
        let file_name_lower = rel_path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_lowercase();
        if file_name_lower == "readme.md" && rel_path.parent() == Some(Path::new("")) {
            readme_content = file_content.clone();
        }
        
        file_list_summary.push_str(&format!("- [`{}`]({})\n", rel_path_str, file_url));
        indexed_count += 1;
    }
    
    // Clean up temporary directory
    let _ = fs::remove_dir_all(&temp_dir);
    
    // 6. Build the final response summary
    let mut summary = format!(
        "# Codebase Ingested: {}/{}\n\n\
        **Source:** {}\n\
        **Branch:** {}\n\
        **Files Indexed:** {} matching files successfully parsed and added to search index.\n\n",
        owner, repo_name, repo_base_url, branch_for_url, indexed_count
    );
    
    if !readme_content.is_empty() {
        summary.push_str("## Repository README\n\n");
        summary.push_str(&readme_content);
        summary.push_str("\n\n---\n\n");
    }
    
    summary.push_str("## Ingested Codebase Files\n\n");
    summary.push_str(&file_list_summary);
    
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_github_url() {
        assert_eq!(
            parse_github_url("https://github.com/tokio-rs/tokio"),
            Some(("tokio-rs".to_string(), "tokio".to_string(), None))
        );
        assert_eq!(
            parse_github_url("https://github.com/tokio-rs/tokio.git"),
            Some(("tokio-rs".to_string(), "tokio".to_string(), None))
        );
        assert_eq!(
            parse_github_url("https://github.com/tokio-rs/tokio/tree/v1.36.0"),
            Some(("tokio-rs".to_string(), "tokio".to_string(), Some("v1.36.0".to_string())))
        );
        assert_eq!(
            parse_github_url("https://github.com/tokio-rs/tokio/blob/main/src/lib.rs"),
            Some(("tokio-rs".to_string(), "tokio".to_string(), Some("main".to_string())))
        );
        assert_eq!(parse_github_url("https://google.com"), None);
    }

    #[test]
    fn test_visit_dirs() {
        let temp = std::env::temp_dir().join("test_visit_dirs_searchxyz");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();
        
        let src = temp.join("src");
        fs::create_dir_all(&src).unwrap();
        
        fs::write(src.join("main.rs"), "fn main() {}").unwrap();
        fs::write(src.join("helper.rs"), "fn helper() {}").unwrap();
        fs::write(temp.join("README.md"), "# Test Repo").unwrap();
        
        // Ignored path
        let target = temp.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("output.rs"), "some compiled output").unwrap();
        
        let include_exts: Vec<String> = vec!["rs".to_string(), "md".to_string()];
        let exclude_paths: Vec<String> = vec!["target".to_string()];
        
        let mut files = Vec::new();
        visit_dirs(&temp, &temp, &include_exts, &exclude_paths, &mut files).unwrap();
        
        let mut file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        file_names.sort();
        
        assert_eq!(file_names, vec!["README.md".to_string(), "helper.rs".to_string(), "main.rs".to_string()]);
        
        let _ = fs::remove_dir_all(&temp);
    }
}
