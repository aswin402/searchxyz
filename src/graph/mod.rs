use std::collections::{HashMap, HashSet};
use std::path::Path;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct Node {
    pub name: String,
    pub entity_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub relationship_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct KnowledgeGraph {
    pub nodes: HashMap<String, Node>,
    pub edges: Vec<Edge>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// Add or update an entity node.
    pub fn add_node(&mut self, name: String, entity_type: String) {
        // If node already exists, preserve its details or update type if empty.
        self.nodes.entry(name.clone()).or_insert(Node {
            name,
            entity_type,
        });
    }

    /// Add a relationship edge, ensuring both endpoint nodes exist.
    pub fn add_edge(&mut self, source: String, source_type: String, target: String, target_type: String, relationship_type: String) {
        self.add_node(source.clone(), source_type);
        self.add_node(target.clone(), target_type);

        let new_edge = Edge {
            source,
            target,
            relationship_type,
        };

        if !self.edges.contains(&new_edge) {
            self.edges.push(new_edge);
        }
    }

    /// Find a node case-insensitively.
    pub fn find_canonical_node_name(&self, name: &str) -> Option<String> {
        if self.nodes.contains_key(name) {
            return Some(name.to_string());
        }
        let lower = name.to_lowercase();
        for k in self.nodes.keys() {
            if k.to_lowercase() == lower {
                return Some(k.clone());
            }
        }
        None
    }

    /// Query neighbors of a node using BFS up to a maximum depth.
    pub fn query_neighbors(&self, start_entity: &str, max_depth: usize) -> (Vec<Node>, Vec<Edge>) {
        let mut visited_nodes = HashSet::new();
        let mut visited_edges = HashSet::new();

        let canonical_name = match self.find_canonical_node_name(start_entity) {
            Some(name) => name,
            None => return (Vec::new(), Vec::new()),
        };

        let mut current_level = vec![canonical_name.clone()];
        visited_nodes.insert(canonical_name);

        for _ in 0..max_depth {
            let mut next_level = Vec::new();
            for node in &current_level {
                for edge in &self.edges {
                    if edge.source == *node {
                        if visited_nodes.insert(edge.target.clone()) {
                            next_level.push(edge.target.clone());
                        }
                        visited_edges.insert(edge.clone());
                    } else if edge.target == *node {
                        if visited_nodes.insert(edge.source.clone()) {
                            next_level.push(edge.source.clone());
                        }
                        visited_edges.insert(edge.clone());
                    }
                }
            }
            if next_level.is_empty() {
                break;
            }
            current_level = next_level;
        }

        let nodes = visited_nodes
            .into_iter()
            .filter_map(|name| self.nodes.get(&name).cloned())
            .collect();
        let edges = visited_edges.into_iter().collect();

        (nodes, edges)
    }

    /// Extract key technologies/topics from content and link them to the Document node.
    pub fn extract_heuristics(&mut self, url: &str, _title: &str, content: &str) {
        // List of technical keywords we look for (case-insensitive).
        let keywords = [
            "rust", "python", "javascript", "typescript", "go", "java", "c++",
            "docker", "kubernetes", "aws", "gcp", "azure", "sqlite", "postgresql",
            "mysql", "mongodb", "redis", "elasticsearch", "tokio", "actix",
            "axum", "fastapi", "django", "flask", "react", "vue", "next.js",
            "tailwind", "npm", "cargo", "git", "github", "linux", "windows",
            "macos", "mcp", "tantivy", "fastembed", "neural", "embedding",
            "search", "crawl", "vector", "semantic", "llm", "ai", "gemini",
        ];

        let content_lower = content.to_lowercase();
        let mut matched = Vec::new();

        for kw in &keywords {
            // Check for keyword with word boundaries
            let kw_pattern = format!(" {}", kw);
            let kw_pattern_start = format!("{}:", kw); // e.g. for lists
            if content_lower.contains(&kw_pattern) || content_lower.starts_with(kw) || content_lower.contains(&kw_pattern_start) {
                matched.push(*kw);
            }
        }

        if !matched.is_empty() {
            // Add the document node
            let doc_name = url.to_string();
            self.add_node(doc_name.clone(), "Document".to_string());
            
            for term in matched {
                let canonical_term = capitalize_keyword(term);
                self.add_edge(
                    doc_name.clone(),
                    "Document".to_string(),
                    canonical_term,
                    "Technology/Topic".to_string(),
                    "mentions".to_string(),
                );
            }
        }
    }

    /// Load the graph from a JSON file.
    pub fn load_from_file(path: &Path) -> Result<Self, std::io::Error> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let graph = serde_json::from_reader(reader)?;
        Ok(graph)
    }

    /// Save the graph to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }
}

fn capitalize_keyword(kw: &str) -> String {
    match kw {
        "rust" => "Rust".to_string(),
        "python" => "Python".to_string(),
        "javascript" => "JavaScript".to_string(),
        "typescript" => "TypeScript".to_string(),
        "go" => "Go".to_string(),
        "java" => "Java".to_string(),
        "c++" => "C++".to_string(),
        "docker" => "Docker".to_string(),
        "kubernetes" => "Kubernetes".to_string(),
        "aws" => "AWS".to_string(),
        "gcp" => "GCP".to_string(),
        "azure" => "Azure".to_string(),
        "sqlite" => "SQLite".to_string(),
        "postgresql" => "PostgreSQL".to_string(),
        "mysql" => "MySQL".to_string(),
        "mongodb" => "MongoDB".to_string(),
        "redis" => "Redis".to_string(),
        "elasticsearch" => "Elasticsearch".to_string(),
        "tokio" => "Tokio".to_string(),
        "actix" => "Actix".to_string(),
        "axum" => "Axum".to_string(),
        "fastapi" => "FastAPI".to_string(),
        "django" => "Django".to_string(),
        "flask" => "Flask".to_string(),
        "react" => "React".to_string(),
        "vue" => "Vue".to_string(),
        "next.js" => "Next.js".to_string(),
        "tailwind" => "Tailwind".to_string(),
        "npm" => "NPM".to_string(),
        "cargo" => "Cargo".to_string(),
        "git" => "Git".to_string(),
        "github" => "GitHub".to_string(),
        "linux" => "Linux".to_string(),
        "windows" => "Windows".to_string(),
        "macos" => "macOS".to_string(),
        "mcp" => "MCP".to_string(),
        "tantivy" => "Tantivy".to_string(),
        "fastembed" => "Fastembed".to_string(),
        "llm" => "LLM".to_string(),
        "ai" => "AI".to_string(),
        "gemini" => "Gemini".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_crud() {
        let mut graph = KnowledgeGraph::new();
        graph.add_edge(
            "Tokio".to_string(),
            "Library".to_string(),
            "Rust".to_string(),
            "Language".to_string(),
            "written_in".to_string(),
        );

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].relationship_type, "written_in");
    }

    #[test]
    fn test_graph_bfs_neighbors() {
        let mut graph = KnowledgeGraph::new();
        graph.add_edge("A".into(), "Type".into(), "B".into(), "Type".into(), "link".into());
        graph.add_edge("B".into(), "Type".into(), "C".into(), "Type".into(), "link".into());
        graph.add_edge("C".into(), "Type".into(), "D".into(), "Type".into(), "link".into());

        // Depth 1 from B should reach A and C
        let (nodes, edges) = graph.query_neighbors("B", 1);
        let mut names: Vec<String> = nodes.into_iter().map(|n| n.name).collect();
        names.sort();
        assert_eq!(names, vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        assert_eq!(edges.len(), 2);

        // Case-insensitive query
        let (nodes_ci, _) = graph.query_neighbors("b", 1);
        assert_eq!(nodes_ci.len(), 3);

        // Depth 2 from A should reach B and C
        let (nodes_a, _) = graph.query_neighbors("A", 2);
        assert_eq!(nodes_a.len(), 3); // A, B, C
    }

    #[test]
    fn test_extract_heuristics() {
        let mut graph = KnowledgeGraph::new();
        graph.extract_heuristics(
            "https://example.com/rust-async",
            "Async Rust",
            "This article describes building async systems in Rust using the Tokio runtime.",
        );

        assert!(graph.nodes.contains_key("https://example.com/rust-async"));
        assert!(graph.nodes.contains_key("Rust"));
        assert!(graph.nodes.contains_key("Tokio"));

        // Mentions relationship
        let has_rust_edge = graph.edges.iter().any(|e| e.source == "https://example.com/rust-async" && e.target == "Rust");
        assert!(has_rust_edge);
    }
}
