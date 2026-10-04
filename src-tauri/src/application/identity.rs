use async_trait::async_trait;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    User,
    Guest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityPolicy {
    LocalGuest,
    JwtOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    pub id: ActorId,
    pub kind: ActorKind,
    pub roles: Vec<String>,
}

impl Actor {
    pub fn user(id: impl Into<String>) -> Self {
        Self {
            id: ActorId(id.into()),
            kind: ActorKind::User,
            roles: vec!["user".into()],
        }
    }
    pub fn owns(&self, owner: &str) -> bool {
        self.id.0 == owner
    }
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("forbidden")]
    Forbidden,
}

/// Public account semantics, independent of persistence and token protocols.
#[derive(Clone, Debug)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub role: String,
}

pub struct AuthenticationResult {
    pub account: Account,
    pub access_token: secrecy::SecretString,
}

/// Persistence-facing account record. The password hash remains opaque and
/// secret-bearing outside the identity infrastructure boundary.
pub struct AccountRecord {
    pub account: Account,
    pub password_hash: secrecy::SecretString,
}

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AccountRecord>, IdentityError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Account>, IdentityError>;
    async fn create(
        &self,
        username: &str,
        email: Option<&str>,
        password_hash: secrecy::SecretString,
    ) -> Result<Account, IdentityError>;
    async fn find_or_create_external(&self, username: &str) -> Result<Account, IdentityError>;
}

#[async_trait]
pub trait AccountGateway: Send + Sync {
    async fn login(
        &self,
        username: &str,
        password: secrecy::SecretString,
    ) -> Result<AuthenticationResult, IdentityError>;
    async fn guest(&self, device: &str) -> Result<AuthenticationResult, IdentityError>;
    async fn register(
        &self,
        username: &str,
        email: Option<&str>,
        password: secrecy::SecretString,
    ) -> Result<AuthenticationResult, IdentityError>;
    async fn current(&self, actor: &Actor) -> Result<Account, IdentityError>;
}

pub struct IdentityApplicationService {
    gateway: std::sync::Arc<dyn AccountGateway>,
}
impl IdentityApplicationService {
    pub fn new(gateway: std::sync::Arc<dyn AccountGateway>) -> Self {
        Self { gateway }
    }
    pub async fn login(
        &self,
        username: &str,
        password: secrecy::SecretString,
    ) -> Result<AuthenticationResult, IdentityError> {
        self.gateway.login(username, password).await
    }
    pub async fn guest(&self, device: &str) -> Result<AuthenticationResult, IdentityError> {
        self.gateway.guest(device).await
    }
    pub async fn register(
        &self,
        username: &str,
        email: Option<&str>,
        password: secrecy::SecretString,
    ) -> Result<AuthenticationResult, IdentityError> {
        self.gateway.register(username, email, password).await
    }
    pub async fn current(&self, actor: &Actor) -> Result<Account, IdentityError> {
        self.gateway.current(actor).await
    }
}

/// Opaque authentication proof; protocol interpretation belongs to the adapter.
/// Deliberately not Debug or Serialize.
pub struct IdentityProof(pub secrecy::SecretString);

#[async_trait]
pub trait IdentityAdapter: Send + Sync {
    async fn authenticate(&self, proof: IdentityProof) -> Result<Actor, IdentityError>;
}
