use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use dashmap::DashMap;
use ropey::Rope;
use gtk_ui::lexer::{Lexer, Token, TokenValue};

pub const LEGEND_TYPE: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::NUMBER,
    SemanticTokenType::STRING,
    SemanticTokenType::MACRO,
    SemanticTokenType::METHOD,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::TYPE,
    SemanticTokenType::CLASS,
    SemanticTokenType::OPERATOR
];

trait TokenExt {
    fn to_legend_type(&self) -> Option<u32>;
}

impl TokenExt for Token {
    fn to_legend_type(&self) -> Option<u32> {
        match &self.value {
            TokenValue::Bool(_) => {
                Some(LEGEND_TYPE.iter()
                    .position(|item| item == &SemanticTokenType::KEYWORD).unwrap() as u32)
            },
            TokenValue::Number(_) => {
                Some(LEGEND_TYPE.iter()
                    .position(|item| item == &SemanticTokenType::NUMBER).unwrap() as u32)
            },
            TokenValue::Setter(_) => {
                Some(LEGEND_TYPE.iter()
                    .position(|item| item == &SemanticTokenType::METHOD).unwrap() as u32)
            },
            TokenValue::String(_) => {
                Some(LEGEND_TYPE.iter()
                    .position(|item| item == &SemanticTokenType::STRING).unwrap() as u32)
            },
            TokenValue::Directive(_) => {
                Some(LEGEND_TYPE.iter()
                    .position(|item| item == &SemanticTokenType::MACRO).unwrap() as u32)
            },
            TokenValue::Definition(_) => {
                Some(LEGEND_TYPE.iter()
                    .position(|item| item == &SemanticTokenType::CLASS).unwrap() as u32)
            },
            TokenValue::Inherits => {
                Some(LEGEND_TYPE.iter()
                    .position(|item| item == &SemanticTokenType::OPERATOR).unwrap() as u32)
            },
            _ => None
        }
    }
}

#[derive(Debug)]
struct Backend {
    client: Client,
    document_map: DashMap<String, Rope>,
    token_map: DashMap<String, Vec<Token>>
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions::default()),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        SemanticTokensRegistrationOptions {
                            text_document_registration_options: {
                                TextDocumentRegistrationOptions {
                                    document_selector: Some(vec![DocumentFilter {
                                        language: Some("gui".to_string()),
                                        scheme: Some("file".to_string()),
                                        pattern: None,
                                    }]),
                                }
                            },
                            semantic_tokens_options: SemanticTokensOptions {
                                work_done_progress_options: WorkDoneProgressOptions::default(),
                                legend: SemanticTokensLegend {
                                    token_types: LEGEND_TYPE.clone().into(),
                                    token_modifiers: vec![],
                                },
                                range: Some(true),
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                            static_registration_options: StaticRegistrationOptions::default(),
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: params.text_document.text,
            version: params.text_document.version,
        })
        .await
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
            version: params.text_document.version,
        })
        .await
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem {
                label: "MyCoolLabel".to_string(),
                insert_text: Some("MyCoolText".to_string()),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some("MyCoolDetail".to_string()),
                ..Default::default()
            },
        ])))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        self.client
            .log_message(MessageType::LOG, "semantic_token_full")
            .await;
        let semantic_tokens = || -> Option<Vec<SemanticToken>> {
            let mut im_complete_tokens = self.token_map.get_mut(&uri)?;
            let rope = self.document_map.get(&uri)?;
            // let ast = self.ast_map.get(&uri)?;
            // let extends_tokens = semantic_token_from_ast(&ast);
            // im_complete_tokens.extend(extends_tokens);
            im_complete_tokens.sort_by(|a, b| a.range.start.cmp(&b.range.start));
            let mut pre_line = 0;
            let mut pre_start = 0;
            let semantic_tokens = im_complete_tokens
                .iter()
                .filter_map(|token| {
                    let line = rope.try_byte_to_line(token.range.start as usize).ok()? as u32;
                    let first = rope.try_line_to_char(line as usize).ok()? as u32;
                    let start = rope.try_byte_to_char(token.range.start as usize).ok()? as u32 - first;
                    let delta_line = line - pre_line;
                    let delta_start = if delta_line == 0 {
                        start - pre_start
                    } else {
                        start
                    };
                    if let Some(token_type) = token.to_legend_type() {
                        let ret = Some(SemanticToken {
                            delta_line,
                            delta_start,
                            length: (token.range.end - token.range.start) as u32,
                            token_modifiers_bitset: 0,
                            token_type,
                        });
                        pre_line = line;
                        pre_start = start;
                        ret
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            Some(semantic_tokens)
        }();
        if let Some(semantic_token) = semantic_tokens {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_token,
            })));
        }
        Ok(None)
    }
    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri.to_string();
        let semantic_tokens = || -> Option<Vec<SemanticToken>> {
            let im_complete_tokens = self.token_map.get(&uri)?;
            let rope = self.document_map.get(&uri)?;
            let mut pre_line = 0;
            let mut pre_start = 0;
            let semantic_tokens = im_complete_tokens
                .iter()
                .filter_map(|token| {
                    let line = rope.try_byte_to_line(token.range.start as usize).ok()? as u32;
                    let first = rope.try_line_to_char(line as usize).ok()? as u32;
                    let start = rope.try_byte_to_char(token.range.start as usize).ok()? as u32 - first;
                    if let Some(token_type) = token.to_legend_type() {
                        let ret = Some(SemanticToken {
                            delta_line: line - pre_line,
                            delta_start: if start >= pre_start {
                                start - pre_start
                            } else {
                                start
                            },
                            length: (token.range.end - token.range.start) as u32,
                            token_modifiers_bitset: 0,
                            token_type,
                        });
                        pre_line = line;
                        pre_start = start;
                        ret
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            Some(semantic_tokens)
        }();
        if let Some(semantic_token) = semantic_tokens {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_token,
            })));
        }
        Ok(None)
    }
}

struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32,
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let rope = Rope::from_str(&params.text);
        self.document_map.
            insert(params.uri.to_string(), rope.clone());

        let mut lexer = Lexer::new(params.text);
        if let Ok(_) = lexer.lex(true) {
            self.client
                .log_message(MessageType::INFO, "Successfully lexed!")
                .await;
        } else {
            self.client
                .log_message(MessageType::INFO, "Failed to lexed!")
                .await;
        }
        self.token_map.insert(params.uri.to_string(), lexer.tokens.clone());
        // self.client
        //     .log_message(MessageType::INFO, format!("{:?}", lexer.tokens))
        //     .await;
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: DashMap::new(),
        token_map: DashMap::new()
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
