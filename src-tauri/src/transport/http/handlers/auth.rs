use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use secrecy::{ExposeSecret, SecretString};

use crate::application::identity::{
    Account, AuthenticationResult, IdentityApplicationService, IdentityError,
};
use crate::transport::http::HttpState;
use crate::transport::http::auth::AuthenticatedActor;
use crate::transport::http::dto::auth::{
    GuestLoginRequest, LoginRequest, RegisterRequest, TokenResponse, UserResponse,
};
use crate::transport::http::error::AppError;

pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/guest", post(guest_login))
        .route("/api/auth/register", post(register))
        .route("/api/auth/me", get(me))
}

fn user(account: Account) -> UserResponse {
    UserResponse {
        id: account.id,
        username: account.username,
        email: account.email,
        avatar: account.avatar,
        role: account.role,
    }
}

fn response(result: AuthenticationResult) -> Json<TokenResponse> {
    Json(TokenResponse {
        access_token: result.access_token.expose_secret().to_owned(),
        token_type: "bearer".into(),
        user: user(result.account),
    })
}

fn error(error: IdentityError) -> AppError {
    match error {
        IdentityError::Forbidden => AppError::Forbidden,
        _ => AppError::Unauthorized,
    }
}

async fn login(
    State(auth): State<Arc<IdentityApplicationService>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    auth.login(&request.username, SecretString::new(request.password))
        .await
        .map(response)
        .map_err(|error| match error {
            IdentityError::Forbidden => AppError::Forbidden,
            _ => AppError::BadRequest("用户名或密码错误".into()),
        })
}

async fn guest_login(
    State(auth): State<Arc<IdentityApplicationService>>,
    Json(request): Json<GuestLoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let device_id = request.device_id.trim();
    if device_id.len() < 16
        || device_id.len() > 128
        || !device_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(AppError::BadRequest("设备标识无效".into()));
    }
    auth.guest(device_id).await.map(response).map_err(error)
}

async fn register(
    State(auth): State<Arc<IdentityApplicationService>>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    if request.username.len() < 3 {
        return Err(AppError::BadRequest("用户名至少 3 个字符".into()));
    }
    if request.password.len() < 6 {
        return Err(AppError::BadRequest("密码至少 6 个字符".into()));
    }
    auth.register(
        &request.username,
        request.email.as_deref(),
        SecretString::new(request.password),
    )
    .await
    .map(response)
    .map_err(|error| match error {
        IdentityError::Forbidden => AppError::Forbidden,
        _ => AppError::BadRequest("用户名已存在".into()),
    })
}

async fn me(
    State(auth): State<Arc<IdentityApplicationService>>,
    actor: AuthenticatedActor,
) -> Result<Json<UserResponse>, AppError> {
    auth.current(&actor.0)
        .await
        .map(user)
        .map(Json)
        .map_err(error)
}
