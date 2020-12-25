use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use zksync_storage::StorageProcessor;
use zksync_types::tokens::TokenMarketVolume;
use zksync_types::{Token, TokenId, TokenLike};

#[derive(Debug, Clone, Default)]
pub struct TokenDBCache {
    // TODO: handle stale entries, edge case when we rename token after adding it (ZKS-97)
    cache: Arc<RwLock<HashMap<TokenLike, Token>>>,
}

impl TokenDBCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_token(
        &self,
        storage: &mut StorageProcessor<'_>,
        token_query: impl Into<TokenLike>,
    ) -> anyhow::Result<Option<Token>> {
        let token_query = token_query.into();
        // HACK: Special case for the Golem:
        //
        // Currently, their token on Rinkeby is called GNT, but it's being renamed to the GLM.
        // So, for some period of time, we should consider GLM token name as an alias to the GNT token.
        //
        // TODO: Remove this case after Golem update [ZKS-173]
        match token_query {
            TokenLike::Symbol(symbol) if symbol == "tGLM" => {
                // Try to lookup Golem token as "tGLM".
                if let Some(token) = self
                    .get_token_impl(storage, TokenLike::Symbol(symbol))
                    .await?
                {
                    // If such token exists, use it.
                    Ok(Some(token))
                } else {
                    // Otherwise to lookup Golem token as "GNT".
                    self.get_token_impl(storage, TokenLike::Symbol("GNT".to_string()))
                        .await
                }
            }
            other => self.get_token_impl(storage, other).await,
        }
    }

    async fn get_token_impl(
        &self,
        storage: &mut StorageProcessor<'_>,
        token_query: TokenLike,
    ) -> anyhow::Result<Option<Token>> {
        // Just return token from cache.
        if let Some(token) = self.cache.read().await.get(&token_query) {
            return Ok(Some(token.clone()));
        }
        // Tries to fetch token from the underlying database.
        let token = {
            storage
                .tokens_schema()
                .get_token(token_query.clone())
                .await?
        };
        // Stores received token into the local cache.
        if let Some(token) = &token {
            self.cache.write().await.insert(token_query, token.clone());
        }

        Ok(token)
    }

    pub async fn token_symbol(
        &self,
        storage: &mut StorageProcessor<'_>,
        token_id: TokenId,
    ) -> anyhow::Result<Option<String>> {
        let token = self.get_token(storage, token_id).await?;
        Ok(token.map(|token| token.symbol))
    }

    pub async fn get_all_tokens(
        storage: &mut StorageProcessor<'_>,
    ) -> Result<Vec<Token>, anyhow::Error> {
        let tokens = storage.tokens_schema().load_tokens().await?;
        Ok(tokens.into_iter().map(|(_k, v)| v).collect())
    }

    pub async fn get_token_market_volume(
        storage: &mut StorageProcessor<'_>,
        token: TokenId,
    ) -> anyhow::Result<Option<TokenMarketVolume>> {
        let volume = storage
            .tokens_schema()
            .get_token_market_volume(token)
            .await?;
        Ok(volume)
    }

    pub async fn update_token_market_volume(
        storage: &mut StorageProcessor<'_>,
        token: TokenId,
        market: TokenMarketVolume,
    ) -> anyhow::Result<()> {
        Ok(storage
            .tokens_schema()
            .update_token_market_volume(token, market)
            .await?)
    }
}
