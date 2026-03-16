//! LSP server implementation.
//!
//! Implements the Language Server Protocol for the Nix module system.

use super::{Capabilities, CompletionProvider, HoverProvider, OptionRegistry};
use crate::eval::{EvalResult, OptionInfo};
use crate::types::OptionPath;
use lsp_types::{
    CompletionOptions, HoverProviderCapability, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Document state for tracking open files
#[derive(Debug, Clone)]
pub struct DocumentState {
    /// Document URI
    pub uri: String,
    /// Document content
    pub content: String,
    /// Document version
    pub version: i32,
    /// Parsed AST (if available)
    pub ast: Option<crate::parse::Spanned<crate::parse::Expr>>,
}

impl DocumentState {
    /// Create a new document state
    pub fn new(uri: String, content: String, version: i32) -> Self {
        Self {
            uri,
            content,
            version,
            ast: None,
        }
    }

    /// Get a line from the document
    pub fn get_line(&self, line: u32) -> Option<&str> {
        self.content.lines().nth(line as usize)
    }

    /// Get word at position
    pub fn word_at(&self, line: u32, column: u32) -> Option<String> {
        let line_content = self.get_line(line)?;
        let col = column as usize;

        if col > line_content.len() {
            return None;
        }

        // Find word boundaries
        let start = line_content[..col]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '-')
            .map(|i| i + 1)
            .unwrap_or(0);

        let end = line_content[col..]
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '-')
            .map(|i| col + i)
            .unwrap_or(line_content.len());

        if start < end {
            Some(line_content[start..end].to_string())
        } else {
            None
        }
    }
}

/// LSP server state
pub struct LspServer {
    /// Server capabilities
    capabilities: Capabilities,
    /// Completion provider
    completion: CompletionProvider,
    /// Hover provider
    hover: HoverProvider,
    /// Option registry (shared between providers)
    registry: Arc<RwLock<OptionRegistry>>,
    /// Open documents
    documents: HashMap<String, DocumentState>,
    /// Workspace root
    workspace_root: Option<PathBuf>,
}

impl LspServer {
    /// Create a new LSP server
    pub fn new(capabilities: Capabilities) -> Self {
        let registry = Arc::new(RwLock::new(OptionRegistry::new()));

        Self {
            completion: CompletionProvider::new(capabilities.clone()),
            hover: HoverProvider::new(capabilities.clone()),
            capabilities,
            registry,
            documents: HashMap::new(),
            workspace_root: None,
        }
    }

    /// Create with initial options
    pub fn with_options(capabilities: Capabilities, options: indexmap::IndexMap<OptionPath, OptionInfo>) -> Self {
        let registry = Arc::new(RwLock::new(OptionRegistry::from_options(options)));

        let completion = CompletionProvider::with_registry(
            capabilities.clone(),
            registry.read().unwrap().clone(),
        );
        let hover = HoverProvider::with_registry(
            capabilities.clone(),
            registry.read().unwrap().clone(),
        );

        Self {
            completion,
            hover,
            capabilities,
            registry,
            documents: HashMap::new(),
            workspace_root: None,
        }
    }

    /// Set workspace root
    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
    }

    /// Get server capabilities for the initialize response
    pub fn server_capabilities(&self) -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            completion_provider: if self.capabilities.completion {
                Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "=".to_string()]),
                    resolve_provider: Some(true),
                    ..Default::default()
                })
            } else {
                None
            },
            hover_provider: if self.capabilities.hover {
                Some(HoverProviderCapability::Simple(true))
            } else {
                None
            },
            ..Default::default()
        }
    }

    /// Update the option registry from evaluation result
    pub fn update_from_eval(&mut self, result: &EvalResult) {
        let new_registry = OptionRegistry::from_options(result.options.clone());

        // Update shared registry
        if let Ok(mut registry) = self.registry.write() {
            *registry = new_registry.clone();
        }

        // Update providers
        self.completion.set_registry(new_registry.clone());
        self.hover.set_registry(new_registry);
    }

    /// Register an option
    pub fn register_option(&mut self, info: OptionInfo) {
        if let Ok(mut registry) = self.registry.write() {
            registry.register(info.clone());

            // Update providers
            let new_registry = registry.clone();
            self.completion.set_registry(new_registry.clone());
            self.hover.set_registry(new_registry);
        }
    }

    /// Get the completion provider
    pub fn completion(&self) -> &CompletionProvider {
        &self.completion
    }

    /// Get the hover provider
    pub fn hover(&self) -> &HoverProvider {
        &self.hover
    }

    /// Open a document
    pub fn open_document(&mut self, uri: String, content: String, version: i32) {
        let state = DocumentState::new(uri.clone(), content, version);
        self.documents.insert(uri, state);
    }

    /// Update a document
    pub fn update_document(&mut self, uri: &str, content: String, version: i32) {
        if let Some(doc) = self.documents.get_mut(uri) {
            doc.content = content;
            doc.version = version;
            doc.ast = None; // Invalidate AST
        }
    }

    /// Close a document
    pub fn close_document(&mut self, uri: &str) {
        self.documents.remove(uri);
    }

    /// Get a document
    pub fn get_document(&self, uri: &str) -> Option<&DocumentState> {
        self.documents.get(uri)
    }

    /// Handle completion request
    pub fn handle_completion(
        &self,
        uri: &str,
        line: u32,
        column: u32,
    ) -> Vec<super::OptionCompletion> {
        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return Vec::new(),
        };

        let line_content = doc.get_line(line).unwrap_or("");
        let line_prefix = if column as usize <= line_content.len() {
            &line_content[..column as usize]
        } else {
            line_content
        };

        let ctx = super::CompletionContext::new(line, column)
            .with_line_prefix(line_prefix.to_string());

        self.completion.complete(&ctx)
    }

    /// Handle hover request
    pub fn handle_hover(
        &self,
        uri: &str,
        line: u32,
        column: u32,
    ) -> Option<super::HoverInfo> {
        let doc = self.documents.get(uri)?;

        let line_content = doc.get_line(line)?;

        let ctx = super::HoverContext::new(line, column)
            .with_line_content(line_content.to_string())
            .with_word(doc.word_at(line, column).unwrap_or_default());

        self.hover.hover_at(&ctx)
    }
}

impl Clone for OptionRegistry {
    fn clone(&self) -> Self {
        OptionRegistry::from_options(self.all_options().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Value;

    #[test]
    fn test_server_capabilities() {
        let server = LspServer::new(Capabilities::all());
        let caps = server.server_capabilities();
        assert!(caps.completion_provider.is_some());
        assert!(caps.hover_provider.is_some());
    }

    #[test]
    fn test_document_state() {
        let doc = DocumentState::new(
            "file:///test.nix".to_string(),
            "services.nginx.enable = true;\nusers.users.test = {};".to_string(),
            1,
        );

        assert_eq!(doc.get_line(0), Some("services.nginx.enable = true;"));
        assert_eq!(doc.get_line(1), Some("users.users.test = {};"));
        assert_eq!(doc.get_line(2), None);
    }

    #[test]
    fn test_word_at() {
        let doc = DocumentState::new(
            "file:///test.nix".to_string(),
            "services.nginx.enable = true;".to_string(),
            1,
        );

        // "services" starts at 0
        assert_eq!(doc.word_at(0, 3), Some("services.nginx.enable".to_string()));
        assert_eq!(doc.word_at(0, 9), Some("services.nginx.enable".to_string()));
    }

    #[test]
    fn test_server_with_options() {
        let mut options = indexmap::IndexMap::new();
        options.insert(
            OptionPath::from_dotted("test.enable"),
            OptionInfo {
                path: OptionPath::from_dotted("test.enable"),
                type_desc: "bool".to_string(),
                default: Some(Value::Bool(false)),
                description: Some("Test option".to_string()),
                declared_in: vec![],
                internal: false,
            },
        );

        let server = LspServer::with_options(Capabilities::all(), options);

        // Should be able to get hover for the option
        let path = OptionPath::from_dotted("test.enable");
        let info = server.hover().hover(&path);
        assert!(info.is_some());
    }

    #[test]
    fn test_open_close_document() {
        let mut server = LspServer::new(Capabilities::all());

        server.open_document(
            "file:///test.nix".to_string(),
            "test content".to_string(),
            1,
        );

        assert!(server.get_document("file:///test.nix").is_some());

        server.close_document("file:///test.nix");
        assert!(server.get_document("file:///test.nix").is_none());
    }
}
