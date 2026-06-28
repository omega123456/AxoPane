use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::provider::ProviderInvocation;

const TOKEN_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredInvocation {
    pub request_id: String,
    pub invocation: ProviderInvocation,
    pub expires_at: Instant,
}

#[derive(Default)]
pub struct NativeMenuTokenStore {
    state: Mutex<TokenStoreState>,
}

#[derive(Default)]
struct TokenStoreState {
    next_id: u64,
    tokens_by_request: HashMap<String, Vec<String>>,
    invocations_by_token: HashMap<String, StoredInvocation>,
}

impl NativeMenuTokenStore {
    pub fn clear_all(&self) {
        let mut state = self.state.lock().expect("native menu token store lock");
        state.tokens_by_request.clear();
        state.invocations_by_token.clear();
    }

    pub fn replace_request(&self, request_id: &str) {
        let mut state = self.state.lock().expect("native menu token store lock");
        prune_expired(&mut state);
        if let Some(tokens) = state.tokens_by_request.remove(request_id) {
            for token in tokens {
                state.invocations_by_token.remove(&token);
            }
        }
    }

    pub fn issue_token(&self, request_id: &str, invocation: ProviderInvocation) -> String {
        let mut state = self.state.lock().expect("native menu token store lock");
        prune_expired(&mut state);
        state.next_id += 1;
        let token = format!("native:{request_id}:{}", state.next_id);
        state
            .tokens_by_request
            .entry(request_id.to_string())
            .or_default()
            .push(token.clone());
        state.invocations_by_token.insert(
            token.clone(),
            StoredInvocation {
                request_id: request_id.to_string(),
                invocation,
                expires_at: Instant::now() + TOKEN_TTL,
            },
        );
        token
    }

    pub fn take(&self, token: &str) -> Option<StoredInvocation> {
        let mut state = self.state.lock().expect("native menu token store lock");
        prune_expired(&mut state);
        let stored = state.invocations_by_token.remove(token)?;
        if let Some(tokens) = state.tokens_by_request.get_mut(&stored.request_id) {
            tokens.retain(|candidate| candidate != token);
            if tokens.is_empty() {
                state.tokens_by_request.remove(&stored.request_id);
            }
        }
        Some(stored)
    }
}

fn prune_expired(state: &mut TokenStoreState) {
    let now = Instant::now();
    let expired: Vec<(String, String)> = state
        .invocations_by_token
        .iter()
        .filter(|(_, stored)| stored.expires_at <= now)
        .map(|(token, stored)| (token.clone(), stored.request_id.clone()))
        .collect();

    for (token, request_id) in expired {
        state.invocations_by_token.remove(&token);
        if let Some(tokens) = state.tokens_by_request.get_mut(&request_id) {
            tokens.retain(|candidate| candidate != &token);
            if tokens.is_empty() {
                state.tokens_by_request.remove(&request_id);
            }
        }
    }
}
