use crate::application::identity::{Actor, IdentityAdapter, IdentityError, IdentityProof};
use crate::transport::http::error::AppError;
use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::request::Parts,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct IdentityState(pub Arc<dyn IdentityAdapter>);

pub struct AuthenticatedActor(pub Actor);

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthenticatedActor
where
    S: Send + Sync,
    IdentityState: FromRef<S>,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let State(identity) = State::<IdentityState>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .filter(|s| !s.is_empty())
            .ok_or(AppError::Unauthorized)?;
        identity
            .0
            .authenticate(IdentityProof(secrecy::SecretString::new(token.to_owned())))
            .await
            .map(Self)
            .map_err(|e| match e {
                IdentityError::Forbidden => AppError::Forbidden,
                _ => AppError::Unauthorized,
            })
    }
}
