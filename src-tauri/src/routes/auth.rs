use crate::auth::middleware::AuthUser;
use crate::db::user_repo;
use crate::error::AppError;
use crate::models::{GuestLoginRequest, LoginRequest, RegisterRequest, TokenResponse};
use crate::state;
use axum::routing::{get, post};
use axum::{Json, Router};

pub fn router() -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/guest", post(guest_login))
        .route("/api/auth/register", post(register))
        .route("/api/auth/me", get(me))
}

async fn login(Json(req): Json<LoginRequest>) -> Result<Json<TokenResponse>, AppError> {
    let pool = state::db_pool();
    let (user, hash) = user_repo::find_by_username(&pool, &req.username)
        .await?
        .ok_or(AppError::BadRequest("用户名或密码错误".into()))?;

    if !user_repo::verify_password(&hash, &req.password) {
        return Err(AppError::BadRequest("用户名或密码错误".into()));
    }

    let token = crate::auth::create_token(&user)?;
    Ok(Json(TokenResponse {
        access_token: token,
        token_type: "bearer".into(),
        user,
    }))
}

async fn guest_login(Json(req): Json<GuestLoginRequest>) -> Result<Json<TokenResponse>, AppError> {
    let device_id = req.device_id.trim();
    if device_id.len() < 16
        || device_id.len() > 128
        || !device_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::BadRequest("设备标识无效".into()));
    }

    let pool = state::db_pool();
    let username = format!("guest_{device_id}");
    let user = user_repo::find_or_create_external(&pool, &username).await?;
    let token = crate::auth::create_token(&user)?;

    Ok(Json(TokenResponse {
        access_token: token,
        token_type: "bearer".into(),
        user,
    }))
}

async fn register(Json(req): Json<RegisterRequest>) -> Result<Json<TokenResponse>, AppError> {
    if req.username.len() < 3 {
        return Err(AppError::BadRequest("用户名至少 3 个字符".into()));
    }
    if req.password.len() < 6 {
        return Err(AppError::BadRequest("密码至少 6 个字符".into()));
    }

    let pool = state::db_pool();
    if user_repo::find_by_username(&pool, &req.username)
        .await?
        .is_some()
    {
        return Err(AppError::BadRequest("用户名已存在".into()));
    }

    let hash = user_repo::hash_password(&req.password)?;
    let user = user_repo::create(&pool, &req.username, req.email.as_deref(), &hash).await?;
    let token = crate::auth::create_token(&user)?;

    Ok(Json(TokenResponse {
        access_token: token,
        token_type: "bearer".into(),
        user,
    }))
}

async fn me(user: AuthUser) -> Result<Json<crate::models::User>, AppError> {
    Ok(Json(user.0))
}
