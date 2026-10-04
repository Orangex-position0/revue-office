use std::sync::Arc;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};

use crate::application::identity::{
    Account, AccountGateway, AccountRepository, Actor, ActorId, ActorKind, AuthenticationResult,
    IdentityAdapter, IdentityError, IdentityPolicy, IdentityProof,
};

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Claims {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub exp: usize,
}

#[async_trait]
pub trait IdentityUserLookup: Send + Sync {
    async fn find(&self, id: &str) -> Result<Option<Actor>, IdentityError>;
}

pub struct DatabaseUserLookup(pub Arc<dyn AccountRepository>);

#[async_trait]
impl IdentityUserLookup for DatabaseUserLookup {
    async fn find(&self, id: &str) -> Result<Option<Actor>, IdentityError> {
        self.0
            .find_by_id(id)
            .await
            .map(|account| account.map(|account| actor_from_account(&account)))
    }
}

fn actor_from_account(account: &Account) -> Actor {
    Actor {
        id: ActorId(account.id.clone()),
        kind: if account.username.starts_with("guest_") {
            ActorKind::Guest
        } else {
            ActorKind::User
        },
        roles: vec![account.role.clone()],
    }
}

pub struct JwtIdentityAdapter {
    secret: SecretString,
    policy: IdentityPolicy,
    lookup: Arc<dyn IdentityUserLookup>,
}

impl JwtIdentityAdapter {
    pub fn new(
        secret: SecretString,
        policy: IdentityPolicy,
        lookup: Arc<dyn IdentityUserLookup>,
    ) -> Self {
        Self {
            secret,
            policy,
            lookup,
        }
    }
}

#[async_trait]
impl IdentityAdapter for JwtIdentityAdapter {
    async fn authenticate(&self, proof: IdentityProof) -> Result<Actor, IdentityError> {
        let claims = jsonwebtoken::decode::<Claims>(
            proof.0.expose_secret(),
            &jsonwebtoken::DecodingKey::from_secret(self.secret.expose_secret().as_bytes()),
            &jsonwebtoken::Validation::default(),
        )
        .map_err(|_| IdentityError::Unauthenticated)?
        .claims;
        let actor = self
            .lookup
            .find(&claims.sub)
            .await?
            .ok_or(IdentityError::Unauthenticated)?;
        if actor.kind == ActorKind::Guest && self.policy == IdentityPolicy::JwtOnly {
            return Err(IdentityError::Forbidden);
        }
        Ok(actor)
    }
}

pub struct AuthEndpoints {
    accounts: Arc<dyn AccountRepository>,
    policy: IdentityPolicy,
    signing_secret: SecretString,
    expiry_hours: i64,
}

impl AuthEndpoints {
    pub fn new(
        accounts: Arc<dyn AccountRepository>,
        policy: IdentityPolicy,
        signing_secret: SecretString,
        expiry_hours: i64,
    ) -> Self {
        Self {
            accounts,
            policy,
            signing_secret,
            expiry_hours,
        }
    }

    fn result(&self, account: Account) -> Result<AuthenticationResult, IdentityError> {
        let claims = Claims {
            sub: account.id.clone(),
            username: account.username.clone(),
            role: account.role.clone(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(self.expiry_hours)).timestamp()
                as usize,
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(self.signing_secret.expose_secret().as_bytes()),
        )
        .map_err(|_| IdentityError::Unauthenticated)?;
        Ok(AuthenticationResult {
            account,
            access_token: SecretString::new(token),
        })
    }
}

#[async_trait]
impl AccountGateway for AuthEndpoints {
    async fn login(
        &self,
        username: &str,
        password: SecretString,
    ) -> Result<AuthenticationResult, IdentityError> {
        let record = self
            .accounts
            .find_by_username(username)
            .await?
            .ok_or(IdentityError::Unauthenticated)?;
        if !bcrypt::verify(
            password.expose_secret(),
            record.password_hash.expose_secret(),
        )
        .unwrap_or(false)
        {
            return Err(IdentityError::Unauthenticated);
        }
        if self.policy == IdentityPolicy::JwtOnly
            && actor_from_account(&record.account).kind == ActorKind::Guest
        {
            return Err(IdentityError::Forbidden);
        }
        self.result(record.account)
    }

    async fn guest(&self, device: &str) -> Result<AuthenticationResult, IdentityError> {
        if self.policy != IdentityPolicy::LocalGuest {
            return Err(IdentityError::Forbidden);
        }
        self.result(
            self.accounts
                .find_or_create_external(&format!("guest_{device}"))
                .await?,
        )
    }

    async fn register(
        &self,
        username: &str,
        email: Option<&str>,
        password: SecretString,
    ) -> Result<AuthenticationResult, IdentityError> {
        if self.accounts.find_by_username(username).await?.is_some() {
            return Err(IdentityError::Unauthenticated);
        }
        let password_hash = bcrypt::hash(password.expose_secret(), 10)
            .map(SecretString::new)
            .map_err(|_| IdentityError::Unauthenticated)?;
        self.result(self.accounts.create(username, email, password_hash).await?)
    }

    async fn current(&self, actor: &Actor) -> Result<Account, IdentityError> {
        self.accounts
            .find_by_id(&actor.id.0)
            .await?
            .ok_or(IdentityError::Unauthenticated)
    }
}
